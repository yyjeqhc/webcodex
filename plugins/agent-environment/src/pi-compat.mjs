import { VERSION } from "@earendil-works/pi-coding-agent";
import { BridgeError } from "./catalog.mjs";

// Pi 1.0's SDK omits the CLI's built-in extension factories, and does not expose
// the hidden-declaration projection publicly. Keep these READ-ONLY compatibility
// seams here, pinned and fail-closed, instead of silently losing MCP/codemode.
export async function cliBuiltins() {
  if (VERSION !== "1.0.0") throw new BridgeError("unsupported_pi_version");
  const module = await import(new URL("./extensions/index.js", import.meta.resolve("@earendil-works/pi-coding-agent")));
  if (!Array.isArray(module.builtInExtensions)) throw new BridgeError("unsupported_pi_cli_extensions");
  return module.builtInExtensions;
}

export function declaredTools(session) {
  if (!(session._hiddenDeclarations instanceof Set)) throw new BridgeError("unsupported_pi_loadout");
  return session.agent.state.tools.filter((tool) => !session._hiddenDeclarations.has(tool.name));
}
