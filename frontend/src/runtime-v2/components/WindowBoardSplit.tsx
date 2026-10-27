import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
const STORAGE_KEY = "webcodex.windowBoard.width";

export function WindowBoardSplit({ selected, children, label }: { selected: boolean; children: ReactNode; label: string }) {
  const root = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(() => {
    try { return Math.max(320, Number(localStorage.getItem(STORAGE_KEY)) || 400); } catch { return 400; }
  });
  const [maximum, setMaximum] = useState(640);
  useEffect(() => {
    const resize = () => setMaximum(Math.max(320, (root.current?.clientWidth || 1067) * .6));
    resize(); const observer = new ResizeObserver(resize);
    if (root.current) observer.observe(root.current);
    return () => observer.disconnect();
  }, []);
  const resize = (value: number) => {
    const next = Math.max(320, Math.min(maximum, value)); setWidth(next);
    try { localStorage.setItem(STORAGE_KEY, String(next)); } catch { /* browser preference is optional */ }
  };
  return <div ref={root} className={"window-board-content" + (selected ? " has-detail" : "")}
    style={{ "--window-board-width": Math.min(width, maximum) + "px" } as CSSProperties}>
    {children}
    {selected && <div className="window-board-splitter" role="separator" tabIndex={0} aria-label={label}
      aria-orientation="vertical" aria-valuemin={320} aria-valuemax={Math.round(maximum)} aria-valuenow={Math.round(Math.min(width, maximum))}
      onKeyDown={event => {
        if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); resize(width + (event.key === "ArrowLeft" ? -20 : 20)); }
      }} onPointerDown={event => { event.currentTarget.setPointerCapture(event.pointerId); }}
      onPointerMove={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) resize(event.clientX - (root.current?.getBoundingClientRect().left || 0)); }}
      onPointerUp={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); }} />}
  </div>;
}
