import type { WindowDetail, WindowsResponse } from "../model/types.js";
import type { RuntimeV2Client } from "./client.js";

export function fetchWindows(
  client: RuntimeV2Client,
  project?: string,
  signal?: AbortSignal,
) {
  return client.post<WindowsResponse>(
    "windows",
    { limit: 2_000, ...(project ? { project } : {}) },
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
