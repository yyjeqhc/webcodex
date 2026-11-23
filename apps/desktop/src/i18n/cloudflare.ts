import type { MessageKey } from "./locale";

export const cloudflarePreparationMessages: Record<string, MessageKey> = {
  cloudflare_component_check: "gemini.check",
  cloudflare_component_download: "gemini.download",
  cloudflare_component_verify: "gemini.verify",
  cloudflare_transport_start: "gemini.start",
  cloudflare_network_unavailable: "gemini.networkFailure",
  cloudflare_download_failed: "gemini.downloadFailure",
  cloudflare_proxy_invalid: "gemini.proxyFailure",
  cloudflare_storage_unavailable: "gemini.storageFailure",
  cloudflare_version_unsupported: "gemini.versionFailure",
  cloudflare_platform_unsupported: "gemini.platformFailure",
  cloudflare_verification_failed: "gemini.verifyFailure",
  cloudflare_extraction_failed: "gemini.extractFailure",
  cloudflare_start_failed: "gemini.startFailure",
};
