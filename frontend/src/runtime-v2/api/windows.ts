import type { WindowDetail, WindowsResponse } from "../model/types.js";
import type { RuntimeV2Client } from "./client.js";

export const PRIMARY_WINDOW_ACTIVITY_LIMIT = 80;

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
