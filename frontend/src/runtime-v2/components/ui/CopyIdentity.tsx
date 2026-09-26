import { Copy } from "lucide-react";
import { useState } from "react";
import { writeClipboardText } from "../../../runtime_api.js";
import { translate, type RuntimeLanguage } from "../../../runtime_i18n.js";

/** Always show the complete identity, including when clipboard access is unavailable. */
export function CopyIdentity({ value, label, language }: { value: string; label: string; language: RuntimeLanguage }) {
  const [notice, setNotice] = useState("");
  const t = (text: string) => translate(text, language);
  return <span className="copy-identity">
    <code>{value}</code>
    <button type="button" aria-label={t("Copy") + " " + label} title={t("Copy") + " " + label}
      onClick={async () => setNotice(await writeClipboardText(value) ? "Copied" : "Copy unavailable; select the text to copy.")}>
      <Copy size={14} />
    </button>
    {notice && <small role="status">{t(notice)}</small>}
  </span>;
}
