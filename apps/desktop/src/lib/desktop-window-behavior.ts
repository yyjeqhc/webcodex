const EDITABLE = "input, textarea, select, [contenteditable=''], [contenteditable='true']";

/** Production WebViews otherwise expose page chrome that does not belong to a
 * desktop window: the browser context menu, print, and an accidental reload
 * that would discard unsaved drafts. Editable fields and selected text keep the
 * native menu for copy and paste. Ctrl/⌘+Shift+R remains a deliberate reload. */
export function installDesktopWindowBehavior(target: Window = window): () => void {
  const onContextMenu = (event: MouseEvent) => {
    const element = event.target instanceof Element ? event.target : null;
    if (element?.closest(EDITABLE)) return;
    const selection = target.getSelection();
    if (selection && !selection.isCollapsed && selection.toString().trim()) return;
    event.preventDefault();
  };
  const onKeyDown = (event: KeyboardEvent) => {
    const modifier = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    const reload = (event.key === "F5" && !modifier && !event.shiftKey) || (modifier && !event.shiftKey && !event.altKey && key === "r");
    const print = modifier && !event.altKey && key === "p";
    if (reload || print) event.preventDefault();
  };
  target.addEventListener("contextmenu", onContextMenu);
  target.addEventListener("keydown", onKeyDown);
  return () => {
    target.removeEventListener("contextmenu", onContextMenu);
    target.removeEventListener("keydown", onKeyDown);
  };
}
