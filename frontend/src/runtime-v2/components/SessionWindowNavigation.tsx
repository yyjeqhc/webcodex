import { Button, Modal } from "@mantine/core";
import { useEffect, useState } from "react";
import { translate, type RuntimeLanguage } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { fetchSessionDetail } from "../api/sessions.js";
import { absoluteTime, shortId } from "../model/format.js";
import type { SessionDetail } from "../model/types.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";

type Props = {
  client: RuntimeV2Client;
  location: SessionLocation;
  language: RuntimeLanguage;
  onClose: () => void;
  onOpenWindow: (windowKey: string, sessionId: string) => void;
  onOpenRecord: (location: SessionLocation) => void;
  onUnauthorized: () => void;
};

/** Resolve only authoritative Session links; a Project match is not a Window link. */
export function SessionWindowNavigation({ client, location, language, onClose, onOpenWindow, onOpenRecord, onUnauthorized }: Props) {
  const t = (value: string) => translate(value, language);
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "error" | "denied">("loading");
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setStatus("loading");
    setDetail(null);
    void fetchSessionDetail(client, location.projectId, location.sessionId, controller.signal, 1).then(response => {
      if (controller.signal.aborted) return;
      if (response?.status === 401) { onUnauthorized(); return; }
      if (response?.status === 403 || response?.status === 404) { setStatus("denied"); return; }
      if (!response?.ok || response.data?.session_id !== location.sessionId) { setStatus("error"); return; }
      const data = response.data;
      if (data.window_activity_available && data.linked_windows.length === 1) {
        onOpenWindow(data.linked_windows[0].client_window_key, location.sessionId);
        return;
      }
      setDetail(data);
      setStatus("ready");
    });
    return () => controller.abort();
  }, [client, location.projectId, location.sessionId, onOpenWindow, onUnauthorized, revision]);

  return <Modal opened onClose={onClose} title={t("Session activity")} closeButtonProps={{ "aria-label": t("Close") }} centered>
    <p>{detail?.title || location.projectName}</p>
    {status === "loading" && <p role="status">{t("Finding linked Windows…")}</p>}
    {status === "denied" && <p role="status">{t("Session unavailable")}</p>}
    {status === "error" && <p role="status">{t("Could not load Session activity.")}</p>}
    {status === "ready" && detail && <>
      <p>{t(!detail.window_activity_available ? "Window activity unavailable" : detail.linked_windows.length ? "Choose a Window for this Session" : "No linked Windows in retained evidence.")}</p>
      {detail.window_activity_available && detail.linked_windows.map(row => <Button
        key={row.client_window_key} variant="default" fullWidth mb="xs"
        onClick={() => onOpenWindow(row.client_window_key, location.sessionId)}
        title={row.client_window_key}
      >Window {shortId(row.client_window_key)} · {absoluteTime(row.last_linked_at_ms)}</Button>)}
      <Button variant="subtle" onClick={() => onOpenRecord(location)}>{t("View Session record")}</Button>
    </>}
    {status === "error" && <Button onClick={() => setRevision(value => value + 1)}>{t("Retry")}</Button>}
  </Modal>;
}
