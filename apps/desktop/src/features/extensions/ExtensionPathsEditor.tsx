import { useEffect, useState } from "react";
import { Button, Textarea } from "@mantine/core";
import type { RunnerSettings } from "../../models/topology";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";

export function ExtensionPathsEditor({ settings, kind, disabled, onSave }: {
  settings: RunnerSettings; kind: "instructions" | "skills"; disabled: boolean; onSave: (paths: RunnerSettings["paths"]) => Promise<boolean>;
}) {
  const { t } = useLocale(); const p = useProduct();
  const configuredInstructions = settings.paths.instruction_files.join("\n");
  const configuredSkills = settings.paths.skill_roots.join("\n");
  const [instructions, setInstructions] = useState(configuredInstructions);
  const [skills, setSkills] = useState(configuredSkills);
  useEffect(() => { setInstructions(configuredInstructions); }, [configuredInstructions, settings.target.config_path, settings.target.client_id, settings.target.server_url]);
  useEffect(() => { setSkills(configuredSkills); }, [configuredSkills, settings.target.config_path, settings.target.client_id, settings.target.server_url]);
  return <form className="extension-editor" onSubmit={event => {
    event.preventDefault(); if (disabled) return;
    const lines = (value: string) => value.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
    // Saving one category must not apply the other category's unsaved draft.
    void onSave(kind === "instructions" ? { ...settings.paths, instruction_files: lines(instructions) } : { ...settings.paths, skill_roots: lines(skills) });
  }}>
    <div hidden={kind !== "instructions"}><Textarea id="instruction-files" label={t("extensions.instructionFiles")} rows={3} value={instructions} onChange={event => setInstructions(event.currentTarget.value)} disabled={disabled} spellCheck={false} /></div>
    <div hidden={kind !== "skills"}><Textarea id="skill-roots" label={t("extensions.skillRoots")} rows={3} value={skills} onChange={event => setSkills(event.currentTarget.value)} disabled={disabled} spellCheck={false} /></div>
    <Button className="primary-button" type="submit" data-webcodex-action="save-runner-settings" disabled={disabled}>{p("save")}</Button>
  </form>;
}
