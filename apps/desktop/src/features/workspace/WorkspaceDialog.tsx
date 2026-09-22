import { Modal } from "@mantine/core";
import { useEffect, useRef, type ReactNode } from "react";
import { useProduct } from "../../i18n/product";

export function WorkspaceDialog({ title, onClose, children, busy = false }: { title: string; onClose: () => void; children: ReactNode; busy?: boolean }) {
  const p = useProduct();
  const trigger = useRef<HTMLElement | null>(document.activeElement instanceof HTMLElement ? document.activeElement : null);
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
  return <Modal opened onClose={() => { if (!busy) onClose(); }} title={title} centered size={800} returnFocus={false}
    closeOnEscape={!busy} closeOnClickOutside={!busy} withCloseButton={!busy}
    closeButtonProps={{ "aria-label": p("close") }}
    classNames={{ content: "workspace-dialog", header: "workspace-dialog-header", body: "workspace-dialog-body" }}
    overlayProps={{ backgroundOpacity: 0.42 }}>
    {children}
  </Modal>;
}
