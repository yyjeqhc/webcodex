import { randomUUID } from "node:crypto";
import { mkdir, realpath } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createAgentSession, DefaultResourceLoader, getAgentDir, ModelRuntime, SessionManager, SettingsManager } from "@earendil-works/pi-coding-agent";
import { createAssistantMessageEventStream, createProvider, getCurrentTools } from "@earendil-works/pi-ai";
import { BridgeError, digest, requireCondition } from "./catalog.mjs";
import { cliBuiltins, declaredTools } from "./pi-compat.mjs";

const PROVIDER = "webcodex-tool-environment";
const MODEL = "native-tools";
const NO_MODEL_LOOP = { retry: { enabled: false }, compaction: { enabled: false } };

/** A local provider emits requested tool calls; Pi alone validates and executes them. */
export async function createPiEnvironment({ cwd, agentDir, stateDir }) {
  cwd = await realpath(cwd);
  agentDir = resolve(agentDir);
  // Built-in Pi extensions use getAgentDir(). The dedicated Plugin process sets
  // this once from operator CLI configuration, never from model arguments.
  requireCondition(resolve(getAgentDir()) === agentDir, "pi_agent_directory_mismatch");
  await mkdir(stateDir, { recursive: true, mode: 0o700 });
  const epoch = randomUUID();
  let pending;
  let closed = false;
  const stream = (model, context) => {
    const output = createAssistantMessageEventStream();
    const message = {
      role: "assistant", content: [], api: model.api, provider: model.provider, model: model.id,
      usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
      stopReason: "stop", timestamp: Date.now(),
    };
    queueMicrotask(() => {
      try {
        requireCondition(model.provider === PROVIDER && model.id === MODEL, "native_model_changed");
        requireCondition(pending && !pending.signal.aborted, "unrequested_or_cancelled_native_turn");
        if (!pending.emitted) {
          // Recheck after Pi's before_agent_start/loadout hooks, at the native
          // provider boundary. Never activate missing/hidden tools to make a call work.
          pending.beforeDispatch();
          const declarations = new Map(getCurrentTools(context.messages).map((tool) => [tool.name, tool]));
          for (const call of pending.calls) {
            const declared = declarations.get(call.name);
            requireCondition(declared && digest(declared.parameters) === digest(call.parameters), "native_loadout_changed");
          }
          message.content = pending.calls.map(({ id, name, arguments: args }) => ({ type: "toolCall", id, name, arguments: args }));
          message.stopReason = "toolUse";
          pending.emitted = true;
        }
        output.push({ type: "start", partial: message });
        output.push({ type: "done", reason: message.stopReason, message });
      } catch (error) {
        if (pending) pending.error = error;
        message.stopReason = "error";
        message.errorMessage = error instanceof BridgeError ? error.code : "native_provider_failure";
        output.push({ type: "error", reason: "error", error: message });
      } finally { output.end(); }
    });
    return output;
  };
  // The bridge never reads or delegates the user's LLM auth.json/models.json.
  // Native MCP/extension credentials remain in Pi's configured environment.
  const runtime = await ModelRuntime.create({
    authPath: join(stateDir, "bridge-auth.json"), modelsPath: null,
    modelsStorePath: join(stateDir, "model-store"), allowModelNetwork: false, refreshOnCreate: false,
  });
  runtime.registerNativeProvider(createProvider({
    id: PROVIDER, name: "WebCodex local tool-call provider",
    auth: { apiKey: { name: "Local bridge", async resolve() { return { auth: { headers: {} }, source: "local" }; } } },
    models: [{ id: MODEL, name: "Externally supplied tool calls", api: PROVIDER, provider: PROVIDER, baseUrl: "", reasoning: false, input: ["text", "image"], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 1_000_000, maxTokens: 100_000 }],
    api: { stream, streamSimple: stream },
  }));
  const settings = SettingsManager.create(cwd, agentDir);
  settings.applyOverrides(NO_MODEL_LOOP);
  const loader = new DefaultResourceLoader({ cwd, agentDir, settingsManager: settings, extensionFactories: await cliBuiltins() });
  await loader.reload();
  requireCondition(loader.getExtensions().errors.length === 0, "native_extension_load_failed");
  const { session } = await createAgentSession({
    cwd, agentDir, modelRuntime: runtime, model: runtime.getModel(PROVIDER, MODEL),
    settingsManager: settings, resourceLoader: loader,
    sessionManager: SessionManager.create(cwd, join(stateDir, "sessions")),
  });
  const protect = () => {
    settings.applyOverrides(NO_MODEL_LOOP);
    // Public Agent StreamFn seam. Even a native hook switching provider cannot
    // make the agent call another model, resolve its auth, retry, or compact via it.
    session.agent.streamFunction = stream;
  };
  const close = async () => {
    if (closed) return;
    closed = true;
    try { await session.abort(); await session.extensionRunner.emit({ type: "session_shutdown" }); }
    finally { session.dispose(); }
  };
  try { protect(); await session.bindExtensions({}); }
  catch (error) { await close().catch(() => {}); throw error; }
  const identity = () => ({ native_source: "pi", session: `pi_${session.sessionId}_${epoch}`, native_session_id: session.sessionId, cwd: session.sessionManager.getCwd(), model: `${session.model?.provider}/${session.model?.id}` });
  return {
    identity,
    isIdle: () => session.isIdle && !closed,
    tools() {
      requireCondition(!closed, "native_session_closed");
      const declared = new Map(declaredTools(session).map((tool) => [tool.name, tool]));
      const callable = new Set(session.getCallableToolNames());
      return session.getAllTools().map((tool) => ({
        name: tool.name, description: declared.get(tool.name)?.description ?? tool.description,
        parameters: declared.get(tool.name)?.parameters ?? tool.parameters,
        exposure: tool.exposure, namespace: tool.namespace, annotations: tool.annotations,
        declared: declared.has(tool.name), callable: callable.has(tool.name),
      }));
    },
    async invoke(calls, { signal, beforeDispatch }) {
      requireCondition(!pending && session.isIdle && !closed, "native_session_busy");
      requireCondition(!signal.aborted, "cancelled_before_dispatch");
      const request = { calls, signal, beforeDispatch, emitted: false, error: undefined };
      const results = [];
      const unsubscribe = session.subscribe((event) => { if (event.type === "turn_end") results.push(...event.toolResults); });
      const abort = () => { void session.abort().catch(() => {}); };
      signal.addEventListener("abort", abort, { once: true });
      pending = request;
      try {
        // Entering the native turn also enters extension hooks. Any subsequent
        // timeout is uncertain, even when no tool-call result reached turn_end.
        beforeDispatch();
        await session.prompt("Execute this WebCodex externally supplied native-tool batch; no model reasoning is requested.");
        if (request.error) throw request.error;
        requireCondition(request.emitted, "native_request_not_emitted");
        return results;
      } finally { pending = undefined; unsubscribe(); signal.removeEventListener("abort", abort); }
    },
    abort: () => session.abort(),
    async refresh() {
      requireCondition(!pending && session.isIdle && !closed, "native_session_busy");
      await session.reload({ beforeSessionStart: protect });
      protect();
      requireCondition(loader.getExtensions().errors.length === 0, "native_extension_load_failed");
    },
    history: (limit) => session.messages.filter((message) => message.role === "toolResult").slice(-limit),
    close,
  };
}
