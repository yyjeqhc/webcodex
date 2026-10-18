import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode, useState } from "react";
import { MantineProvider } from "@mantine/core";
import { describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../i18n/locale";
import { WorkspaceDialog } from "./WorkspaceDialog";

function nestedDialog(busy: boolean, close: () => void) {
  return <MantineProvider><LocaleProvider><section aria-label="Extensions"><section role="tabpanel" aria-label="MCP Providers"><article>
    <WorkspaceDialog title="Add MCP Provider" onClose={close} busy={busy}>
      <label htmlFor="fixture-provider-name">Provider Name</label><input id="fixture-provider-name" />
      <label htmlFor="fixture-provider-secret">Credential</label><input id="fixture-provider-secret" type="password" />
    </WorkspaceDialog>
  </article></section></section></LocaleProvider></MantineProvider>;
}

describe("Mantine modal accessibility boundary", () => {
  it("portals nested provider dialogs to the document root without changing field semantics", () => {
    const view = render(nestedDialog(false, vi.fn()));
    const dialog = screen.getByRole("dialog", { name: "Add MCP Provider" });
    expect(document.body.contains(dialog)).toBe(true);
    expect(within(view.container).queryByRole("dialog")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("textbox", { name: "Provider Name" })).toBeEnabled();
    expect(within(dialog).getByLabelText("Credential")).toHaveAttribute("type", "password");
  });

  it("keeps busy dismissal fenced after portal rendering", () => {
    const close = vi.fn(); const view = render(nestedDialog(true, close));
    expect(within(screen.getByRole("dialog")).queryByRole("button")).toBeNull();
    expect(close).not.toHaveBeenCalled();
    view.rerender(nestedDialog(false, close));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button"));
    expect(close).toHaveBeenCalledTimes(1);
  });
});

function DialogReplacement() {
  const [view, setView] = useState<"window" | "session" | null>(null);
  return <><button onClick={() => setView("window")}>Open work details</button>
    {view === "window" && <WorkspaceDialog key="window" title="Window" onClose={() => setView(null)}>
      <button onClick={() => setView("session")}>Open associated session</button>
    </WorkspaceDialog>}
    {view === "session" && <WorkspaceDialog key="session" title="Session" onClose={() => setView(null)}>
      <button onClick={() => setView(null)}>Close session</button>
    </WorkspaceDialog>}
  </>;
}

it.each([false, true])("restores the original work opener after Window → Session replacement (StrictMode=%s)", async strict => {
  const app = <MantineProvider env="test"><LocaleProvider><DialogReplacement /></LocaleProvider></MantineProvider>;
  render(strict ? <StrictMode>{app}</StrictMode> : app);
  const opener = screen.getByRole("button", { name: "Open work details" });
  for (let attempt = 0; attempt < 2; attempt++) {
    opener.focus(); fireEvent.click(opener);
    const associated = await screen.findByRole("button", { name: "Open associated session" });
    associated.focus(); fireEvent.click(associated);
    expect(await screen.findByRole("dialog", { name: "Session" })).toBeInTheDocument();
    expect(associated.isConnected).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Close session" }));
    await waitFor(() => expect(opener).toHaveFocus());
  }
});
