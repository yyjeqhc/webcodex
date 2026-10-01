import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

// This fixture is migration evidence, never an execution alias registry. Reading
// real shipped App HTML prevents its tests and mocks agreeing on a stale name.
const renames = JSON.parse(fs.readFileSync(new URL("../../crates/webcodex-tool-contracts/src/tests/fixtures/v05_tool_renames.json", import.meta.url), "utf8"));
for (const file of ["mcp_agent_continuation_app.html", "mcp_goal_plan_app.html", "mcp_job_terminal_continuation_app.html", "mcp_work_result_app.html"]) {
  test(`${file}: shipped request targets use the current canonical catalog`, () => {
    const source = fs.readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
    const names = [...source.matchAll(/\bname\s*:\s*["']([a-z][a-z0-9_]*)["']/g)].map(match => match[1]);
    assert(names.length > 0, "fixture must inspect actual named requests");
    for (const name of names) {
      assert(!Object.hasOwn(renames, name), `retired request target: ${name}`);
    }
    assert(names.some(name => Object.values(renames).includes(name)), "must exercise a renamed request");
  });
}
