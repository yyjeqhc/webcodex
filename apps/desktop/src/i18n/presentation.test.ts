import { describe, expect, it } from "vitest";
import type { DesktopError } from "../models/topology";
import { messages, type MessageKey } from "./locale";
import { desktopCommandDiagnostics, desktopErrorPresentation } from "./presentation";

const zh = (key: MessageKey) => messages["zh-CN"][key];

describe("Desktop command error presentation", () => {
  it("uses the safe command phase to present the failing subsystem", () => {
    const error: DesktopError = {
      code: "webcodex_command_failed",
      message: "WebCodex did not complete the requested operation",
      next_action: "Retry.",
      details: {
        phase: "server_init",
        logical_command: "server init",
        executable: "webcodex.exe",
        exit_code: 17,
        reason_code: "path_outside_allowed_roots",
      },
    };
    expect(desktopErrorPresentation(error, zh).title).toBe(messages["zh-CN"]["error.serverTitle"]);
    expect(desktopCommandDiagnostics(error)).toEqual({
      phase: "server_init",
      logicalCommand: "server init",
      executable: "webcodex.exe",
      exitCode: 17,
      reasonCode: "path_outside_allowed_roots",
    });
  });

  it("presents a missing explicit local Project as a Project recovery problem", () => {
    const error: DesktopError = {
      code: "project_not_ready",
      message: "Local setup requires an explicit project folder",
      next_action: "Choose the project folder that this Runner should manage, then retry setup.",
    };
    expect(desktopErrorPresentation(error, zh)).toEqual({
      title: messages["zh-CN"]["error.projectTitle"],
      action: messages["zh-CN"]["error.projectAction"],
    });
  });

  it("never projects arbitrary or secret-bearing detail fields", () => {
    const error: DesktopError = {
      code: "webcodex_command_failed",
      message: "failed",
      next_action: "Retry.",
      details: {
        phase: "login",
        logical_command: "login",
        executable: "C:\\private\\webcodex.exe",
        reason_code: "authorization_token_deadbeef",
        stderr: "Bearer super-secret-value",
      },
    };
    expect(desktopCommandDiagnostics(error)).toEqual({
      phase: "login",
      logicalCommand: "login",
      executable: undefined,
      exitCode: undefined,
      reasonCode: undefined,
    });
  });
});
