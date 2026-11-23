import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { CONNECTIONS_TOOLS_MESSAGES, connectionsToolsText } from "../../i18n/connections-tools";
import { LocaleProvider } from "../../i18n/locale";
import { PRODUCT_LOCALES } from "../../i18n/product";
import { connectionFixture } from "../../test/connections-fixtures";
import { ConnectionCard } from "./ConnectionCard";

const path = "C:\\Users\\example\\AppData\\Local\\webcodex\\runtime\\regular-tunnel-runtime";

beforeEach(() => {
  localStorage.removeItem("webcodex.desktop.locale");
});

describe("Connection advanced details", () => {

  it.each(PRODUCT_LOCALES)("acknowledges a requested stop or restart immediately in %s", (locale) => {
    localStorage.setItem("webcodex.desktop.locale", locale);
    const profile = connectionFixture({ lifecycle: "running", ready: true });
    const props = { profile, canStart: true, busy: true, copied: false,
      onAction: () => undefined, onEdit: () => undefined, onDelete: () => undefined,
      onCopy: () => undefined, onSettings: () => undefined };
    const { container, rerender } = render(
      <LocaleProvider><ConnectionCard {...props} pendingAction="stop" /></LocaleProvider>,
    );
    const card = container.querySelector("article.connection-profile")!;
    const status = card.querySelector('[role="status"]')!;
    expect(card).toHaveAttribute("aria-busy", "true");
    expect(status).toHaveTextContent(connectionsToolsText(locale, "stopping"));
    expect(status.querySelector(".status-dot")).not.toHaveClass("ready");

    rerender(<LocaleProvider><ConnectionCard {...props} pendingAction="restart" /></LocaleProvider>);
    expect(status).toHaveTextContent(connectionsToolsText(locale, "restarting"));
    expect(card).toHaveAttribute("aria-busy", "true");

    rerender(<LocaleProvider><ConnectionCard {...props} busy={false} pendingAction={null} /></LocaleProvider>);
    expect(card).toHaveAttribute("aria-busy", "false");
  });
  it.each(PRODUCT_LOCALES)("keeps the two-column metadata and translated labels in %s", (locale) => {
    localStorage.setItem("webcodex.desktop.locale", locale);
    const profile = connectionFixture({
      runtime_directory: path,
      startup_timings_ms: { cli_total: 5540, initial_poll: 5510, poll_attempts: 1, poll_request_ms: 5502 },
      tunnel_ready: true,
      local_mcp_ready: false,
      process_ready: true,
      process_started: true,
    });
    const { container } = render(
      <LocaleProvider>
        <ConnectionCard
          profile={profile}
          canStart={true}
          busy={false}
          copied={false}
          onAction={() => undefined}
          onEdit={() => undefined}
          onDelete={() => undefined}
          onCopy={() => undefined}
          onSettings={() => undefined}
        />
      </LocaleProvider>,
    );
    const details = container.querySelector("dl.connection-meta-grid");
    expect(details).not.toBeNull();
    const children = Array.from(details!.children);
    expect(children.length % 2).toBe(0);
    for (let index = 0; index < children.length; index += 2) {
      expect(children[index].tagName).toBe("DT");
      expect(children[index + 1].tagName).toBe("DD");
    }
    const pairs = new Map(Array.from(details!.querySelectorAll("dt"), (dt) => [
      dt.textContent,
      dt.nextElementSibling?.textContent,
    ]));
    for (const key of [
      "profileId", "pid", "processStarted", "processReady", "cliStartup",
      "firstControlPlanePoll", "initialPollRequests", "pollHttpTime",
      "pollRetryWait", "otherStartupWork", "tunnelReady",
      "localMcpReady", "mcpAddress", "runtimeDirectory",
    ] as const) {
      expect(pairs.has(connectionsToolsText(locale, key))).toBe(true);
    }
    expect(pairs.get(connectionsToolsText(locale, "processReady"))).toBe(connectionsToolsText(locale, "yes"));
    expect(pairs.get(connectionsToolsText(locale, "localMcpReady"))).toBe(connectionsToolsText(locale, "no"));
    expect(pairs.get(connectionsToolsText(locale, "pollRetryWait"))).toBe("8 ms");
    expect(pairs.get(connectionsToolsText(locale, "runtimeDirectory"))).toBe(path);
    expect(details?.querySelectorAll("dd.connection-meta-path code")).toHaveLength(2);
    expect(details?.querySelector("dd.connection-meta-path code:last-child")).toBeTruthy();
  });

  it.each(PRODUCT_LOCALES)("explains binding changes without suggesting an MCP restart in %s", (locale) => {
    localStorage.setItem("webcodex.desktop.locale", locale);
    for (const reason_code of [
      "tunnel_protected_history_binding_changed",
      "tunnel_receipts_binding_changed",
      "tunnel_archive_binding_changed",
    ]) {
      const profile = connectionFixture({
        host_mode: "embedded",
        lifecycle: "error",
        ready: false,
        last_error: "local_mcp_unavailable",
        reason_code,
        tunnel_ready: false,
        local_mcp_ready: false,
      });
      const { container, unmount } = render(
        <LocaleProvider>
          <ConnectionCard profile={profile} canStart={true} busy={false} copied={false}
            onAction={() => undefined} onEdit={() => undefined} onDelete={() => undefined}
            onCopy={() => undefined} onSettings={() => undefined} />
        </LocaleProvider>,
      );
      const recovery = container.querySelector(".connection-recovery")!;
      expect(recovery.textContent).toContain(connectionsToolsText(locale, "tunnelBindingChanged"));
      expect(recovery.textContent).toContain(connectionsToolsText(locale, "tunnelBindingRecovery"));
      expect(recovery.textContent).not.toContain(connectionsToolsText(locale, "localMcpRecovery"));
      expect(recovery.textContent).not.toContain(connectionsToolsText(locale, "restartUncertainRecovery"));
      expect(container.querySelector(".connection-actions .primary-button")).toBeNull();
      unmount();
    }
  });

  it("omits unavailable optional diagnostics without creating empty labels", () => {
    const profile = connectionFixture({ startup_timings_ms: null, local_mcp_ready: null, runtime_directory: null });
    const { container } = render(
      <LocaleProvider>
        <ConnectionCard profile={profile} canStart={true} busy={false} copied={false}
          onAction={() => undefined} onEdit={() => undefined} onDelete={() => undefined}
          onCopy={() => undefined} onSettings={() => undefined} />
      </LocaleProvider>,
    );
    const details = container.querySelector("dl.connection-meta-grid")!;
    const labels = Array.from(details.querySelectorAll("dt"), (dt) => dt.textContent);
    expect(labels).not.toContain(CONNECTIONS_TOOLS_MESSAGES.initialPollRequests[1]);
    expect(labels).not.toContain(CONNECTIONS_TOOLS_MESSAGES.runtimeDirectory[1]);
    expect(Array.from(details.querySelectorAll("dt")).find(dt => dt.textContent === CONNECTIONS_TOOLS_MESSAGES.localMcpReady[1])?.nextElementSibling?.textContent).toBe("—");
  });
});
