import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { DesktopState } from "../../models/topology";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { BrowserRuntimeStatus } from "./BrowserRuntimeStatus";
import { productText } from "../../i18n/product";
import { LocaleProvider } from "../../i18n/locale";

const query = vi.hoisted(() => vi.fn());
vi.mock("../workspace/WorkspaceContext", () => ({ workspaceQuery: query }));
const state = {
  workspace_runner: { client_id: "runner-one" },
  readiness: { runtime_ready: true },
  current_operation: null,
} as DesktopState;

beforeEach(() => { vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US"); });
const view = (value: DesktopState) => <DesktopMantineProvider><LocaleProvider><BrowserRuntimeStatus state={value}/></LocaleProvider></DesktopMantineProvider>;
const show = (value: DesktopState) => render(view(value));

it("shows a verified Runner-scoped Browser count", async () => {
  query.mockResolvedValue({ client_id: "runner-one", state: "fresh", count: 2, owned_count: 1, attached_count: 1 });
  show(state);
  expect(await screen.findByText(/2 active on this Runner/)).toBeInTheDocument();
  expect(query).toHaveBeenCalledWith({ kind: "browser_status" });
  expect(screen.getByText(/1 WebCodex-owned, 1 externally attached/)).toBeInTheDocument();
});

it("never represents denied or invalid responses as zero", async () => {
  query.mockResolvedValue({ client_id: "another-runner", state: "fresh", count: 0, owned_count: 0, attached_count: 0 });
  show(state);
  expect(await screen.findByText(/count is unknown/)).toBeInTheDocument();
  query.mockRejectedValue({ code: "workspace_permission_denied" });
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(query).toHaveBeenCalledTimes(2));
  expect(screen.getByText(/count is unknown/)).toBeInTheDocument();
});

it("disables observations when the Runtime is not ready", () => {
  show({
    ...state, readiness: { ...state.readiness, runtime_ready: false },
  });
  expect(screen.getByText(/Runtime is stopped or busy/)).toBeInTheDocument();
  expect(query).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Refresh" })).toBeDisabled();
});

it("scopes a count to the selected Runner and does not reuse an old observation", async () => {
  query.mockResolvedValueOnce({ client_id: "runner-one", state: "fresh", count: 2, owned_count: 1, attached_count: 1 });
  let release!: (value: unknown) => void;
  query.mockImplementationOnce(() => new Promise(resolve => { release = resolve; }));
  const mounted = show(state);
  expect(await screen.findByText(/2 active on this Runner/)).toBeInTheDocument();
  mounted.rerender(view({
    ...state, workspace_runner: { client_id: "runner-two" },
  } as DesktopState));
  expect(await screen.findByText("Checking Browser runtimes…")).toBeInTheDocument();
  expect(screen.queryByText(/2 active on this Runner/)).not.toBeInTheDocument();
  release({ client_id: "runner-two", state: "fresh", count: 1, owned_count: 1, attached_count: 0 });
  expect(await screen.findByText(/1 active on this Runner/)).toBeInTheDocument();
});

it("has localized copy for the supported Desktop languages", () => {
  for (const locale of ["en-US", "zh-CN", "de-DE", "fr-FR", "ja-JP", "ko-KR", "zh-TW"]) {
    expect(productText(locale, "browserRuntimeTitle").length).toBeGreaterThan(0);
    expect(productText(locale, "browserRuntimeActive")).toContain("{count}");
    expect(productText(locale, "browserRuntimeUnknown").length).toBeGreaterThan(0);
  }
});


it("rejects contradictory ownership counts and never offers a close action", async () => {
  query.mockResolvedValue({ client_id: "runner-one", state: "fresh", count: 2, owned_count: 3, attached_count: 0 });
  show(state);
  expect(await screen.findByText(/count is unknown/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Close all/i })).not.toBeInTheDocument();
});

it("does not accept missing ownership evidence as a fresh count", async () => {
  query.mockResolvedValue({ client_id: "runner-one", state: "fresh", count: 1 });
  show(state);
  expect(await screen.findByText(/count is unknown/)).toBeInTheDocument();
});
