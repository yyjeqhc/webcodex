import { beforeEach, expect, it, vi } from "vitest";
import { subscribeDesktopNavigation } from "./desktop-navigation";

const bridge = vi.hoisted(() => ({ listen: vi.fn(), read: vi.fn(), ack: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: bridge.listen }));
vi.mock("./desktop-api", () => ({ desktopApi: { readDesktopNavigation: bridge.read, acknowledgeDesktopNavigation: bridge.ack } }));

function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(r => { resolve = r; }); return { promise, resolve }; }
let handler: (event: { payload: unknown }) => void;
const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
beforeEach(() => {
  vi.resetAllMocks();
  bridge.listen.mockImplementation(async (_name: string, next: typeof handler) => { handler = next; return vi.fn(); });
  bridge.read.mockResolvedValue(null);
  bridge.ack.mockResolvedValue(undefined);
});

it("subscribes before consuming the pending initial target and acknowledges once", async () => {
  const subscribed = deferred<() => void>();
  bridge.listen.mockReturnValue(subscribed.promise);
  bridge.read.mockResolvedValue({ sequence: 1, target: "settings" });
  const apply = vi.fn();
  const stop = subscribeDesktopNavigation(apply);
  expect(bridge.read).not.toHaveBeenCalled();
  subscribed.resolve(vi.fn());
  await settle();
  expect(apply).toHaveBeenCalledExactlyOnceWith("settings");
  expect(bridge.ack).toHaveBeenCalledExactlyOnceWith(1);
  stop();
});

it("existing renderer navigation remains immediate and consumed intents are not replayed", async () => {
  const apply = vi.fn();
  const stop = subscribeDesktopNavigation(apply);
  await settle();
  bridge.read.mockResolvedValueOnce({ sequence: 2, target: "connections" });
  handler({ payload: "connections" });
  await settle();
  expect(apply).toHaveBeenCalledExactlyOnceWith("connections");
  handler({ payload: "activity" });
  await settle();
  expect(apply).toHaveBeenCalledTimes(1);
  stop();
});

it("coalesces navigation wakes during a read and applies the latest native intent last", async () => {
  const first = deferred<unknown>();
  bridge.read.mockReturnValueOnce(first.promise).mockResolvedValueOnce({ sequence: 2, target: "settings" });
  const apply = vi.fn();
  const stop = subscribeDesktopNavigation(apply);
  await settle();
  handler({ payload: "activity" });
  handler({ payload: "settings" });
  expect(bridge.read).toHaveBeenCalledTimes(1);
  first.resolve({ sequence: 1, target: "activity" });
  await settle();
  expect(apply.mock.calls).toEqual([["activity"], ["settings"]]);
  expect(bridge.ack.mock.calls).toEqual([[1], [2]]);
  stop();
});

it("disposed StrictMode mount never consumes a response intended for its replacement", async () => {
  const pending = deferred<unknown>();
  bridge.read.mockReturnValueOnce(pending.promise);
  const apply = vi.fn();
  const stop = subscribeDesktopNavigation(apply);
  await settle();
  stop();
  pending.resolve({ sequence: 1, target: "activity" });
  await settle();
  expect(apply).not.toHaveBeenCalled();
  expect(bridge.ack).not.toHaveBeenCalled();
  bridge.read.mockResolvedValue({ sequence: 1, target: "activity" });
  const stopNext = subscribeDesktopNavigation(apply);
  await settle();
  expect(apply).toHaveBeenCalledExactlyOnceWith("activity");
  stopNext();
});

it("late registration after disposal unlistens without consuming native navigation", async () => {
  const subscribed = deferred<() => void>();
  const unlisten = vi.fn();
  bridge.listen.mockReturnValue(subscribed.promise);
  const stop = subscribeDesktopNavigation(vi.fn());
  stop(); subscribed.resolve(unlisten); await settle();
  expect(unlisten).toHaveBeenCalledTimes(1);
  expect(bridge.read).not.toHaveBeenCalled();
});

it("ignores invalid targets/sequences and ignores stale read responses", async () => {
  const apply = vi.fn();
  const stop = subscribeDesktopNavigation(apply);
  await settle();
  handler({ payload: "run_process" });
  expect(bridge.read).toHaveBeenCalledTimes(1);
  for (const intent of [{ sequence: 1, target: "invalid" }, { sequence: -1, target: "activity" }, { sequence: 0x1_0000_0000, target: "activity" }]) {
    bridge.read.mockResolvedValueOnce(intent); handler({ payload: "activity" }); await settle();
  }
  expect(apply).not.toHaveBeenCalled(); expect(bridge.ack).not.toHaveBeenCalled();
  bridge.read.mockResolvedValueOnce({ sequence: 3, target: "settings" });
  handler({ payload: "settings" }); await settle();
  bridge.read.mockResolvedValueOnce({ sequence: 2, target: "activity" });
  handler({ payload: "activity" }); await settle();
  expect(apply).toHaveBeenCalledExactlyOnceWith("settings");
  stop();
});

it("delivery failure leaves intent retryable without timers or a retry storm", async () => {
  bridge.read.mockRejectedValueOnce(new Error("unavailable"));
  const apply = vi.fn(); const stop = subscribeDesktopNavigation(apply);
  await settle(); expect(bridge.read).toHaveBeenCalledTimes(1);
  bridge.read.mockResolvedValueOnce({ sequence: 1, target: "activity" });
  handler({ payload: "activity" }); await settle();
  expect(apply).toHaveBeenCalledExactlyOnceWith("activity"); stop();
});
