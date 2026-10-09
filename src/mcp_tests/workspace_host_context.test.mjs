import test from "node:test";
import assert from "node:assert/strict";
import { app } from "./app_test_support.mjs";

for (const filename of ["mcp_workbench_app.html", "mcp_work_result_app.html"]) {
  test(`${filename}: initialization acknowledgement precedes inline size notifications`, async () => {
    const view = app(filename);
    await view.reply(view.sent[0], { protocolVersion: "2026-01-26", hostContext: { displayMode: "inline" } });
    const initialized = view.sent.findIndex(message => message.method === "ui/notifications/initialized");
    const sized = view.sent.findIndex(message => message.method === "ui/notifications/size-changed");
    assert.ok(initialized >= 0 && sized > initialized);
  });
  test(`${filename}: Host display changes cannot come from another frame or survive disposal`, async () => {
    const view = app(filename);
    await view.reply(view.sent[0], { protocolVersion: "2026-01-26", hostContext: { theme: "dark", displayMode: "fullscreen" } });
    assert.equal(view.document.documentElement.style.colorScheme, "dark");
    assert.equal(view.document.documentElement.getAttribute("data-display-mode"), "fullscreen");
    view.notification("ui/notifications/host-context-changed", { theme: "light", displayMode: "inline" }, {});
    assert.equal(view.document.documentElement.style.colorScheme, "dark");
    view.notification("ui/notifications/host-context-changed", { theme: "light", displayMode: "inline" });
    assert.equal(view.document.documentElement.style.colorScheme, "light");
    assert.equal(view.document.documentElement.getAttribute("data-display-mode"), "inline");
    const sizes = () => view.sent.filter(message => message.method === "ui/notifications/size-changed");
    assert.deepEqual(JSON.parse(JSON.stringify(sizes().at(-1).params)), { height: 400 });
    const count = sizes().length;
    view.notification("ui/notifications/host-context-changed", { displayMode: "inline" });
    assert.equal(sizes().length, count);
    view.notification("ui/notifications/host-context-changed", { theme: "invalid", displayMode: "invalid" });
    assert.equal(view.document.documentElement.style.colorScheme, "light");
    assert.equal(sizes().length, count);
    view.notification("ui/resource-teardown", {});
    view.notification("ui/notifications/host-context-changed", { theme: "dark", displayMode: "fullscreen" });
    assert.equal(view.document.documentElement.style.colorScheme, "light");
    assert.equal(sizes().length, count);
  });
}
