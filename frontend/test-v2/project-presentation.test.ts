import { describe, expect, it } from "vitest";
import { displayProjectPath, projectPresentationName, projectFamilyName, type PresentableProject } from "../src/ui/projectPresentation";
import { projectDisplayName } from "../src/runtime-v2/model/format";

it("uses one indexed source lookup per Window instead of rescanning the Project array", () => {
  let lookups = 0;
  class CountedIndex extends Map<string, PresentableProject> {
    override get(key: string) { lookups++; return super.get(key); }
  }
  const primary: PresentableProject = { id:"agent:r:primary", client_id:"r", name:"Primary" };
  const projects: PresentableProject[] = [primary, ...Array.from({length:1999},(_,i)=>({
    id:`agent:r:worktree-${i}`,client_id:"r",name:`Worktree ${i}`,
    lineage:{kind:"managed_worktree_source" as const,source_project_id:"primary",base_sha:"a".repeat(40)}
  }))];
  const index = new CountedIndex(projects.map(project=>[project.id,project]));
  for (let i=0;i<2000;i++) expect(projectFamilyName(projects[1+i%1999],index)).toBe("Primary");
  expect(lookups).toBe(2000);
  console.info("PROJECT_LOOKUP projects=2000 windows=2000 source_map_gets=2000 array_scans=0");
});

describe("shared Windows project presentation", () => {
  it("preserves canonical inputs while rendering disk and UNC paths", () => {
    const path = String.raw`\\?\C:\repo`;
    expect(displayProjectPath(path)).toBe(String.raw`C:\repo`);
    expect(path).toBe(String.raw`\\?\C:\repo`);
    expect(displayProjectPath(String.raw`\\?\UNC\server\share`)).toBe(String.raw`\\server\share`);
    expect(displayProjectPath("/work/A")).toBe("/work/A");
  });
  it("repairs generic root names and preserves explicit names", () => {
    expect(projectPresentationName({ path: "C:\\", name: "project" })).toBe("C:");
    expect(projectDisplayName("Project", "agent:mini:root", "D:\\")).toBe("D:");
    expect(projectDisplayName("My disk", "agent:mini:root", "D:\\")).toBe("My disk");
    expect(projectPresentationName({ path: String.raw`\\?\UNC\server\share`, name: "Project" })).toBe("share");
  });
});
