import { useEffect, useRef, useState } from "react";
import { Button, TextInput } from "@mantine/core";
import { X } from "lucide-react";
import type { RunnerSettings } from "../../models/topology";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useInstructionsText } from "../../i18n/instructions";
import { useConnectionsTools } from "../../i18n/connections-tools";

type PathKind = "instructions" | "skills";
const draft = (paths: string[]) => paths.length ? [...paths] : [""];
const normalized = (paths: string[]) => [...new Set(paths.map(path => path.trim()).filter(Boolean))];

export function ExtensionPathsEditor({ settings, kind, disabled, onSave, onBrowse }: {
  settings: RunnerSettings; kind: PathKind; disabled: boolean;
  onSave: (paths: RunnerSettings["paths"]) => Promise<boolean>;
  onBrowse: (kind: PathKind) => Promise<string | null>;
}) {
  const { t } = useLocale(); const p = useProduct(); const i = useInstructionsText(); const c = useConnectionsTools();
  const configuredInstructions = settings.paths.instruction_files.join("\n");
  const configuredSkills = settings.paths.skill_roots.join("\n");
  const [instructions, setInstructions] = useState(() => draft(settings.paths.instruction_files));
  const [skills, setSkills] = useState(() => draft(settings.paths.skill_roots));
  const [browsing, setBrowsing] = useState(false);
  const target = JSON.stringify(settings.target);
  const currentTarget = useRef<string | null>(target);
  currentTarget.current = target;
  useEffect(() => () => { currentTarget.current = null; }, []);
  useEffect(() => { setInstructions(draft(settings.paths.instruction_files)); }, [configuredInstructions, settings.target.config_path, settings.target.client_id, settings.target.server_url]);
  useEffect(() => { setSkills(draft(settings.paths.skill_roots)); }, [configuredSkills, settings.target.config_path, settings.target.client_id, settings.target.server_url]);
  const paths = kind === "instructions" ? instructions : skills;
  const setPaths = kind === "instructions" ? setInstructions : setSkills;
  const saved = kind === "instructions" ? settings.paths.instruction_files : settings.paths.skill_roots;
  const dirty = JSON.stringify(normalized(paths)) !== JSON.stringify(saved);
  const atCapacity = normalized(paths).length >= 16;
  const locked = disabled || browsing;
  const browse = async () => {
    if (locked || atCapacity) return;
    setBrowsing(true);
    try {
      const path = await onBrowse(kind);
      if (!path || currentTarget.current !== target) return;
      setPaths(current => current.includes(path) ? current : [...current.filter(value => value.trim()), path].slice(0, 16));
    } finally {
      if (currentTarget.current !== null) setBrowsing(false);
    }
  };
  return <form className="extension-editor extension-path-editor" onSubmit={event => {
    event.preventDefault(); if (locked || !dirty) return;
    // Save only this category against the observed target and other saved paths.
    void onSave(kind === "instructions" ? { ...settings.paths, instruction_files: normalized(paths) } : { ...settings.paths, skill_roots: normalized(paths) });
  }}>
    <p className="extension-scope-note">{p("sharedPaths")}</p>
    {(["instructions", "skills"] as const).map(category => {
      const entries = category === "instructions" ? instructions : skills;
      const update = category === "instructions" ? setInstructions : setSkills;
      const id = category === "instructions" ? "instruction-files" : "skill-roots";
      const label = t(category === "instructions" ? "extensions.instructionFiles" : "extensions.skillRoots");
      return <div className="extension-path-list" key={category} hidden={kind !== category}>
        {entries.map((path, index) => <div className="extension-path-row" key={index}>
          <span className="extension-path-number" aria-hidden="true">{index + 1}</span>
          <TextInput id={index ? `${id}-${index}` : id} aria-label={index ? `${label} ${index + 1}` : label} value={path} onChange={event => { const value = event.currentTarget.value; update(current => current.map((entry, position) => position === index ? value : entry)); }} disabled={locked} placeholder={p("absolutePath")} maxLength={4096} spellCheck={false} autoComplete="off" />
          <button type="button" className="secondary-button extension-path-remove" aria-label={`${c("remove")} ${label} ${index + 1}`} onClick={() => update(current => current.filter((_, position) => position !== index))} disabled={locked}><X size={18} aria-hidden="true" /></button>
        </div>)}
      </div>;
    })}
    <div className="extension-path-add-actions">
      <Button type="button" variant="default" disabled={locked || atCapacity} onClick={() => void browse()}>{p(kind === "instructions" ? "addInstructions" : "addSkill")}</Button>
      <Button type="button" variant="subtle" disabled={locked || paths.length >= 16} onClick={() => setPaths(current => [...current, ""])}>{p("enterPath")}</Button>
      {paths.length >= 16 && <span>{p("pathLimit")}</span>}
    </div>
    <div className="extension-path-save-actions">
      <Button className="primary-button" type="submit" data-webcodex-action="save-runner-settings" disabled={locked || !dirty}>{p("save")}</Button>
      {dirty && <><Button type="button" variant="default" disabled={locked} onClick={() => setPaths(draft(saved))}>{p("cancel")}</Button><span role="status">{i("unsaved")}</span></>}
    </div>
  </form>;
}
