import { Drawer, TextInput } from "@mantine/core";
import { Pencil } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { translate, type RuntimeLanguage } from "../../runtime_i18n.js";

export function WindowLabelEditor({ name, label, mobile, editing, saving, error, language, onEditingChange, onSave }: {
  name: string; label?: string | null; mobile: boolean; editing: boolean; saving: boolean; error: string | null;
  language: RuntimeLanguage; onEditingChange: (value: boolean) => void; onSave: (value: string | null) => Promise<boolean>;
}) {
  const t = (text: string) => translate(text, language);
  const [draft, setDraft] = useState(label || "");
  const submitting = useRef(false);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const cancelled = useRef(false);
  const wasEditing = useRef(false);
  useEffect(() => {
    if (editing && !wasEditing.current) { setDraft(label || ""); cancelled.current = false; }
    wasEditing.current = editing;
  }, [editing, label]);
  const cancel = () => { cancelled.current = true; onEditingChange(false); };
  const save = async (value = draft) => {
    if (cancelled.current || submitting.current || saving) return;
    const normalized = value.trim() || null;
    if (normalized === (label ?? null)) { onEditingChange(false); return; }
    submitting.current = true;
    try { if (await onSave(normalized) && mounted.current) onEditingChange(false); }
    finally { submitting.current = false; }
  };
  const input = <TextInput autoFocus data-autofocus aria-label={t("Window name")} value={draft} maxLength={80}
    disabled={saving} error={error ? t(error) : undefined} description={`${[...draft].length}/80`}
    onChange={event => setDraft(event.currentTarget.value)}
    onBlur={() => { if (!mobile) void save(); }}
    onKeyDown={event => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); cancel(); }
      if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); void save(); }
    }} />;
  return <>
    <div className="window-board-title">
      {!mobile && editing ? input : <h2 className={label == null ? "window-board-default-name" : undefined}>{name}</h2>}
      {(!editing || mobile) && <button type="button" className="window-board-rename" aria-label={t("Rename Window")} disabled={saving}
        onClick={() => onEditingChange(true)}><Pencil size={16} /></button>}
    </div>
    {mobile && <Drawer opened={editing} onClose={cancel} position="bottom" size="auto" title={t("Rename Window")}
      closeOnEscape={!saving} closeOnClickOutside={!saving} withCloseButton={!saving}>
      <div className="window-label-sheet">{input}<div className="window-label-actions">
        <button type="button" disabled={saving} onClick={() => { setDraft(""); void save(""); }}>{t("Clear name")}</button>
        <button type="button" disabled={saving} onClick={() => void save()}>{t("Save")}</button>
      </div></div>
    </Drawer>}
  </>;
}
