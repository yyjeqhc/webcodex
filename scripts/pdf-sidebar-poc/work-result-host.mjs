import { AppBridge, PostMessageTransport } from "@modelcontextprotocol/ext-apps/app-bridge";
const frame = document.querySelector("iframe"), params = new URLSearchParams(location.search);
const bridge = new AppBridge(null, { name: "Work Result browser test", version: "0.1" }, { serverTools: {}, logging: {} },
  { hostContext: { theme: params.get("theme") || "light", displayMode: "inline", availableDisplayModes: ["inline"] } });
bridge.oncalltool = async request => {
  const response = await fetch("/tool", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(request) });
  return response.json();
};
bridge.oninitialized = async () => {
  const response = await fetch(`/present?sample=${params.get("sample") || "text"}`);
  await bridge.sendToolResult(await response.json());
};
await bridge.connect(new PostMessageTransport(frame.contentWindow, frame.contentWindow));
frame.src = `/viewer?policy=${params.get("policy") || "allow"}`;
globalThis.workResultTestBridge = bridge;
