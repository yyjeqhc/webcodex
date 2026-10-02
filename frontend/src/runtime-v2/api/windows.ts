import type { WindowDetail, WindowsResponse } from "../model/types.js";
import type { RuntimeV2Client } from "./client.js";

export const PRIMARY_WINDOW_ACTIVITY_LIMIT = 80;

export const WINDOW_PAGE_SIZE = 50;
export type WindowSelection = { projects?: string[]; query?: string; client_window_key?: string };

export function fetchWindows(client: RuntimeV2Client, selection: WindowSelection & { offset?: number; limit?: number } = {}, signal?: AbortSignal) {
  return client.post<WindowsResponse>("windows", { projection: "inventory", limit: WINDOW_PAGE_SIZE, ...selection }, signal);
}

export function fetchWindowLiveness(client: RuntimeV2Client, selection: WindowSelection = {}, signal?: AbortSignal) {
  // The server bounds its in-flight registry independently of retained history.
  return client.post<WindowsResponse>("windows", { projection: "liveness", ...selection }, signal);
}

export function fetchWindowPrimaryDetail(
  client: RuntimeV2Client,
  key: string,
  signal?: AbortSignal,
) {
  return client.post<WindowDetail>(
    "window",
    {
      client_window_key: key,
      activity_limit: PRIMARY_WINDOW_ACTIVITY_LIMIT,
      detail_level: "primary",
    },
    signal,
  );
}

export function fetchWindowDetail(
  client: RuntimeV2Client,
  key: string,
  signal?: AbortSignal,
) {
  return client.post<WindowDetail>(
    "window",
    { client_window_key: key, activity_limit: 2_000 },
    signal,
  );
}
