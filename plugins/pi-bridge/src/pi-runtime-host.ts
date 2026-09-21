import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Value } from "typebox/value";

import {
  DefaultPackageManager,
  DefaultResourceLoader,
  ExtensionRunner,
  ModelRegistry,
  ModelRuntime,
  ProjectTrustStore,
  SessionManager,
  SettingsManager,
  createEventBus,
  hasTrustRequiringProjectResources,
} from "@earendil-works/pi-coding-agent";
import type {
  RegisteredTool,
  ToolCallEventResult,
} from "@earendil-works/pi-coding-agent";
import { ExtensionApprovalStore } from "./approval-store.js";
import type { ApprovalStatus } from "./approval-store.js";
import { ExtensionSessionStore } from "./extension-session-store.js";

const TOOL_TEXT_LIMIT = 64 * 1024;
const DETAILS_LIMIT = 32 * 1024;
const RESOURCE_TEXT_LIMIT = 128 * 1024;
const MAX_IMAGE_BASE64_CHARS = 440 * 1024;
const SUPPORTED_IMAGE_MIME = new Set([
  "image/png",
  "image/jpeg",
  "image/webp",
  "image/gif",
]);

export type ExtensionResultContent =
  | { type: "text"; text: string }
  | { type: "image"; data: string; mimeType: string };

let promptTemplateHelpersPromise:
  | Promise<{ substituteArgs: (content: string, args: string[]) => string }>
  | undefined;

let extensionLoaderHelpersPromise:
  | Promise<{ clearExtensionCache: () => void }>
  | undefined;

async function promptTemplateHelpers(): Promise<{
  substituteArgs: (content: string, args: string[]) => string;
}> {
  if (promptTemplateHelpersPromise === undefined) {
    const packageIndex = fileURLToPath(import.meta.resolve("@earendil-works/pi-coding-agent"));
    const moduleUrl = pathToFileURL(
      path.join(path.dirname(packageIndex), "core", "prompt-templates.js"),
    ).href;
    promptTemplateHelpersPromise = import(moduleUrl).then((module) => {
      if (typeof module.substituteArgs !== "function") {
        throw new Error("Pi prompt template helper substituteArgs is unavailable");
      }
      return { substituteArgs: module.substituteArgs };
    });
  }
  return promptTemplateHelpersPromise;
}

async function extensionLoaderHelpers(): Promise<{ clearExtensionCache: () => void }> {
  if (extensionLoaderHelpersPromise === undefined) {
    const packageIndex = fileURLToPath(import.meta.resolve("@earendil-works/pi-coding-agent"));
    const moduleUrl = pathToFileURL(
      path.join(path.dirname(packageIndex), "core", "extensions", "loader.js"),
    ).href;
    extensionLoaderHelpersPromise = import(moduleUrl).then((module) => {
      if (typeof module.clearExtensionCache !== "function") {
        throw new Error("Pi extension loader helper clearExtensionCache is unavailable");
      }
      return { clearExtensionCache: module.clearExtensionCache };
    });
  }
  return extensionLoaderHelpersPromise;
}

export interface ExtensionToolSummary {
  name: string;
  label: string;
  description: string;
  source: string;
}

export interface ExtensionCandidateSummary {
  candidateId: string;
  displayPath: string;
  scope: "user" | "project" | "temporary";
  sha256: string;
  approved: boolean;
  executable: boolean;
  nextAction: "ready" | "trust_then_approve" | "approve_exact_fingerprint" | "fix_candidate";
  reason?: string;
}

interface ScopedApprovalStatus {
  status: ApprovalStatus;
  scope: "user" | "project" | "temporary";
}

export interface ResourceInventory {
  projectTrusted: boolean;
  generation: number;
  extensions: {
    candidates: number;
    approved: number;
    pending: number;
    loaded: number;
    errors: Array<{ path: string; error: string }>;
  };
  skills: Array<{
    name: string;
    description: string;
    path: string;
    disableModelInvocation: boolean;
  }>;
  prompts: Array<{
    name: string;
    description: string;
    path: string;
  }>;
  contextFiles: Array<{ path: string; chars: number }>;
  themes: Array<{ name: string }>;
  packages: Array<{
    source: string;
    scope: "user" | "project";
    filtered: boolean;
    installed: boolean;
  }>;
}

export interface ExtensionToolCallResult {
  tool: string;
  engine: "pi-extension";
  text: string;
  isError: boolean;
  contentTypes: string[];
  content: ExtensionResultContent[];
  detailsJson: string | null;
  generation: number;
}

function bounded(value: string, limit: number): string {
  const suffix = "\n[truncated]";
  return value.length <= limit ? value : (value.slice(0, Math.max(0, limit - suffix.length)) + suffix).slice(0, limit);
}

function jsonDetails(value: unknown): string | null {
  if (value === undefined) return null;
  try {
    return bounded(JSON.stringify(value), DETAILS_LIMIT);
  } catch {
    return "[unserializable details]";
  }
}

function extensionCandidateId(extensionPath: string): string {
  return (
    "wpx_" +
    createHash("sha256")
      .update("webpi-extension-candidate\0")
      .update(path.resolve(extensionPath))
      .digest("hex")
      .slice(0, 24)
  );
}

export class PiRuntimeHost {
  readonly cwd: string;
  readonly agentDir: string;

  private readonly trustStore: ProjectTrustStore;
  private readonly settingsManager: SettingsManager;
  private readonly packageManager: DefaultPackageManager;
  private readonly approvalStore: ExtensionApprovalStore;
  private readonly eventBus: ReturnType<typeof createEventBus>;
  private loader: DefaultResourceLoader;
  private extensionApprovalStatuses: ScopedApprovalStatus[] = [];
  private loadedExtensionApprovals: ScopedApprovalStatus[] = [];
  private readonly modelRuntime: ModelRuntime;
  private readonly modelRegistry: ModelRegistry;
  private readonly sessionManager: SessionManager;
  private readonly extensionSessionStore: ExtensionSessionStore;

  private runner: ExtensionRunner | undefined;
  private generationValue = 0;
  private activeToolNames = new Set<string>();
  private sessionName: string | undefined;
  private closed = false;

  private constructor(
    cwd: string,
    agentDir: string,
    trustStore: ProjectTrustStore,
    settingsManager: SettingsManager,
    packageManager: DefaultPackageManager,
    approvalStore: ExtensionApprovalStore,
    eventBus: ReturnType<typeof createEventBus>,
    loader: DefaultResourceLoader,
    modelRuntime: ModelRuntime,
    modelRegistry: ModelRegistry,
    sessionManager: SessionManager,
    extensionSessionStore: ExtensionSessionStore,
  ) {
    this.cwd = cwd;
    this.agentDir = agentDir;
    this.trustStore = trustStore;
    this.settingsManager = settingsManager;
    this.packageManager = packageManager;
    this.approvalStore = approvalStore;
    this.eventBus = eventBus;
    this.loader = loader;
    this.modelRuntime = modelRuntime;
    this.modelRegistry = modelRegistry;
    this.sessionManager = sessionManager;
    this.extensionSessionStore = extensionSessionStore;
  }

  static async create(cwd = process.cwd()): Promise<PiRuntimeHost> {
    const root = path.resolve(cwd);
    const agentDir = path.resolve(
      process.env.WEBPI_PI_AGENT_DIR ?? path.join(root, ".webpi-state", "pi-agent"),
    );
    const trustStore = new ProjectTrustStore(agentDir);
    const explicitlyTrusted = trustStore.get(root) === true;
    const needsTrust = hasTrustRequiringProjectResources(root);
    const projectTrusted = explicitlyTrusted || !needsTrust;

    const settingsManager = SettingsManager.create(root, agentDir, { projectTrusted });
    const eventBus = createEventBus();
    const packageManager = new DefaultPackageManager({
      cwd: root,
      agentDir,
      settingsManager,
    });
    const approvalStore = new ExtensionApprovalStore(root, agentDir);
    const loader = new DefaultResourceLoader({
      cwd: root,
      agentDir,
      settingsManager,
      eventBus,
      noExtensions: true,
    });
    const modelRuntime = await ModelRuntime.create({
      authPath: path.join(agentDir, "auth.json"),
      modelsPath: null,
      modelsStorePath: path.join(agentDir, "models-store.json"),
      allowModelNetwork: false,
      refreshOnCreate: false,
    });
    const modelRegistry = new ModelRegistry(modelRuntime);
    const extensionSessionStore = new ExtensionSessionStore(root, agentDir);
    const sessionManager = SessionManager.inMemory(
      root,
      undefined,
      extensionSessionStore.load(),
    );

    const host = new PiRuntimeHost(
      root,
      agentDir,
      trustStore,
      settingsManager,
      packageManager,
      approvalStore,
      eventBus,
      loader,
      modelRuntime,
      modelRegistry,
      sessionManager,
      extensionSessionStore,
    );
    await host.reload();
    return host;
  }

  get generation(): number {
    return this.generationValue;
  }

  isProjectTrusted(): boolean {
    if (!hasTrustRequiringProjectResources(this.cwd)) return true;
    return this.trustStore.get(this.cwd) === true;
  }

  isProjectExplicitlyTrusted(): boolean {
    return this.trustStore.get(this.cwd) === true;
  }

  relative(value: string): string {
    const absolute = path.resolve(value);
    const rootPrefix = this.cwd.endsWith(path.sep) ? this.cwd : this.cwd + path.sep;
    if (absolute === this.cwd) return ".";
    if (absolute.startsWith(rootPrefix)) {
      return path.relative(this.cwd, absolute).split(path.sep).join("/");
    }
    if (absolute.startsWith(this.agentDir + path.sep) || absolute === this.agentDir) {
      return "<webpi-agent>/" + path.relative(this.agentDir, absolute).split(path.sep).join("/");
    }
    return "<external>/" + path.basename(absolute);
  }

  private extensionCandidateSummary(entry: ScopedApprovalStatus): ExtensionCandidateSummary {
    const projectTrusted = this.isProjectTrusted();
    const executable = entry.status.approved && (entry.scope !== "project" || projectTrusted);
    const nextAction: ExtensionCandidateSummary["nextAction"] =
      entry.status.sha256 === null
        ? "fix_candidate"
        : executable
          ? "ready"
          : entry.scope === "project" && !projectTrusted
            ? "trust_then_approve"
            : "approve_exact_fingerprint";
    return {
      candidateId: extensionCandidateId(entry.status.path),
      displayPath: this.relative(entry.status.path),
      scope: entry.scope,
      sha256: entry.status.sha256 ?? "",
      approved: entry.status.approved,
      executable,
      nextAction,
      ...(entry.status.reason === undefined ? {} : { reason: this.sanitizeMessage(entry.status.reason) }),
    };
  }

  async extensionCandidates(): Promise<ExtensionCandidateSummary[]> {
    await this.refreshExtensionApprovalStatuses();
    return this.extensionApprovalStatuses.map((entry) => this.extensionCandidateSummary(entry));
  }

  async installPackage(source: string, scope: "user" | "project"): Promise<ResourceInventory> {
    if (!this.isProjectExplicitlyTrusted()) {
      throw new Error("explicit Pi project trust is required before web package installation");
    }
    await this.packageManager.installAndPersist(source, { local: scope === "project" });
    return this.reload();
  }

  async updatePackage(source?: string): Promise<ResourceInventory> {
    if (!this.isProjectExplicitlyTrusted()) {
      throw new Error("explicit Pi project trust is required before web package update");
    }
    await this.packageManager.update(source);
    return this.reload();
  }

  async removePackage(source: string, scope: "user" | "project"): Promise<ResourceInventory> {
    if (!this.isProjectExplicitlyTrusted()) {
      throw new Error("explicit Pi project trust is required before web package removal");
    }
    await this.packageManager.removeAndPersist(source, { local: scope === "project" });
    return this.reload();
  }

  async approveExtensionCandidate(
    candidateId: string,
    expectedSha256: string,
  ): Promise<ExtensionCandidateSummary> {
    await this.refreshExtensionApprovalStatuses();
    const entry = this.extensionApprovalStatuses.find(
      (candidate) => extensionCandidateId(candidate.status.path) === candidateId,
    );
    if (entry === undefined) throw new Error("unknown Pi extension candidate; refresh candidate status");
    if (entry.status.sha256 === null) throw new Error("Pi extension candidate cannot be fingerprinted");
    if (entry.scope === "project" && !this.isProjectExplicitlyTrusted()) {
      throw new Error("project extension approval requires explicit Pi project trust first");
    }
    await this.approvalStore.approveExpected(entry.status.path, expectedSha256);
    await this.refreshExtensionApprovalStatuses();
    const approved = this.extensionApprovalStatuses.find(
      (candidate) => extensionCandidateId(candidate.status.path) === candidateId,
    );
    if (approved === undefined) throw new Error("approved Pi extension candidate disappeared");
    return this.extensionCandidateSummary(approved);
  }

  async revokeExtensionCandidate(candidateId: string): Promise<ResourceInventory> {
    await this.refreshExtensionApprovalStatuses();
    const entry = this.extensionApprovalStatuses.find(
      (candidate) => extensionCandidateId(candidate.status.path) === candidateId,
    );
    if (entry === undefined) throw new Error("unknown Pi extension candidate; refresh candidate status");
    await this.approvalStore.revoke(entry.status.path);
    return this.reload();
  }

  private sanitizeMessage(value: string): string {
    return bounded(
      value
        .split(this.cwd).join("<project>")
        .split(this.agentDir).join("<webpi-agent>"),
      RESOURCE_TEXT_LIMIT,
    );
  }

  private async refreshExtensionApprovalStatuses(): Promise<boolean> {
    const projectTrusted = this.isProjectTrusted();
    this.settingsManager.setProjectTrusted(projectTrusted);
    await this.settingsManager.reload();

    const resolved = await this.packageManager.resolve(async () => "skip");
    const candidateByPath = new Map<string, "user" | "project" | "temporary">();
    for (const entry of resolved.extensions) {
      if (!entry.enabled) continue;
      const resolvedPath = path.resolve(entry.path);
      const existing = candidateByPath.get(resolvedPath);
      if (
        existing === undefined ||
        (existing === "user" && entry.metadata.scope === "project")
      ) {
        candidateByPath.set(resolvedPath, entry.metadata.scope);
      }
    }
    const candidates = [...candidateByPath.entries()].sort(([a], [b]) => a.localeCompare(b));
    this.extensionApprovalStatuses = await Promise.all(
      candidates.map(async ([candidate, scope]) => ({
        status: await this.approvalStore.status(candidate),
        scope,
      })),
    );
    return projectTrusted;
  }

  private async createApprovedLoader(): Promise<DefaultResourceLoader> {
    const projectTrusted = await this.refreshExtensionApprovalStatuses();
    const approved = this.extensionApprovalStatuses.filter(
      (entry) => entry.status.approved && (entry.scope !== "project" || projectTrusted),
    );
    const approvedPaths = approved.map((entry) => entry.status.path);

    const loader = new DefaultResourceLoader({
      cwd: this.cwd,
      agentDir: this.agentDir,
      settingsManager: this.settingsManager,
      eventBus: this.eventBus,
      noExtensions: true,
      additionalExtensionPaths: approvedPaths,
    });
    await loader.reload();
    this.loadedExtensionApprovals = approved;
    return loader;
  }

  private extensionSourceLabel(extensionPath: string): string {
    if (extensionPath.startsWith("<")) {
      return "extension:" + extensionPath.replace(/[<>]/gu, "");
    }
    return "extension:" + path.basename(extensionPath).replace(/\.(?:ts|js)$/u, "");
  }

  private buildExtensionResourcePaths(
    entries: Array<{ path: string; extensionPath: string }>,
  ): Array<{
    path: string;
    metadata: {
      source: string;
      scope: "temporary";
      origin: "top-level";
      baseDir?: string;
    };
  }> {
    return entries.map((entry) => ({
      path: entry.path,
      metadata: {
        source: this.extensionSourceLabel(entry.extensionPath),
        scope: "temporary",
        origin: "top-level",
        ...(entry.extensionPath.startsWith("<")
          ? {}
          : { baseDir: path.dirname(entry.extensionPath) }),
      },
    }));
  }

  private async startRunnerLifecycle(
    runner: ExtensionRunner,
    reason: "startup" | "reload",
  ): Promise<void> {
    await runner.emit({ type: "session_start", reason });

    if (!runner.hasHandlers("resources_discover")) return;
    const discovered = await runner.emitResourcesDiscover(this.cwd, reason);
    if (
      discovered.skillPaths.length === 0 &&
      discovered.promptPaths.length === 0 &&
      discovered.themePaths.length === 0
    ) {
      return;
    }
    this.loader.extendResources({
      skillPaths: this.buildExtensionResourcePaths(discovered.skillPaths),
      promptPaths: this.buildExtensionResourcePaths(discovered.promptPaths),
      themePaths: this.buildExtensionResourcePaths(discovered.themePaths),
    });
  }

  private unsupportedAgentControl(feature: string): never {
    throw new Error(
      "Pi " +
        feature +
        " is not applicable in WebPi because ChatGPT web owns the only LLM loop; use WebPi canonical Workflow Session/runtime tools instead",
    );
  }

  private createRunner(): ExtensionRunner {
    const extensionsResult = this.loader.getExtensions();
    const runner = new ExtensionRunner(
      extensionsResult.extensions,
      extensionsResult.runtime,
      this.cwd,
      this.sessionManager,
      this.modelRegistry,
    );

    this.activeToolNames = new Set(
      runner.getAllRegisteredTools().map((entry) => entry.definition.name),
    );

    const getToolInfos = () =>
      runner.getAllRegisteredTools().map((entry) => ({
        name: entry.definition.name,
        description: entry.definition.description,
        parameters: entry.definition.parameters,
        promptGuidelines: entry.definition.promptGuidelines,
        sourceInfo: entry.sourceInfo,
      }));

    runner.bindCore(
      {
        sendMessage: () => this.unsupportedAgentControl("sendMessage"),
        sendUserMessage: () => this.unsupportedAgentControl("sendUserMessage"),
        appendEntry: (customType: string, data?: unknown) => {
          this.sessionManager.appendCustomEntry(customType, data);
          this.extensionSessionStore.save(this.sessionManager);
        },
        setSessionName: (name: string) => {
          this.sessionManager.appendSessionInfo(name);
          this.extensionSessionStore.save(this.sessionManager);
          this.sessionName = name;
        },
        getSessionName: () => this.sessionManager.getSessionName() ?? this.sessionName,
        setLabel: (entryId: string, label: string | undefined) => {
          this.sessionManager.appendLabelChange(entryId, label);
          this.extensionSessionStore.save(this.sessionManager);
        },
        getActiveTools: () => [...this.activeToolNames],
        getAllTools: getToolInfos,
        setActiveTools: (toolNames: string[]) => {
          this.activeToolNames = new Set(toolNames);
        },
        refreshTools: () => {
          this.activeToolNames = new Set(
            runner.getAllRegisteredTools().map((entry) => entry.definition.name),
          );
        },
        getCommands: () =>
          runner.getRegisteredCommands().map((command) => ({
            name: command.invocationName,
            description: command.description,
            source: "extension" as const,
          })),
        setModel: async () => false,
        getThinkingLevel: () => "off",
        setThinkingLevel: () => this.unsupportedAgentControl("thinking-level control"),
      } as any,
      {
        getModel: () => undefined,
        getScopedModels: () => [],
        isIdle: () => true,
        isProjectTrusted: () => this.isProjectTrusted(),
        getSignal: () => undefined,
        abort: () => this.unsupportedAgentControl("agent abort"),
        hasPendingMessages: () => false,
        shutdown: () => this.unsupportedAgentControl("agent shutdown"),
        getContextUsage: () => undefined,
        compact: () => this.unsupportedAgentControl("context compaction"),
        getSystemPrompt: () => this.loader.getSystemPrompt() ?? "",
        getSystemPromptOptions: () => ({ cwd: this.cwd }),
      } as any,
    );
    runner.bindCommandContext({
      waitForIdle: async () => {},
      newSession: async () => this.unsupportedAgentControl("newSession"),
      fork: async () => this.unsupportedAgentControl("session fork"),
      navigateTree: async () => this.unsupportedAgentControl("session tree navigation"),
      switchSession: async () => this.unsupportedAgentControl("session switch"),
      reload: async () => {
        await this.reload();
      },
    });
    runner.setUIContext(undefined, "print");
    return runner;
  }

  async reload(): Promise<ResourceInventory> {
    if (this.closed) throw new Error("Pi extension runtime is closed");
    const reason: "startup" | "reload" = this.generationValue === 0 ? "startup" : "reload";
    const previous = this.runner;
    this.runner = undefined;
    this.activeToolNames.clear();
    if (previous !== undefined) {
      try { await previous.emit({ type: "session_shutdown", reason: "reload" }); }
      finally { previous.invalidate("WebPi Pi resources reloaded"); }
    }
    (await extensionLoaderHelpers()).clearExtensionCache();
    try {
      this.loader = await this.createApprovedLoader();
      this.runner = this.createRunner();
      await this.startRunnerLifecycle(this.runner, reason);
      this.generationValue += 1;
      return this.inventory();
    } catch (error) {
      this.runner?.invalidate("WebPi Pi reload failed");
      this.runner = undefined;
      this.activeToolNames.clear();
      throw error;
    }
  }

  async shutdown(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    const previous = this.runner;
    this.runner = undefined;
    this.activeToolNames.clear();
    if (previous !== undefined) {
      try { await previous.emit({ type: "session_shutdown", reason: "quit" }); }
      finally { previous.invalidate("WebPi Pi extension runtime closed"); }
    }
  }

  inventory(): ResourceInventory {
    const extensions = this.loader.getExtensions();
    const skills = this.loader.getSkills();
    const prompts = this.loader.getPrompts();
    const themes = this.loader.getThemes();
    const agentsFiles = this.loader.getAgentsFiles();
    const packages = this.packageManager.listConfiguredPackages();

    return {
      projectTrusted: this.isProjectTrusted(),
      generation: this.generationValue,
      extensions: {
        candidates: this.extensionApprovalStatuses.length,
        approved: this.extensionApprovalStatuses.filter((entry) => entry.status.approved).length,
        pending: this.extensionApprovalStatuses.filter((entry) => !entry.status.approved).length,
        loaded: extensions.extensions.length,
        errors: extensions.errors.map((entry) => ({
          path: this.relative(entry.path),
          error: this.sanitizeMessage(entry.error),
        })),
      },
      skills: skills.skills.map((skill) => ({
        name: skill.name,
        description: skill.description,
        path: this.relative(skill.filePath),
        disableModelInvocation: skill.disableModelInvocation,
      })),
      prompts: prompts.prompts.map((prompt) => ({
        name: prompt.name,
        description: prompt.description,
        path: this.relative(prompt.filePath),
      })),
      contextFiles: agentsFiles.agentsFiles.map((entry) => ({
        path: this.relative(entry.path),
        chars: entry.content.length,
      })),
      themes: themes.themes.map((theme) => ({ name: theme.name ?? "unnamed" })),
      packages: packages.map((entry) => ({
        source: entry.source,
        scope: entry.scope,
        filtered: entry.filtered,
        installed: entry.installedPath !== undefined,
      })),
    };
  }

  async readSkill(name: string): Promise<{
    name: string;
    description: string;
    path: string;
    content: string;
    disableModelInvocation: boolean;
    generation: number;
  }> {
    const skill = this.loader.getSkills().skills.find((entry) => entry.name === name);
    if (skill === undefined) throw new Error("unknown Pi skill: " + name);
    const content = bounded(await readFile(skill.filePath, "utf8"), RESOURCE_TEXT_LIMIT);
    return {
      name: skill.name,
      description: skill.description,
      path: this.relative(skill.filePath),
      content: this.sanitizeMessage(content),
      disableModelInvocation: skill.disableModelInvocation,
      generation: this.generationValue,
    };
  }

  async expandPrompt(name: string, args: string[]): Promise<{
    name: string;
    description: string;
    expanded: string;
    generation: number;
  }> {
    const prompt = this.loader.getPrompts().prompts.find((entry) => entry.name === name);
    if (prompt === undefined) throw new Error("unknown Pi prompt: " + name);
    const helpers = await promptTemplateHelpers();
    return {
      name: prompt.name,
      description: prompt.description,
      expanded: this.sanitizeMessage(
        bounded(helpers.substituteArgs(prompt.content, args), RESOURCE_TEXT_LIMIT),
      ),
      generation: this.generationValue,
    };
  }

  contextSnapshot(): {
    generation: number;
    contextFiles: Array<{ path: string; content: string }>;
    systemPrompt: string;
    appendSystemPrompt: string[];
  } {
    const contextFiles = this.loader.getAgentsFiles().agentsFiles.map((entry) => ({
      path: this.relative(entry.path),
      content: this.sanitizeMessage(bounded(entry.content, RESOURCE_TEXT_LIMIT)),
    }));
    const systemPrompt = this.loader.getSystemPrompt();
    return {
      generation: this.generationValue,
      contextFiles,
      systemPrompt:
        systemPrompt === undefined
          ? ""
          : this.sanitizeMessage(bounded(systemPrompt, RESOURCE_TEXT_LIMIT)),
      appendSystemPrompt: this.loader
        .getAppendSystemPrompt()
        .map((entry) => this.sanitizeMessage(bounded(entry, RESOURCE_TEXT_LIMIT))),
    };
  }

  listCommands(): Array<{
    name: string;
    description: string;
    source: string;
  }> {
    if (this.runner === undefined) return [];
    return this.runner.getRegisteredCommands().map((command) => ({
      name: command.invocationName,
      description: command.description ?? "",
      source: this.relative(command.sourceInfo.path),
    }));
  }

  private async assertExecutableRuntime(): Promise<void> {
    const runner = this.runner;
    if (this.closed || runner === undefined) throw new Error("Pi extension runtime is closed or not initialized; reload approved resources");
    for (const entry of this.loadedExtensionApprovals) {
      const current = await this.approvalStore.status(entry.status.path);
      if (!current.approved || current.sha256 !== entry.status.sha256
          || (entry.scope === "project" && !this.isProjectTrusted())) {
        this.runner = undefined;
        this.activeToolNames.clear();
        runner.invalidate("Pi extension approval or trust changed");
        throw new Error("Pi extension approval, content, or project trust changed; review and reload before executing");
      }
    }
    if (runner !== this.runner) throw new Error("Pi runtime generation changed during approval validation; describe again");
  }

  async describeExtensionTool(name: string) {
    await this.assertExecutableRuntime();
    const registered = this.getRegisteredTool(name);
    if (!registered) throw new Error("unknown Pi extension tool: " + name);
    const parametersJson = JSON.stringify(registered.definition.parameters);
    if (Buffer.byteLength(parametersJson, "utf8") > 64 * 1024) throw new Error("Pi extension input schema exceeds the description bound");
    return {
      name: registered.definition.name,
      label: registered.definition.label,
      description: registered.definition.description,
      source: this.relative(registered.sourceInfo.path),
      parametersJson,
      active: this.activeToolNames.has(name),
      generation: this.generationValue,
    };
  }

  async callCommand(name: string, args: string): Promise<{
    command: string;
    completed: boolean;
    generation: number;
  }> {
    await this.assertExecutableRuntime();
    const runner = this.runner;
    if (runner === undefined) throw new Error("Pi extension runtime is not initialized");
    const command = runner.getCommand(name);
    if (command === undefined) throw new Error("unknown Pi extension command: " + name);
    await command.handler(args, runner.createCommandContext());
    return {
      command: command.invocationName,
      completed: true,
      generation: this.generationValue,
    };
  }

  listExtensionTools(): ExtensionToolSummary[] {
    if (this.runner === undefined) return [];
    return this.runner.getAllRegisteredTools().map((entry: RegisteredTool) => ({
      name: entry.definition.name,
      label: entry.definition.label,
      description: entry.definition.description,
      source: this.relative(entry.sourceInfo.path),
    }));
  }

  private getRegisteredTool(name: string): RegisteredTool | undefined {
    return this.runner?.getAllRegisteredTools().find((entry) => entry.definition.name === name);
  }

  async callExtensionTool(name: string, args: Record<string, unknown>): Promise<ExtensionToolCallResult> {
    await this.assertExecutableRuntime();
    const runner = this.runner;
    if (runner === undefined) throw new Error("Pi extension runtime is not initialized");

    const registered = this.getRegisteredTool(name);
    if (registered === undefined) throw new Error("unknown Pi extension tool: " + name);
    if (!this.activeToolNames.has(name)) throw new Error("Pi extension tool is not active: " + name);

    const toolCallId = randomUUID();
    const input: Record<string, unknown> = { ...args };
    await runner.emit({
      type: "tool_execution_start",
      toolCallId,
      toolName: name,
      args: input,
    });
    const gate = (await runner.emitToolCall({
      type: "tool_call",
      toolCallId,
      toolName: name,
      input,
    } as any)) as ToolCallEventResult | undefined;

    if (gate?.block) {
      await runner.emit({ type: "tool_execution_end", toolCallId, toolName: name,
        result: { content: [{ type: "text", text: gate.reason ?? "Pi extension blocked tool execution" }] }, isError: true });
      return {
        tool: name,
        engine: "pi-extension",
        text: this.sanitizeMessage(gate.reason ?? "Pi extension blocked tool execution"),
        isError: true,
        contentTypes: ["text"],
        content: [
          {
            type: "text",
            text: this.sanitizeMessage(gate.reason ?? "Pi extension blocked tool execution"),
          },
        ],
        detailsJson: null,
        generation: this.generationValue,
      };
    }

    let rawResult: any;
    try {
      const prepared = registered.definition.prepareArguments
        ? registered.definition.prepareArguments(input)
        : input;
      if (!Value.Check(registered.definition.parameters, prepared)) {
        const diagnostics = [...Value.Errors(registered.definition.parameters, prepared)]
          .slice(0, 8)
          .map((entry) => {
            const candidate = entry as unknown as { path?: string; message?: string };
            return (candidate.path || "/") + ": " + (candidate.message || "invalid value");
          })
          .join("; ");
        throw new Error(
          "invalid arguments for Pi extension tool " + name + (diagnostics ? ": " + diagnostics : ""),
        );
      }
      rawResult = await registered.definition.execute(
        toolCallId,
        prepared as any,
        undefined,
        undefined,
        runner.createContext(),
      );
    } catch (error) {
      const message = this.sanitizeMessage(error instanceof Error ? error.message : String(error));
      rawResult = {
        content: [{ type: "text", text: message }],
        details: undefined,
        isError: true,
      };
    }

    const baseContent = Array.isArray(rawResult?.content) ? rawResult.content : [];
    const resultHook = (await runner.emitToolResult({
      type: "tool_result",
      toolCallId,
      toolName: name,
      input,
      content: baseContent,
      details: rawResult?.details,
      isError: rawResult?.isError === true,
      usage: rawResult?.usage,
    } as any)) as
      | {
          content?: Array<Record<string, unknown>>;
          details?: unknown;
          isError?: boolean;
          usage?: unknown;
        }
      | undefined;

    const content = resultHook?.content ?? baseContent;
    const details = resultHook?.details ?? rawResult?.details;
    const isError = resultHook?.isError ?? rawResult?.isError === true;

    const textParts: string[] = [];
    const contentTypes: string[] = [];
    const normalizedContent: ExtensionResultContent[] = [];
    let normalizedError = isError;
    for (const block of content as Array<Record<string, unknown>>) {
      const type = typeof block.type === "string" ? block.type : "unknown";
      contentTypes.push(type);
      if (type === "text" && typeof block.text === "string") {
        const safeText = this.sanitizeMessage(bounded(block.text, TOOL_TEXT_LIMIT));
        textParts.push(safeText);
        normalizedContent.push({ type: "text", text: safeText });
        continue;
      }
      if (
        type === "image" &&
        typeof block.data === "string" &&
        typeof block.mimeType === "string"
      ) {
        if (
          SUPPORTED_IMAGE_MIME.has(block.mimeType) &&
          block.data.length <= MAX_IMAGE_BASE64_CHARS
        ) {
          normalizedContent.push({
            type: "image",
            data: block.data,
            mimeType: block.mimeType,
          });
          continue;
        }
        normalizedError = true;
        const message = "Pi extension returned an unsupported or oversized image result";
        textParts.push(message);
        normalizedContent.push({ type: "text", text: message });
        continue;
      }
      normalizedError = true;
      const message = "Pi extension returned unsupported tool content";
      textParts.push(message);
      normalizedContent.push({ type: "text", text: message });
    }

    if (normalizedContent.length === 0) {
      normalizedContent.push({
        type: "text",
        text: normalizedError ? "Pi extension tool failed without content" : "",
      });
    }

    await runner.emit({
      type: "tool_execution_end",
      toolCallId,
      toolName: name,
      result: rawResult,
      isError,
    });

    return {
      tool: name,
      engine: "pi-extension",
      text: this.sanitizeMessage(bounded(textParts.join("\n"), TOOL_TEXT_LIMIT)),
      isError: normalizedError,
      contentTypes,
      content: normalizedContent,
      detailsJson: jsonDetails(details),
      generation: this.generationValue,
    };
  }
}
