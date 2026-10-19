import { afterEach, describe, expect, it } from "vitest";
import { installDesktopWindowBehavior } from "./desktop-window-behavior";

function dispatch(target: EventTarget, event: Event) {
  target.dispatchEvent(event);
  return event.defaultPrevented;
}
const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
const menu = () => new MouseEvent("contextmenu", { bubbles: true, cancelable: true });

describe("installDesktopWindowBehavior", () => {
  let uninstall: (() => void) | undefined;
  afterEach(() => {
    uninstall?.();
    uninstall = undefined;
    window.getSelection()?.removeAllRanges();
    document.body.innerHTML = "";
  });

  it("suppresses the page context menu but keeps it for editable fields and selected text", () => {
    document.body.innerHTML = `<p id="text">Runner fixture-runner</p><button id="other">Other</button><input id="field" /><input id="checkbox" type="checkbox" /><textarea id="area"></textarea><div id="editable" contenteditable="true"></div><div id="plain" contenteditable="plaintext-only"></div>`;
    uninstall = installDesktopWindowBehavior();
    const text = document.getElementById("text")!;
    expect(dispatch(text, menu())).toBe(true);
    expect(dispatch(document.getElementById("field")!, menu())).toBe(false);
    expect(dispatch(document.getElementById("checkbox")!, menu())).toBe(true);
    expect(dispatch(document.getElementById("area")!, menu())).toBe(false);
    expect(dispatch(document.getElementById("editable")!, menu())).toBe(false);
    expect(dispatch(document.getElementById("plain")!, menu())).toBe(false);

    const range = document.createRange();
    range.selectNodeContents(text);
    window.getSelection()!.addRange(range);
    expect(dispatch(text, menu())).toBe(false);
    expect(dispatch(document.getElementById("other")!, menu())).toBe(true);
  });

  it("blocks accidental reload and print while leaving deliberate and ordinary keys alone", () => {
    uninstall = installDesktopWindowBehavior();
    expect(dispatch(window, key({ key: "F5" }))).toBe(true);
    expect(dispatch(window, key({ key: "r", ctrlKey: true }))).toBe(true);
    expect(dispatch(window, key({ key: "R", metaKey: true }))).toBe(true);
    expect(dispatch(window, key({ key: "p", ctrlKey: true }))).toBe(true);

    expect(dispatch(window, key({ key: "R", ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(dispatch(window, key({ key: "F5", ctrlKey: true }))).toBe(false);
    expect(dispatch(window, key({ key: "r" }))).toBe(false);
    expect(dispatch(window, key({ key: "c", ctrlKey: true }))).toBe(false);
  });

  it("removes every listener when uninstalled", () => {
    installDesktopWindowBehavior()();
    expect(dispatch(window, key({ key: "F5" }))).toBe(false);
    expect(dispatch(document.body, menu())).toBe(false);
  });
});
