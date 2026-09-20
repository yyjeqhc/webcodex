import { KeyRound, LockKeyhole } from "lucide-react";
import { FormEvent, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";

type Props = {
  language: RuntimeLanguage;
  onConnect: (token: string, remember: boolean) => void;
};

export function AuthGate({ language, onConnect }: Props) {
  const t = (value: string) => translate(value, language);
  const [value, setValue] = useState("");
  const [remember, setRemember] = useState(true);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const token = value.trim();
    if (token) onConnect(token, remember);
  };

  return (
    <main className="auth-shell">
      <section className="auth-card">
        <div className="auth-brand"><span className="brand-mark">W</span><strong>WebCodex</strong></div>
        <div className="auth-icon"><LockKeyhole size={23} /></div>
        <span className="eyebrow">{t("Runtime workspace")}</span>
        <h1>{t("Connect to your workspace")}</h1>
        <p>{t("Use your access key to open this workspace.")}</p>
        <form onSubmit={submit}>
          <label htmlFor="runtime-v2-token">{t("Access key")}</label>
          <div className="auth-field">
            <KeyRound size={16} />
            <input
              id="runtime-v2-token"
              data-testid="runtime-token-input"
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              placeholder={t("Runtime Bearer credential")}
            />
          </div>
          <label className="checkbox-line auth-remember">
            <input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} />
            {t("Remember for this tab")}
          </label>
          <button className="auth-connect" type="submit" disabled={!value.trim()}>{t("Connect")}</button>
        </form>
        <details className="auth-advanced">
          <summary>{t("Advanced")}</summary>
          <p>{t("The key stays in this tab and is cleared when you lock the workspace or close the tab.")}</p>
          <p>{t("Project, Session, Window, communication and Runtime views remain constrained by the existing server authority checks.")}</p>
        </details>
      </section>
    </main>
  );
}
