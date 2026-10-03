import { useEffect, useState } from "react";
import { WorkspaceEmptyState } from "../../components/WorkspaceEmptyState";
import { useProduct } from "../../i18n/product";
import type { InstructionSummary } from "../../models/workspace";
import { workspaceQuery } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { ManagedInstructionsPanel } from "./ManagedInstructionsPanel";
import type { ExtensionPanelContext } from "./extension-panel-types";

export function InstructionConfiguration({ active, settings, disabled, onState, onRefresh }: ExtensionPanelContext & { active: boolean }) {
  return <ManagedInstructionsPanel active={active} settings={settings} disabled={disabled} onState={onState} onEnabled={onRefresh} />;
}

export function InstructionsPreview({ catalog, projectLabel, onOpenDocument }: ExtensionPanelContext) {
  const p = useProduct();
  return <>
    {catalog?.instructions.files.map(file => <article className="extension-row" key={`${file.source_scope}:${file.path}`}><div><strong>{file.source_scope === "runner" ? p("globalInstructions") : file.path.split(/[\\/]/).pop()}</strong><span>{file.source_scope === "runner" ? "Runner" : projectLabel} · {p("available")}</span><details><summary>{p("details")}</summary><code>{file.path}</code></details></div><button className="secondary-button" onClick={() => onOpenDocument(file)} aria-label={`${p("open")} ${file.path.split(/[\\/]/).pop()}`}>{p("open")}</button></article>)}
    {catalog?.instructions.scan_complete && !catalog.instructions.files.length && <WorkspaceEmptyState kind="document" message={p("noInstructions")} />}
    {catalog && !catalog.instructions.scan_complete && <p className="workspace-notice">{p("unavailable")}</p>}
  </>;
}

export function SkillsPreview({ catalog, projectLabel }: ExtensionPanelContext) {
  const p = useProduct(); const skills = catalog?.skills.catalog?.skills || [];
  return <>
    {skills.map(skill => <article className="extension-row" key={skill.skill_id}><div><strong>{skill.name}</strong><p>{skill.description}</p><span>{skill.source_scope === "project" ? projectLabel : "Runner"} · {p("available")}</span></div><details><summary>{p("details")}</summary><code>{skill.trust}</code></details></article>)}
    {catalog?.skills.available && !skills.length && <WorkspaceEmptyState kind="skill" message={p("noProjectSkills")} />}
    {catalog && !catalog.skills.available && <p className="workspace-notice">{p("unavailable")}</p>}
    {catalog?.skills.catalog?.truncated && <p>{p("partial")}</p>}
  </>;
}

export function InstructionDocument({ project, file, onClose }: { project: string; file: InstructionSummary; onClose: () => void }) {
  const p = useProduct(); const [content, setContent] = useState<string | null>(null); const [failed, setFailed] = useState(false);
  useEffect(() => { let cancelled = false; void workspaceQuery<{ content: string }>({ kind: "instruction", project, source_scope: file.source_scope, path: file.path, fingerprint: file.fingerprint }).then(value => { if (!cancelled) setContent(value.content); }).catch(() => { if (!cancelled) setFailed(true); }); return () => { cancelled = true; }; }, [project, file]);
  return <WorkspaceDialog title={file.source_scope === "runner" ? p("globalInstructions") : file.path} onClose={onClose}>{failed ? <p role="alert">{p("loadError")}</p> : content === null ? <p role="status">{p("loading")}</p> : <pre className="instruction-content">{content}</pre>}{file.truncated && <p>{p("partial")}</p>}</WorkspaceDialog>;
}
