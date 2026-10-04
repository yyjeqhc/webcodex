import { AppBridge, PostMessageTransport } from "@modelcontextprotocol/ext-apps/app-bridge";

const $ = id => document.getElementById(id);
const params = new URLSearchParams(location.search);
$("sample").value = params.get("sample") || "text";
$("policy").value = params.get("policy") || "allow";
let bridge, ready = false, inlineWidth = "420px";
let context = { theme: "light", displayMode: "inline", availableDisplayModes: ["inline", "fullscreen"],
  containerDimensions: { width: 420, maxHeight: 800 } };
async function tool(name, args) {
  const response = await fetch("/tool", { method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, arguments: args }) });
  if (!response.ok) throw new Error(`Tool transport: ${response.status}`);
  return response.json();
}
async function showSample() {
  if (!ready) return;
  await bridge.sendToolResult(await tool("display_pdf_sample", { sample: $("sample").value }));
}
async function load() {
  ready = false;
  if (bridge) { try { await bridge.teardownResource({}); } catch {} await bridge.close(); }
  bridge = new AppBridge(null, { name: "Local PDF compatibility harness", version: "0.1.0" },
    { serverTools: {}, logging: {} }, { hostContext: context });
  bridge.oncalltool = async request => {
    if (request.name !== "read_pdf_sample_bytes") throw new Error("Tool not admitted for this view");
    return tool(request.name, request.arguments);
  };
  bridge.oninitialized = () => { ready = true; $("host-status").textContent = `MCP Apps 已连接 · worker-src ${$("policy").value === "allow" ? "blob:" : "'none'"}`; showSample().catch(failed); };
  bridge.onrequestdisplaymode = async ({ mode }) => {
    context = { ...context, displayMode: mode === "fullscreen" ? "fullscreen" : "inline" };
    $("panel").classList.toggle("fullscreen", context.displayMode === "fullscreen");
    $("panel").style.width = context.displayMode === "fullscreen" ? "100%" : inlineWidth;
    bridge.setHostContext(context); return { mode: context.displayMode };
  };
  bridge.onerror = failed;
  await bridge.connect(new PostMessageTransport($("viewer").contentWindow, $("viewer").contentWindow));
  $("viewer").src = `/viewer?policy=${encodeURIComponent($("policy").value)}`;
}
function failed(error) { $("host-status").textContent = error.message; console.error(error); }
$("reload").onclick = () => load().catch(failed);
$("sample").onchange = () => showSample().catch(failed);
$("policy").onchange = () => load().catch(failed);
$("narrow").onclick = () => { inlineWidth = "360px"; $("panel").style.width = inlineWidth; };
$("wide").onclick = () => { inlineWidth = "860px"; $("panel").style.width = inlineWidth; };
$("theme").onclick = () => { context = { ...context, theme: context.theme === "light" ? "dark" : "light" }; bridge.setHostContext(context); };
$("teardown").onclick = async () => {
  await bridge.teardownResource({}); ready = false; $("host-status").textContent = "组件已卸载";
};
globalThis.pdfPocHost = { showSample, load };
load().catch(failed);
