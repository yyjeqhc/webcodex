import { AppBridge, PostMessageTransport } from "@modelcontextprotocol/ext-apps/app-bridge";
const frame = document.querySelector("iframe"), params = new URLSearchParams(location.search);
const bridge = new AppBridge(null, { name: "PDF reader browser test", version: "1.0" }, { serverTools: {}, logging: {} },
  { hostContext: { theme: params.get("theme") || "light", displayMode: "fullscreen", availableDisplayModes: ["inline", "fullscreen"] } });
bridge.oncalltool = async request => {
  const response = await fetch(`/tool?fallback=${params.get("fallback") || "0"}&stale=${params.get("stale") || "0"}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(request) });
  return response.json();
};
bridge.oninitialized = async () => {
  const response = await fetch(`/present?sample=${params.get("sample") || "cjk"}&fallback=${params.get("fallback") || "0"}`);
  await bridge.sendToolResult(await response.json());
};
await bridge.connect(new PostMessageTransport(frame.contentWindow, frame.contentWindow));
frame.src = `/viewer?policy=${params.get("policy") || "allow"}`;
globalThis.pdfDocumentTestBridge = bridge;
