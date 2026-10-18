import { Modal } from "@mantine/core";
import { useEffect, useRef, type ReactNode } from "react";
import { useProduct } from "../../i18n/product";

// A replacement dialog inherits the first opener before its predecessor's DOM
// disappears. Weak keys keep this metadata scoped to mounted modal roots.
const returnTargets = new WeakMap<HTMLElement, HTMLElement>();
function captureReturnTarget(): HTMLElement | null {
  const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const owner = active?.closest<HTMLElement>(".workspace-dialog-owner");
  return (owner && returnTargets.get(owner)) || active;
}

export function WorkspaceDialog({ title, onClose, children, busy = false }: { title: string; onClose: () => void; children: ReactNode; busy?: boolean }) {
  const p = useProduct();
  const trigger = useRef<HTMLElement | null>(captureReturnTarget());
  const focusTimer = useRef<number | null>(null);
  useEffect(() => {
    if (focusTimer.current !== null) window.clearTimeout(focusTimer.current);
    return () => {
      const element = trigger.current;
      focusTimer.current = window.setTimeout(() => {
        // A replacement dialog owns focus now. Do not race Mantine's next focus
        // trap by restoring the old trigger behind it (for example Window →
        // Session details or cancel → reopen Runtime confirmation).
        const activeModal = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"]'))
          .some(dialog => getComputedStyle(dialog).display !== "none" && getComputedStyle(dialog).visibility !== "hidden");
        if (!activeModal && element?.isConnected) element.focus();
      }, 0);
    };
  }, []);
  return <Modal className="workspace-dialog-owner" ref={node => {
    if (node && trigger.current) returnTargets.set(node, trigger.current);
  }} opened onClose={() => { if (!busy) onClose(); }} title={title} centered size={800} returnFocus={false}
    closeOnEscape={!busy} closeOnClickOutside={!busy} withCloseButton={!busy}
    closeButtonProps={{ "aria-label": p("close") }}
    classNames={{ content: "workspace-dialog", header: "workspace-dialog-header", body: "workspace-dialog-body" }}
    overlayProps={{ backgroundOpacity: 0.42 }}>
    {children}
  </Modal>;
}
