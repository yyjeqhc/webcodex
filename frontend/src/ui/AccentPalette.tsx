import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { ACCENT_PRESETS, normalizeAccent } from "./accent.js";

type Props = { color: string; onChange: (color: string) => void; label: string; customLabel: string; compact?: boolean; colorLabel?: (name: string) => string };

/** A native disclosure with a palette outside clipping/scroll containers. */
export function AccentPalette({ color, onChange, label, customLabel, compact = false, colorLabel = value => value }: Props) {
  const root = useRef<HTMLDetailsElement>(null); const trigger = useRef<HTMLElement>(null); const panel = useRef<HTMLDivElement>(null);
  const [opened, setOpened] = useState(false); const [position, setPosition] = useState<CSSProperties>({ position: "fixed", visibility: "hidden" });
  const id = useId();
  const close = (restore = false) => { setOpened(false); if (restore) trigger.current?.focus(); };
  useLayoutEffect(() => {
    if (!opened) return;
    const place = () => {
      const anchor = trigger.current?.getBoundingClientRect(); if (!anchor) return;
      const viewport = window.visualViewport;
      const leftEdge = viewport?.offsetLeft ?? 0; const topEdge = viewport?.offsetTop ?? 0;
      const width = viewport?.width ?? window.innerWidth; const height = viewport?.height ?? window.innerHeight;
      const paletteWidth = Math.min(260, Math.max(120, width - 24));
      const paletteHeight = Math.min(panel.current?.scrollHeight || 192, Math.max(90, height - 24));
      const left = Math.max(leftEdge + 12, Math.min(anchor.left, leftEdge + width - paletteWidth - 12));
      const below = anchor.bottom + 8;
      const top = below + paletteHeight <= topEdge + height - 12 ? below : Math.max(topEdge + 12, anchor.top - paletteHeight - 8);
      setPosition({ position: "fixed", visibility: "visible", top, left, right: "auto", bottom: "auto", width: paletteWidth, maxHeight: height - 24 });
    };
    place();
    panel.current?.querySelector<HTMLButtonElement>('button[aria-pressed="true"]')?.focus();
    window.addEventListener("resize", place); window.addEventListener("scroll", place, true);
    window.visualViewport?.addEventListener("resize", place); window.visualViewport?.addEventListener("scroll", place);
    return () => { window.removeEventListener("resize", place); window.removeEventListener("scroll", place, true); window.visualViewport?.removeEventListener("resize", place); window.visualViewport?.removeEventListener("scroll", place); };
  }, [opened]);
  useEffect(() => {
    if (!opened) return;
    const outside = (event: Event) => { const target = event.target as Node; if (!root.current?.contains(target) && !panel.current?.contains(target)) close(); };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(true); } };
    document.addEventListener("pointerdown", outside); document.addEventListener("focusin", outside); document.addEventListener("keydown", escape, true);
    return () => { document.removeEventListener("pointerdown", outside); document.removeEventListener("focusin", outside); document.removeEventListener("keydown", escape, true); };
  }, [opened]);
  return <details ref={root} open={opened} className={`accent-picker${compact ? " accent-picker-compact" : ""}`} onToggle={event => { if (event.currentTarget.open !== opened) setOpened(event.currentTarget.open); }}>
    <summary ref={trigger} aria-label={label} aria-haspopup="dialog" aria-expanded={opened} aria-controls={opened ? id : undefined} title={label} onClick={event => { event.preventDefault(); setOpened(value => !value); }}>
      <i className="accent-current" aria-hidden="true" /><span>{label}</span>
    </summary>
    {opened && createPortal(<div id={id} ref={panel} role="dialog" aria-label={label} className="accent-picker-panel accent-picker-portal" style={position} onKeyDown={event => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight" && event.key !== "Home" && event.key !== "End") return;
      const buttons = Array.from(panel.current?.querySelectorAll<HTMLButtonElement>(".accent-swatch") || []);
      if (!buttons.length || !buttons.includes(document.activeElement as HTMLButtonElement)) return;
      event.preventDefault(); const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (current + (event.key === "ArrowRight" ? 1 : buttons.length - 1)) % buttons.length;
      buttons[next].focus();
    }}>
      <div className="accent-picker-title">{label}</div>
      <div className="accent-swatches">{ACCENT_PRESETS.map(preset => <button type="button" key={preset.id} className="accent-swatch" title={colorLabel(preset.label)} aria-label={colorLabel(preset.label)} aria-pressed={color.toLowerCase() === preset.color} style={{ "--swatch-color": preset.color } as CSSProperties} onClick={() => onChange(preset.color)} />)}</div>
      <label className="accent-custom"><span>{customLabel}</span><input type="color" aria-label={customLabel} value={normalizeAccent(color) ?? ACCENT_PRESETS[0].color} onChange={event => { const next = normalizeAccent(event.currentTarget.value); if (next) onChange(next); }} /><code>{color}</code></label>
    </div>, document.body)}
  </details>;
}
