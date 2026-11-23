import type { CloudflareConnectionStatus, TunnelConnection } from "../../models/connections-tools";

export function defaultGeminiProfile(profiles: TunnelConnection[]) {
  const candidates = profiles.filter(profile => profile.provider && profile.provider.kind !== "openai");
  return candidates.find(profile => profile.autostart)
    ?? candidates.find(profile => profile.oauth_dynamic_registration_enabled)
    ?? candidates[0];
}

export function verifiedPublicOrigin(profile: TunnelConnection, status: CloudflareConnectionStatus | null, requireRegistration: boolean) {
  if (!status?.public_origin || status.lifecycle !== "running" || status.profile_id !== profile.id ||
      status.configuration_id !== profile.configuration_id || status.configured_revision !== profile.revision ||
      profile.server_restart_required || (requireRegistration && (!status.registration_available || status.applied_revision !== profile.revision))) return null;
  try {
    const url = new URL(status.public_origin);
    if (url.protocol === "https:" && url.origin === status.public_origin &&
        (profile.provider?.kind !== "cloudflare_named" || profile.provider.public_origin === url.origin)) return url.origin;
  } catch { /* A malformed observation supplies no public address. */ }
  return null;
}
