import { projectPresentationName } from "../../ui/projectPresentation.js";

export function shortId(value: string, head = 10, tail = 5): string {
  if (!value || value.length <= head + tail + 1) return value;
  return `${value.slice(0, head)}…${value.slice(-tail)}`;
}

export function relativeTime(timestamp: number | undefined, now = Date.now()): string {
  if (!timestamp) return "—";
  const milliseconds = timestamp > 10_000_000_000 ? timestamp : timestamp * 1_000;
  const delta = Math.max(0, now - milliseconds);
  if (delta < 5_000) return "now";
  if (delta < 60_000) return `${Math.floor(delta / 1_000)}s`;
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h`;
  return `${Math.floor(delta / 86_400_000)}d`;
}

export function absoluteTime(timestamp: number | undefined): string {
  if (!timestamp) return "—";
  const milliseconds = timestamp > 10_000_000_000 ? timestamp : timestamp * 1_000;
  return new Date(milliseconds).toLocaleString();
}

export function clockTime(timestamp: number | undefined): string {
  if (!timestamp) return "—";
  const milliseconds = timestamp > 10_000_000_000 ? timestamp : timestamp * 1_000;
  return new Date(milliseconds).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function durationText(milliseconds: number | undefined): string {
  if (milliseconds === undefined || milliseconds === null || milliseconds < 0) return "—";
  if (milliseconds < 1_000) return `${milliseconds}ms`;
  const seconds = Math.floor(milliseconds / 1_000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return remainder ? `${minutes}m ${remainder}s` : `${minutes}m`;
}

export function projectDisplayName(name: string | undefined, id: string, path?: string): string {
  return projectPresentationName({ name, id, path });
}
