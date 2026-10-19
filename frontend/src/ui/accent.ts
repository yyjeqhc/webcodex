/** A neutral interface with one user-selected interactive color. */
export const ACCENT_STORAGE_KEY = "webcodex.ui.accent.v1";
export const DEFAULT_ACCENT = "#2563eb";
export const ACCENT_CHANGE_EVENT = "webcodex:accent-change";

// Tailwind's Blue, Indigo, Teal, Violet, and Orange scales provide familiar,
// distinct starting points. The user's own hex color is never limited to them.
export const ACCENT_PRESETS = [
  { id: "blue", label: "Blue", color: "#2563eb" },
  { id: "indigo", label: "Indigo", color: "#4f46e5" },
  { id: "teal", label: "Teal", color: "#0f766e" },
  { id: "violet", label: "Violet", color: "#7c3aed" },
  { id: "orange", label: "Orange", color: "#c2410c" },
] as const;

type Rgb = [number, number, number];

export function normalizeAccent(value: unknown): string | null {
  return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value.toLowerCase() : null;
}

export function loadAccentPreference(): string {
  try { return normalizeAccent(window.localStorage.getItem(ACCENT_STORAGE_KEY)) || DEFAULT_ACCENT; }
  catch { return DEFAULT_ACCENT; }
}

export function persistAccentPreference(value: string): void {
  const color = normalizeAccent(value);
  if (!color) return;
  try { window.localStorage.setItem(ACCENT_STORAGE_KEY, color); }
  catch { /* Keep the active in-memory choice. */ }
  window.dispatchEvent(new CustomEvent(ACCENT_CHANGE_EVENT, { detail: color }));
}

function rgb(value: string): Rgb {
  return [1, 3, 5].map((offset) => parseInt(value.slice(offset, offset + 2), 16)) as Rgb;
}

function hex(value: Rgb): string {
  return "#" + value.map((channel) => Math.round(channel).toString(16).padStart(2, "0")).join("");
}

function mix(a: Rgb, b: Rgb, amount: number): Rgb {
  return a.map((channel, index) => channel * (1 - amount) + b[index] * amount) as Rgb;
}

function luminance(value: Rgb): number {
  const [r, g, b] = value.map((channel) => {
    const linear = channel / 255;
    return linear <= 0.04045 ? linear / 12.92 : ((linear + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
  const first = luminance(rgb(a));
  const second = luminance(rgb(b));
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

function contrastColor(seed: Rgb, background: string, target: Rgb, minimum = 4.5): Rgb {
  if (contrastRatio(hex(seed), background) >= minimum) return seed;
  let low = 0;
  let high = 1;
  for (let index = 0; index < 16; index += 1) {
    const mid = (low + high) / 2;
    if (contrastRatio(hex(mix(seed, target, mid)), background) >= minimum) high = mid;
    else low = mid;
  }
  return mix(seed, target, high);
}

export function accentTokens(value: string, theme: "light" | "dark") {
  const seed = rgb(normalizeAccent(value) || DEFAULT_ACCENT);
  const dark = theme === "dark";
  const base = contrastColor(seed, dark ? "#111111" : "#ffffff", dark ? [255, 255, 255] : [0, 0, 0], 4.6);
  const hover = mix(base, dark ? [255, 255, 255] : [0, 0, 0], 0.12);
  const surface = dark ? [24, 24, 26] as Rgb : [255, 255, 255] as Rgb;
  const onAccent = contrastRatio(hex(base), "#ffffff") >= 4.5 ? "#ffffff" : "#111111";
  const soft = hex(mix(surface, base, dark ? 0.23 : 0.12));
  const softHover = hex(mix(surface, base, dark ? 0.32 : 0.18));
  const onSoft = hex(contrastColor(base, softHover, dark ? [255, 255, 255] : [0, 0, 0], 4.6));
  return {
    accent: hex(base),
    hover: hex(hover),
    soft,
    softHover,
    onSoft,
    onAccent,
  };
}

export function applyAccentPreference(value: string, theme: "light" | "dark", root: HTMLElement = document.documentElement): void {
  const tokens = accentTokens(value, theme);
  root.style.setProperty("--ui-accent", tokens.accent);
  root.style.setProperty("--ui-accent-hover", tokens.hover);
  root.style.setProperty("--ui-accent-soft", tokens.soft);
  root.style.setProperty("--ui-on-accent-soft", tokens.onSoft);
  root.style.setProperty("--ui-on-accent", tokens.onAccent);
  root.dataset.accent = ACCENT_PRESETS.find((preset) => preset.color === normalizeAccent(value))?.id || "custom";
}
