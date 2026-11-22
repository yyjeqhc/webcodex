import { useCallback, useEffect, useRef, useState } from "react";
import { normalizeDesktopError } from "../../i18n/presentation";
import type { DesktopError } from "../../models/topology";
import { desktopApi } from "../../lib/desktop-api";
import type { CloudflareConnectionRequest, CloudflareConnectionStatus, TunnelConnection } from "../../models/connections-tools";

// The card remounts at an Environment, profile, revision or Server availability
// change. Only loopback status reads poll. Mutations use the displayed fences.
export function useCloudflareControl(profile: TunnelConnection, canStart: boolean) {
  const [status, setStatus] = useState<CloudflareConnectionStatus | null>(null);
  const [readFailure, setReadFailure] = useState<DesktopError | null>(null);
  const [actionFailure, setActionFailure] = useState<DesktopError | null>(null);
  const [actionPending, setActionPending] = useState<"start" | "stop" | "restart" | null>(null);
  const sequence = useRef(0);
  const reading = useRef(false);
  const mutating = useRef(false);

  const refresh = useCallback(async () => {
    if (!canStart || reading.current || mutating.current) return;
    reading.current = true;
    const attempt = ++sequence.current;
    try {
      const next = await desktopApi.cloudflareConnection({ action: "status", profile_id: profile.id });
      if (attempt !== sequence.current) return;
      if (next.profile_id !== profile.id || next.configuration_id !== profile.configuration_id) throw {code: "cloudflare_revision_stale"};
      setStatus(next);
      setReadFailure(null);
      if (next.lifecycle === "running") setActionFailure(null);
    } catch (error) {
      if (attempt !== sequence.current) return;
      setStatus(null);
      setReadFailure(normalizeDesktopError(error));
    } finally {
      reading.current = false;
    }
  }, [canStart, profile.id, profile.configuration_id]);

  useEffect(() => {
    if (!canStart) return;
    void refresh();
    const timer = window.setInterval(() => void refresh(), 3000);
    return () => {
      ++sequence.current;
      window.clearInterval(timer);
    };
  }, [canStart, refresh]);

  const run = async (action: "start" | "stop" | "restart") => {
    if (!status || (action !== "stop" && !profile.configuration_id) || mutating.current || !canStart) return;
    mutating.current = true;
    const attempt = ++sequence.current;
    setActionPending(action);
    setReadFailure(null);
    setActionFailure(null);
    setStatus(null);
    const stop: CloudflareConnectionRequest = {
      action: "stop", profile_id: profile.id, server_instance_id: status.server_instance_id,
      process_generation: status.process_generation,
    };
    try {
      if (action === "restart") {
        const stopped = await desktopApi.cloudflareConnection(stop);
        if (attempt !== sequence.current) return;
        if (stopped.profile_id !== profile.id || stopped.server_instance_id !== status.server_instance_id ||
            stopped.process_generation !== status.process_generation || stopped.configured_revision !== profile.revision ||
            stopped.lifecycle !== "stopped") throw new Error();
      }
      const next = await desktopApi.cloudflareConnection(action === "stop" ? stop : {
        action: "start", profile_id: profile.id, server_instance_id: status.server_instance_id, expected_revision: profile.revision, expected_generation: status.process_generation, expected_configuration_id: profile.configuration_id!,
      });
      if (attempt !== sequence.current) return;
      if (next.configuration_id !== profile.configuration_id || next.profile_id !== profile.id || next.server_instance_id !== status.server_instance_id) throw new Error();
      setStatus(next);
    } catch (error) {
      if (attempt === sequence.current) setActionFailure(normalizeDesktopError(error));
    } finally {
      mutating.current = false;
      if (attempt === sequence.current) setActionPending(null);
    }
  };
  const failure = readFailure ?? actionFailure;
  return { status, failure, failureCode: failure?.code ?? null, actionPending, refresh, run };
}
