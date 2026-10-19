import { useEffect } from "react";

const REVEAL_DELAY_MS = 400;

/** Reveal navigation shortcut keys while Ctrl/⌘ is held on its own.
 * The short delay keeps ordinary chords such as Ctrl+C from flashing hints. */
export function useShortcutHints(target: Window = window) {
  useEffect(() => {
    const root = target.document.documentElement;
    let timer: number | undefined;
    const hide = () => {
      if (timer !== undefined) target.clearTimeout(timer);
      timer = undefined;
      delete root.dataset.shortcutHints;
    };
    const onKeyDown = (event: KeyboardEvent) => {
      const modifierOnly = (event.key === "Control" || event.key === "Meta") && !event.altKey && !event.shiftKey;
      if (!modifierOnly) { hide(); return; }
      if (timer !== undefined || root.dataset.shortcutHints !== undefined) return;
      timer = target.setTimeout(() => {
        timer = undefined;
        root.dataset.shortcutHints = "";
      }, REVEAL_DELAY_MS);
    };
    target.addEventListener("keydown", onKeyDown);
    target.addEventListener("keyup", hide);
    target.addEventListener("pointerdown", hide);
    target.addEventListener("blur", hide);
    return () => {
      hide();
      target.removeEventListener("keydown", onKeyDown);
      target.removeEventListener("keyup", hide);
      target.removeEventListener("pointerdown", hide);
      target.removeEventListener("blur", hide);
    };
  }, [target]);
}
