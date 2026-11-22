import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopErrorAlert } from "./DesktopErrorAlert";
import { LocaleProvider } from "../i18n/locale";
import { normalizeDesktopError } from "../i18n/presentation";
import type { DesktopError } from "../models/topology";

function renderAlert(
  error: DesktopError,
  options: {
    locale?: "zh-CN" | "en-US";
    showSafeMessage?: boolean;
    actions?: Parameters<typeof DesktopErrorAlert>[0]["actions"];
  } = {},
) {
  localStorage.setItem("webcodex.desktop.locale", options.locale ?? "zh-CN");
  return render(
    <LocaleProvider>
      <DesktopErrorAlert
        error={error}
        showSafeMessage={options.showSafeMessage}
        actions={options.actions}
      />
    </LocaleProvider>,
  );
}

beforeEach(() => {
  localStorage.clear();
});

describe("DesktopErrorAlert", () => {
  it("renders three-layer localized Chinese diagnostics with stage and HTTP status for Server 400", () => {
    const onOpenDiagnostics = vi.fn();
    const onOpenRuntime = vi.fn();
    const onRetry = vi.fn();
    const error: DesktopError = {
      code: "server_request_failed",
      message: "Local user creation failed: The Server rejected the request parameters (HTTP 400)",
      next_action: "Check setup inputs",
      diagnostic: {
        component: "server",
        stage: "user_create",
        reason_code: "http_bad_request",
        recovery_kind: "user_action",
        safe_action: "open_diagnostics",
        retry_allowed: false,
        http_status: 400,
        operation_id: "desktop-operation-9",
        activity_sequence: 42,
      },
    };
    renderAlert(error, {
      locale: "zh-CN",
      actions: { onOpenDiagnostics, onOpenRuntime, onRetry },
    });

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Server 拒绝了此次请求 (HTTP 400)");
    expect(alert).toHaveTextContent("失败阶段：创建本地用户");
    expect(alert).toHaveTextContent("HTTP 400");
    expect(alert).toHaveTextContent("user_create");
    expect(alert).toHaveTextContent("desktop-operation-9");
    expect(alert).toHaveTextContent("#42");

    // Must offer service diagnostics, never blind Retry or unrelated Runtime selector.
    fireEvent.click(screen.getByRole("button", { name: "打开服务诊断" }));
    expect(onOpenDiagnostics).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "重试" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "打开 Runtime 与服务" })).not.toBeInTheDocument();
  });

  it("maps Tunnel errors to connection checks without offering unrelated Runtime settings", () => {
    const onCheckConnection = vi.fn();
    const onOpenRuntime = vi.fn();
    const error: DesktopError = {
      code: "cloudflare_server_unreachable",
      message: "Could not reach local Server",
      next_action: "Check local Server",
      diagnostic: {
        component: "tunnel",
        stage: "cloudflare_status",
        reason_code: "server_unreachable",
        recovery_kind: "user_action",
        safe_action: "check_connection",
        retry_allowed: true,
        resource_id: "cf-profile-1",
      },
    };
    renderAlert(error, {
      locale: "zh-CN",
      actions: { onCheckConnection, onOpenRuntime },
    });

    expect(screen.getByRole("alert")).toHaveTextContent("Tunnel 连接中断或无法访问网络");
    expect(screen.getByRole("alert")).toHaveTextContent("失败阶段：检查 Cloudflare 状态");
    fireEvent.click(screen.getByRole("button", { name: "重新检查连接" }));
    expect(onCheckConnection).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "打开 Runtime 与服务" })).not.toBeInTheDocument();
  });

  it("forbids blind retry and force takeover for ownership conflicts and uncertain writes", () => {
    const onRetry = vi.fn();
    const onStartServer = vi.fn();
    const onOpenDiagnostics = vi.fn();
    const onRefresh = vi.fn();

    const ownership = renderAlert(
      { code: "service_foreignservice", message: "Existing unowned service", next_action: "Inspect" },
      { locale: "en-US", actions: { onRetry, onStartServer, onOpenDiagnostics, onRefresh } },
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Cannot confirm local service or process ownership");
    expect(screen.getByRole("alert")).toHaveTextContent("Automatic retry is disabled");
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Start local Server" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Inspect conflicting services" }));
    expect(onOpenDiagnostics).toHaveBeenCalledOnce();
    ownership.unmount();

    renderAlert(
      { code: "operation_uncertain", message: "Outcome unknown", next_action: "Observe first" },
      { locale: "en-US", actions: { onRetry, onRefresh, onOpenDiagnostics } },
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Operation outcome could not be confirmed");
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Observe current state" }));
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("renders background contention as non-destructive auto-recovering status and never leaks secret text", () => {
    const secretError = normalizeDesktopError({
      code: "server_request_failed",
      message: "Authorization: Bearer wc_pat_ultra_secret_canary_value",
      next_action: "Inspect",
      diagnostic: {
        component: "server",
        stage: "user_authentication",
        reason_code: "wc_pat_bad_reason",
        recovery_kind: "user_action",
        safe_action: "open_diagnostics",
        retry_allowed: false,
      },
    });
    const rendered = renderAlert(secretError, { locale: "en-US", showSafeMessage: true });
    expect(screen.getByRole("alert")).not.toHaveTextContent("wc_pat_ultra_secret_canary_value");
    expect(screen.getByRole("alert")).not.toHaveTextContent("wc_pat_bad_reason");
    rendered.unmount();

    renderAlert(
      { code: "desktop_operation_busy", message: "Busy", next_action: "Wait" },
      { locale: "zh-CN", actions: { onRefresh: vi.fn() } },
    );
    const busyAlert = screen.getByRole("alert");
    expect(busyAlert.className).toContain("tone-info");
    expect(busyAlert).toHaveAttribute("data-error-recovery", "auto_recovering");
  });
});
