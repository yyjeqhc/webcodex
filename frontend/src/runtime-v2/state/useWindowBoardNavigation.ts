import { useCallback, useEffect, useRef } from "react";
import { useMediaQuery } from "@mantine/hooks";

/** Browser navigation is presentation only: never put a Window identifier in a URL. */
export function useWindowBoardNavigation(selectedKey: string, select: (key: string) => void,
  onMobileDetailChange?: (open: boolean) => void) {
  const mobile = useMediaQuery("(max-width: 700px)", false, { getInitialValueInEffect: false });
  const narrow = useMediaQuery("(max-width: 1279px)", false, { getInitialValueInEffect: false });
  const detailRef = useRef<HTMLElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const ownsHistory = useRef(false);
  const selected = useRef(selectedKey); selected.current = selectedKey;
  const close = useCallback(() => {
    if (ownsHistory.current && window.history.state?.webcodexWindowDetail) window.history.back();
    else select("");
  }, [select]);
  const open = useCallback((key: string) => {
    returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    select(key);
  }, [select]);
  useEffect(() => {
    if (!mobile || !selectedKey || ownsHistory.current) return;
    window.history.pushState({ webcodexWindowDetail: true }, "");
    ownsHistory.current = true;
  }, [mobile, selectedKey]);
  useEffect(() => {
    const popped = () => { ownsHistory.current = false; select(""); };
    window.addEventListener("popstate", popped);
    return () => {
      window.removeEventListener("popstate", popped);
      // Retire only our marker when the workbench unmounts; never navigate a
      // different view or reveal a Window identity through browser history.
      if (ownsHistory.current && window.history.state?.webcodexWindowDetail) window.history.replaceState(null, "");
    };
  }, [select]);
  useEffect(() => {
    onMobileDetailChange?.(mobile && Boolean(selectedKey));
    return () => onMobileDetailChange?.(false);
  }, [mobile, selectedKey, onMobileDetailChange]);
  useEffect(() => {
    if (selectedKey && narrow) detailRef.current?.focus();
    if (!selectedKey && returnFocus.current?.isConnected) returnFocus.current.focus();
  }, [selectedKey, narrow]);
  useEffect(() => {
    const detail = detailRef.current;
    const header = detail?.querySelector("header");
    if (!mobile || !detail || !header) return;
    const measure = () => detail.style.setProperty("--window-board-header-height", header.getBoundingClientRect().height + "px");
    measure(); const observer = new ResizeObserver(measure); observer.observe(header);
    return () => observer.disconnect();
  }, [selectedKey, mobile]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || !selected.current) return;
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (target?.closest("input, textarea, select, [contenteditable=true], [role=dialog], [data-portal]")) return;
      // Portalled menus/popovers own Escape even when focus stays on their trigger.
      const overlays = document.querySelectorAll<HTMLElement>('[data-portal] [role=dialog], [data-portal] [role=menu], [data-portal] .mantine-Popover-dropdown');
      if ([...overlays].some(element => {
        for (let node: HTMLElement | null = element; node; node = node.parentElement) {
          const style = getComputedStyle(node);
          if (node.hidden || style.display === 'none' || style.visibility === 'hidden') return false;
        }
        return true;
      })) return;
      event.preventDefault(); close();
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [close]);
  return { mobile, drawer: narrow && !mobile, detailRef, open, close };
}
