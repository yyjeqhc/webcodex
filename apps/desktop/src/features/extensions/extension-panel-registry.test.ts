import { describe, expect, it } from "vitest";
import { productText } from "../../i18n/product";
import { connectionsToolsText } from "../../i18n/connections-tools";
import { runnerCapabilitiesText } from "../../i18n/runner-capabilities";
import { EXTENSION_PANELS, extensionPanel, extensionTabForKey } from "./extension-panel-registry";

describe("bundled extension panel registry", () => {
  it("keeps a single complete, immutable capability-first inventory", () => {
    const ids = EXTENSION_PANELS.map(panel => panel.id);
    expect(ids).toEqual(["codingAgents", "sshResources", "mcpProviders", "nativePlugins", "skills", "instructions"]);
    expect(new Set(ids).size).toBe(ids.length);
    expect(Object.isFrozen(EXTENSION_PANELS)).toBe(true);
    for (const panel of EXTENSION_PANELS) {
      expect(Object.isFrozen(panel)).toBe(true);
      expect(extensionPanel(panel.id)).toBe(panel);
      expect(typeof panel.Content).toBe("function");
    }
    expect(extensionPanel(undefined).id).toBe("codingAgents");
    expect(extensionPanel("future-provider").id).toBe("codingAgents");
    expect(EXTENSION_PANELS.filter(panel => panel.PersistentConfiguration).map(panel => panel.id)).toEqual(["instructions"]);
    expect(EXTENSION_PANELS.filter(panel => panel.projectPreview).map(panel => panel.id)).toEqual(["nativePlugins", "skills", "instructions"]);
  });

  it("keeps localized labels and purposes with their components", () => {
    for (const locale of ["en-US", "zh-CN", "de-DE", "fr-FR", "ja-JP", "ko-KR", "zh-TW"]) {
      const text = {
        product: (key: Parameters<typeof productText>[1]) => productText(locale, key),
        connections: (key: Parameters<typeof connectionsToolsText>[1]) => connectionsToolsText(locale, key),
        capabilities: (key: Parameters<typeof runnerCapabilitiesText>[1]) => runnerCapabilitiesText(locale, key),
      };
      for (const panel of EXTENSION_PANELS) {
        expect(panel.title(text).length).toBeGreaterThan(0);
        expect(text.product(panel.purpose).length).toBeGreaterThan(0);
      }
    }
  });

  it("derives wrapped arrow and Home/End navigation from the same inventory", () => {
    for (const [index, panel] of EXTENSION_PANELS.entries()) {
      const next = EXTENSION_PANELS[(index + 1) % EXTENSION_PANELS.length].id;
      const previous = EXTENSION_PANELS[(index + EXTENSION_PANELS.length - 1) % EXTENSION_PANELS.length].id;
      for (const key of ["ArrowRight", "ArrowDown"]) expect(extensionTabForKey(panel.id, key)).toBe(next);
      for (const key of ["ArrowLeft", "ArrowUp"]) expect(extensionTabForKey(panel.id, key)).toBe(previous);
      expect(extensionTabForKey(panel.id, "Home")).toBe("codingAgents");
      expect(extensionTabForKey(panel.id, "End")).toBe("instructions");
      expect(extensionTabForKey(panel.id, "Tab")).toBeNull();
    }
  });
});
