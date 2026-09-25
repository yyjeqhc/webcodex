import { describe, expect, it } from "vitest";
import { displayProjectPath, projectPresentationName } from "../src/ui/projectPresentation";
import { projectDisplayName } from "../src/runtime-v2/model/format";

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
