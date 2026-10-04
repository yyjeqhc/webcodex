import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import { LANGUAGE_STORAGE_KEY, loadLanguagePreference, translate } from "../src/runtime_i18n.js";
import { normalizeRuntimeLanguage, RUNTIME_LANGUAGES, WORK_TEXT } from "../src/runtime_locales.js";

describe("Runtime language preferences", () => {
  it.each(RUNTIME_LANGUAGES)("restores $label", ({ value }) => {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, value);
    expect(loadLanguagePreference()).toBe(value);
  });
  it.each([
    ["en-US", "en"], ["de-AT", "de-DE"], ["fr-CA", "fr-FR"],
    ["zh-Hant-HK", "zh-TW"], ["zh-HK", "zh-TW"], ["zh-SG", "zh-CN"],
    ["ja", "ja-JP"], ["ko", "ko-KR"], ["unknown", undefined],
  ])("normalizes %s", (input, expected) => {
    expect(normalizeRuntimeLanguage(input)).toBe(expected);
  });
  it("uses the first supported browser language when storage is invalid", () => {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, "unsupported");
    const spy = vi.spyOn(navigator, "languages", "get").mockReturnValue(["es-ES", "fr-CA"]);
    expect(loadLanguagePreference()).toBe("fr-FR");
    spy.mockRestore();
  });
  it("keeps the pre-React bootstrap aligned with every supported locale", () => {
    const html = readFileSync("src/runtime-v2/runtime.html", "utf8");
    expect(html).toContain("navigator.languages");
    for (const { value } of RUNTIME_LANGUAGES) expect(html).toContain(`\"${value}\"`);
  });
  it("preserves interpolation fields in every catalog and falls back for raw evidence", () => {
    for (const [source, translations] of Object.entries(WORK_TEXT)) {
      expect(translations).toHaveLength(5);
      for (const translation of translations) {
        expect(translation.trim()).not.toBe("");
        expect(translation.match(/\{\w+\}/g) || []).toEqual(source.match(/\{\w+\}/g) || []);
      }
    }
    expect(translate("raw_backend_detail", "de-DE")).toBe("raw_backend_detail");
  });
});
