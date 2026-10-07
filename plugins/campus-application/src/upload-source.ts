import { homedir } from "node:os";
import { posix, win32 } from "node:path";

export type UploadSource = { project: string; project_root?: string | undefined };
export type UploadLocation = { project: string; path: string };

// This only derives a Browser project-relative path from explicit caller context.
// Browser still independently authorizes the Project and canonical file (including
// symlinks). The provider cwd and profile directory never imply an upload Project.
export function resolveUploadLocation(value: string, source?: UploadSource): UploadLocation | undefined {
  if (!source?.project.trim() || source.project.length > 512 || source.project.includes("\0")
    || !value || value.includes("\0") || Buffer.byteLength(value) > 4096) return;
  const expanded = value.startsWith("~/") ? posix.join(homedir(), value.slice(2)) : value;
  if (expanded.startsWith("~")) return;
  const absolute = posix.isAbsolute(expanded) || win32.isAbsolute(expanded);
  let path = expanded;
  if (absolute) {
    const rawRoot = source.project_root;
    if (!rawRoot || rawRoot.includes("\0")) return;
    const root = rawRoot.startsWith("~/") ? posix.join(homedir(), rawRoot.slice(2)) : rawRoot;
    const paths = win32.isAbsolute(expanded) && !posix.isAbsolute(expanded) ? win32 : posix;
    if (!paths.isAbsolute(root)) return;
    path = paths.relative(paths.resolve(root), paths.resolve(expanded));
    if (!path || paths.isAbsolute(path)) return;
  }
  const segments = path.replaceAll("\\", "/").split("/");
  if (segments.some(part => part === "..") || segments[0] === "" || /^[A-Za-z]:/.test(path)) return;
  path = segments.filter(part => part !== "." && part !== "").join("/");
  if (!path) return;
  return { project: source.project, path };
}

export function uploadFileName(value: string): string {
  return value.split(/[\\/]/).pop() ?? value;
}
