import { defaultVariantColorsResolver, type VariantColorsResolver } from "@mantine/core";
import { accentTokens } from "./accent.js";

/** A single user accent still needs distinct foreground and surface colors. */
export function accentVariantResolver(accent: string, scheme: "light" | "dark"): VariantColorsResolver {
  const tokens = accentTokens(accent, scheme);
  return (input) => {
    const defaults = defaultVariantColorsResolver(input);
    if ((input.color || input.theme.primaryColor) !== "brand") return defaults;
    if (input.variant === "light") return {
      ...defaults,
      background: tokens.soft,
      hover: tokens.softHover,
      color: tokens.onSoft,
      hoverColor: tokens.onSoft,
      border: "1px solid transparent",
    };
    // brand uses a single accent, not a ten-shade palette. Mantine's default
    // subtle/outline hover otherwise resolves to the same color as the label.
    if (input.variant === "subtle" || input.variant === "outline") return {
      ...defaults,
      background: "transparent",
      hover: tokens.soft,
      color: tokens.accent,
      hoverColor: tokens.onSoft,
      border: input.variant === "outline" ? `1px solid ${tokens.accent}` : "1px solid transparent",
    };
    if (input.variant === "filled") return {
      ...defaults,
      background: tokens.accent,
      hover: tokens.hover,
      color: tokens.onAccent,
      hoverColor: tokens.onAccent,
    };
    return defaults;
  };
}
