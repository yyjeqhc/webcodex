const EDITABLE = "textarea, select, [contenteditable=''], [contenteditable='true'], [contenteditable='plaintext-only']";
const TEXT_INPUT_TYPES = new Set(["text", "search", "password", "email", "url", "tel", "number", "date", "datetime-local", "month", "time", "week"]);

/** Production WebViews otherwise expose page chrome that does not belong to a
 * desktop window: the browser context menu, print, and an accidental reload
 * that would discard unsaved drafts. Editable fields and selected text keep the
 * native menu for copy and paste. Ctrl/⌘+Shift+R remains a deliberate reload. */
export function installDesktopWindowBehavior(target: Window = window): () => void {
  const onContextMenu = (event: MouseEvent) => {
    const element = event.target instanceof Element ? event.target : null;
    const input = element?.closest("input");
    if (input && TEXT_INPUT_TYPES.has(input.type)) return;
    if (element?.closest(EDITABLE)) return;
    const selection = target.getSelection();
    // A selection elsewhere on the page must not reopen browser chrome here.
    if (element && selection && !selection.isCollapsed && selection.toString().trim()
      && selection.rangeCount > 0 && selection.getRangeAt(0).intersectsNode(element)) return;
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
