import { describe, expect, it } from "vitest";
import { ACCENT_PRESETS, accentTokens, contrastRatio, normalizeAccent } from "../src/ui/accent.js";
import { colorsTuple, DEFAULT_THEME, defaultVariantColorsResolver } from "@mantine/core";
import { accentVariantResolver } from "../src/ui/mantineAccentTheme.js";

describe("theme accent", () => {
  it("accepts only a complete hex color", () => {
    expect(normalizeAccent("#Aa22FF")).toBe("#aa22ff");
    expect(normalizeAccent("red; background: black")).toBeNull();
    expect(normalizeAccent("#fff")).toBeNull();
  });

  it("keeps preset and arbitrary colors readable in both themes", () => {
    expect(contrastRatio("#ffffff", "#000000")).toBeCloseTo(21, 1);
    for (const value of [...ACCENT_PRESETS.map((preset) => preset.color), "#ffffff", "#000000", "#ffcc00", "#888888"]) {
      for (const theme of ["light", "dark"] as const) {
        const tokens = accentTokens(value, theme);
        expect(contrastRatio(tokens.accent, theme === "light" ? "#ffffff" : "#111111")).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(tokens.accent, tokens.onAccent)).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(tokens.onSoft, tokens.soft)).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(tokens.onSoft, tokens.softHover)).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("keeps brand button labels readable when hovered, including selection controls", () => {
    for (const accent of [...ACCENT_PRESETS.map(preset => preset.color), "#ffffff", "#000000", "#ffcc00", "#888888"]) {
      for (const scheme of ["light", "dark"] as const) {
        const tokens = accentTokens(accent, scheme);
        const theme = { ...DEFAULT_THEME, primaryColor: "brand", colors: { ...DEFAULT_THEME.colors, brand: colorsTuple(tokens.accent) } };
        const resolve = accentVariantResolver(accent, scheme);
        for (const variant of ["subtle", "outline", "light", "filled"]) {
          const colors = resolve({ theme, color: "brand", variant });
          expect(contrastRatio(colors.hoverColor || colors.color, colors.hover), `${accent} ${scheme} ${variant}`).toBeGreaterThanOrEqual(4.5);
        }
        const danger = { theme, color: "red", variant: "filled" };
        expect(resolve(danger)).toEqual(defaultVariantColorsResolver(danger));
      }
    }
  });
});
