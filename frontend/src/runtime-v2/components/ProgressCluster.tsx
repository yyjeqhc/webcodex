import { ChevronDown, Code2, Search, ShieldCheck, TerminalSquare } from "lucide-react";
import type { ProgressGroup } from "../model/work.js";

export function ProgressCluster({ group }: { group: ProgressGroup }) {
  const icon =
    group.intent === "explored" ? <Search size={16} /> :
    group.intent === "edited" ? <Code2 size={16} /> :
    group.intent === "tested" ? <ShieldCheck size={16} /> :
    <TerminalSquare size={16} />;

  return (
    <details className={"tool-cluster " + (group.state === "success" ? "good" : "")}>
      <summary>
        <span className="tool-cluster-icon">{icon}</span>
        <span className="tool-cluster-title">
          <strong>{group.label}{group.count > 1 ? " · " + group.count : ""}</strong>
          <small>{group.latestSummary || group.tools.join(" · ") || group.state}</small>
        </span>
        <ChevronDown size={15} />
      </summary>
      <div className="tool-cluster-detail">
        {group.actor && <p><strong>{group.actor.name}</strong> · {group.actor.kind}</p>}
        {!!group.tools.length && (
          <div className="evidence-chip-row">{group.tools.map((tool) => <code key={tool}>{tool}</code>)}</div>
        )}
        {!!group.paths.length && (
          <div className="file-grid">{group.paths.map((path) => <code key={path}>{path}</code>)}</div>
        )}
      </div>
    </details>
  );
}
