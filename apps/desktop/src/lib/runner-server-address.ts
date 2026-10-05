/** Presentation validation for remote-machine onboarding; Core owns admission. */
export function runnerServerAddress(value: string): {
  origin: string | null;
  issue: "invalid" | "loopback" | "openai" | null;
} {
  try {
    const url = new URL(value.trim());
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password
      || url.pathname !== "/" || url.search || url.hash) return { origin: null, issue: "invalid" };
    const host = url.hostname.toLowerCase().replace(/\.$/, "");
    if (host === "localhost" || host.endsWith(".localhost") || host === "0.0.0.0"
      || /^127\./.test(host) || host === "[::1]" || host === "[::]"
      || /^\[::ffff:7f[\da-f]{2}:[\da-f]+\]$/.test(host)) {
      return { origin: url.origin, issue: "loopback" };
    }
    if (["openai.com", "chatgpt.com"].some(domain => host === domain || host.endsWith(`.${domain}`))) {
      return { origin: url.origin, issue: "openai" };
    }
    return { origin: url.origin, issue: null };
  } catch {
    return { origin: null, issue: "invalid" };
  }
}
