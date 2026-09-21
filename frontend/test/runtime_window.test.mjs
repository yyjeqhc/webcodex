import test from "node:test";
import assert from "node:assert/strict";
import {
  windowDateTimeLabel,
  windowAgeLabel,
  runtimeProjectClientId,
  renderWindowActivityRows,
  createWindowCard,
  renderWindowActiveRequests,
  renderWindowLinkedSessions,
  renderSessionWindowCorrelationLinks,
  formatWindowDetailFields,
  renderWindowCards,
  renderProjectWindowCards,
} from "../dist/runtime_window.js";
import {
  formatProjectWindowStatusText,
} from "../dist/runtime_navigation.js";

function createMockElement(tag = "div") {
  const listeners = new Map();
  let directText = "";
  const el = {
    tagName: tag.toUpperCase(),
    className: "",
    title: "",
    dataset: {},
    children: [],
    childNodes: [],
    attributes: {},
    get textContent() {
      if (this.childNodes.length === 0) return directText;
      return this.childNodes.map((c) => (typeof c === "string" ? c : c.textContent || "")).join("");
    },
    set textContent(val) {
      directText = String(val);
      this.childNodes = [];
      this.children = [];
    },
    get firstChild() {
      return this.childNodes[0] || null;
    },
    get childElementCount() {
      return this.children.length;
    },
    classList: {
      classes: new Set(),
      add(cls) {
        cls.split(/\s+/).filter(Boolean).forEach((c) => this.classes.add(c));
      },
      remove(cls) {
        this.classes.delete(cls);
      },
      contains(cls) {
        return this.classes.has(cls);
      },
      toggle(cls, force) {
        if (force === undefined) {
          if (this.classes.has(cls)) {
            this.classes.delete(cls);
            return false;
          }
          this.classes.add(cls);
          return true;
        }
        if (force) {
          this.classes.add(cls);
          return true;
        }
        this.classes.delete(cls);
        return false;
      },
    },
    appendChild(child) {
      if (child.className) {
        child.classList.add(child.className);
      }
      this.children.push(child);
      this.childNodes.push(child);
      return child;
    },
    removeChild(child) {
      const idx = this.childNodes.indexOf(child);
      if (idx >= 0) this.childNodes.splice(idx, 1);
      const cidx = this.children.indexOf(child);
      if (cidx >= 0) this.children.splice(cidx, 1);
      return child;
    },
    setAttribute(key, value) {
      this.attributes[key] = String(value);
      if (key === "class") this.classList.add(String(value));
    },
    getAttribute(key) {
      return this.attributes[key] ?? null;
    },
    addEventListener(event, handler) {
      if (!listeners.has(event)) listeners.set(event, []);
      listeners.get(event).push(handler);
    },
    click() {
      const list = listeners.get("click") || [];
      for (const handler of list) handler({ type: "click" });
    },
    querySelector(selector) {
      const results = this.querySelectorAll(selector);
      return results[0] || null;
    },
    querySelectorAll(selector) {
      const found = [];
      const match = (elem) => {
        if (selector.startsWith(".")) {
          const cls = selector.slice(1);
          if (elem.classList?.contains(cls) || (elem.className && elem.className.includes(cls))) {
            found.push(elem);
          }
        } else if (selector.toLowerCase() === elem.tagName.toLowerCase()) {
          found.push(elem);
        }
        for (const c of elem.children) match(c);
      };
      for (const child of this.children) match(child);
      return found;
    },
  };
  return el;
}

function withMockDom(fn) {
  const originalDoc = globalThis.document;
  globalThis.document = {
    createElement(tag) {
      return createMockElement(tag);
    },
  };
  try {
    return fn();
  } finally {
    globalThis.document = originalDoc;
  }
}

test("windowDateTimeLabel formats timestamps or shows unavailable message", () => {
  assert.equal(windowDateTimeLabel(null), "time unavailable");
  assert.equal(windowDateTimeLabel(0), "time unavailable");
  assert.equal(windowDateTimeLabel(-1), "time unavailable");
  assert.equal(windowDateTimeLabel("abc"), "time unavailable");
  assert.equal(windowDateTimeLabel(null, "zh-CN"), "时间不可用");

  const formatted = windowDateTimeLabel(1700000000000, "en");
  assert.ok(formatted.length > 0 && formatted !== "time unavailable");
});

test("windowAgeLabel delegates to runtimeWindowActivityLabel", () => {
  const now = 100_000;
  assert.equal(windowAgeLabel(null, now), "No WebPi activity");
  assert.equal(windowAgeLabel(now - 500, now), "just now");
  assert.equal(windowAgeLabel(now - 15000, now), "15s ago");
});

test("runtimeProjectClientId extracts client id from agent projects", () => {
  assert.equal(runtimeProjectClientId("agent:runner-42:project-name"), "runner-42");
  assert.equal(runtimeProjectClientId("agent:remote-box:repo:sub"), "remote-box");
  assert.equal(runtimeProjectClientId("plain-project"), "");
  assert.equal(runtimeProjectClientId("local:test"), "");
  assert.equal(runtimeProjectClientId(null), "");
});

test("renderWindowActivityRows renders activity cards with facts and trace copy", () => {
  withMockDom(() => {
    const container = document.createElement("div");
    let copiedTrace = "";

    const activities = [
      {
        tool_name: "read_files",
        started_at_ms: 1700000000000,
        status: "success",
        project: "agent:runner-1:proj",
        activity_presentation: "work",
        activity_kind: "read",
        meaningful: true,
        service_ms: 45,
        next_call_gap_ms: 12,
        cycle_ms: 57,
        workflow_sessions: [
          { workflow_session_id: "wc_sess_1", relation: "direct" },
        ],
        server_trace_id: "trace-xyz-123",
      },
      {
        method: "unknown_method",
        status: "failed",
        activity_presentation: "transport",
        recorder_gap_session_id: "wc_sess_gap",
        response_streaming: true,
        window_transition_kind: "overlap",
      },
    ];

    renderWindowActivityRows(container, activities, {
      compact: true,
      language: "en",
      onCopyTrace: (traceId) => {
        copiedTrace = traceId;
      },
    });

    assert.equal(container.children.length, 2);

    const first = container.children[0];
    assert.ok(first.className.includes("compact"));
    assert.equal(first.querySelector("strong")?.textContent, "read_files");

    const chips = first.querySelectorAll(".chip").map((c) => c.textContent);
    assert.ok(chips.includes("success"));
    assert.ok(chips.includes("agent:runner-1:proj"));
    assert.ok(chips.includes("work"));
    assert.ok(chips.includes("read"));
    assert.ok(chips.includes("meaningful"));
    assert.ok(chips.includes("service 45 ms"));
    assert.ok(chips.includes("next gap 12 ms"));
    assert.ok(chips.includes("cycle 57 ms"));

    const copyButton = first.querySelector(".window-trace-copy");
    assert.ok(copyButton);
    copyButton.click();
    assert.equal(copiedTrace, "trace-xyz-123");

    const second = container.children[1];
    assert.ok(second.className.includes("recorder-gap"));
    assert.ok(second.querySelector(".window-gap-note"));
    assert.equal(second.querySelector("strong")?.textContent, "unknown_method");
    const secondChips = second.querySelectorAll(".chip").map((c) => c.textContent);
    assert.ok(secondChips.includes("transport"));
    assert.ok(secondChips.includes("streaming timing unavailable"));
    assert.ok(secondChips.includes("overlap from previous"));
  });
});

test("createWindowCard builds card and fires select callback", () => {
  withMockDom(() => {
    let selected = "";
    const card = createWindowCard(
      {
        client_window_key: "0123456789abcdef0123456789abcdef",
        active_count: 2,
        last_tool_call_at_ms: 9000,
        last_meaningful_activity_at_ms: 8000,
        linked_session_count: 3,
        recorder_gap_count: 1,
      },
      "0123456789abcdef0123456789abcdef",
      (key) => {
        selected = key;
      },
      10000,
    );

    assert.ok(card);
    assert.ok(card.className.includes("selected"));
    assert.equal(card.getAttribute("aria-current"), "true");
    assert.match(card.querySelector("strong")?.textContent || "", /Window 01234567…cdef/);
    assert.equal(card.querySelector(".chip")?.textContent, "2 active");

    card.click();
    assert.equal(selected, "0123456789abcdef0123456789abcdef");

    assert.equal(createWindowCard({}, "", () => {}), null);
  });
});

test("renderWindowActiveRequests renders active requests or empty placeholder", () => {
  withMockDom(() => {
    const container = document.createElement("div");
    let copiedTrace = "";

    renderWindowActiveRequests(container, [], { now: 10000 });
    assert.equal(container.textContent, "No WebPi request is currently active.");

    const requests = [
      {
        tool_name: "terminal_run",
        project: "agent:r1:p",
        started_at_ms: 9500,
        elapsed_ms: 500,
        server_trace_id: "trace-run-1",
      },
    ];
    renderWindowActiveRequests(container, requests, {
      now: 10000,
      onCopyTrace: (t) => {
        copiedTrace = t;
      },
    });

    assert.equal(container.children.length, 1);
    assert.equal(container.querySelector("strong")?.textContent, "terminal_run");
    assert.match(
      container.querySelector(".muted")?.textContent || "",
      /agent:r1:p · started just now · 500 ms elapsed/,
    );

    const btn = container.querySelector(".window-trace-copy");
    assert.ok(btn);
    btn.click();
    assert.equal(copiedTrace, "trace-run-1");
  });
});

test("renderWindowLinkedSessions renders session buttons and empty placeholder", () => {
  withMockDom(() => {
    const container = document.createElement("div");
    let openedSession = null;

    renderWindowLinkedSessions(container, [], () => {});
    assert.equal(container.textContent, "No authorized Workflow Session links.");

    const sessions = [
      {
        title: "Feature Session",
        workflow_session_id: "sess-1",
        lifecycle: "active",
        project: "p1",
        relations: ["ancestor"],
      },
    ];

    renderWindowLinkedSessions(container, sessions, (s) => {
      openedSession = s;
    });
    assert.equal(container.children.length, 1);
    const btn = container.children[0];
    assert.equal(btn.querySelector("strong")?.textContent, "Feature Session");
    assert.match(btn.querySelector(".muted")?.textContent || "", /sess-1 · active · p1 · ancestor/);

    btn.click();
    assert.deepEqual(openedSession, sessions[0]);
  });
});

test("renderSessionWindowCorrelationLinks renders correlated window cards", () => {
  withMockDom(() => {
    const container = document.createElement("div");
    let selectedKey = "";

    const links = [
      {
        client_window_key: "abcdef0123456789abcdef0123456789",
        source: "cli",
        last_seen_at_ms: 9500,
        recorder_gap_count: 2,
      },
    ];

    renderSessionWindowCorrelationLinks(
      container,
      links,
      (k) => {
        selectedKey = k;
      },
      10000,
    );
    assert.equal(container.children.length, 1);
    const card = container.children[0];
    assert.ok(card.className.includes("recorder-gap"));
    assert.match(
      card.querySelector(".muted")?.textContent || "",
      /cli · last WebPi activity just now · 2 recorder gap/,
    );

    card.click();
    assert.equal(selectedKey, "abcdef0123456789abcdef0123456789");
  });
});

test("formatWindowDetailFields produces populated metrics and fallback texts", () => {
  assert.equal(formatWindowDetailFields(null), null);

  const detail = {
    client_window_key: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    source: "web",
    active_count: 2,
    last_tool_call_at_ms: 9000,
    last_meaningful_activity_at_ms: 8000,
    sessions_returned: 3,
    sessions_truncated: true,
    activity_returned: 10,
    activity_truncated: false,
  };

  const fields = formatWindowDetailFields(detail, "", 10000, "en");
  assert.ok(fields);
  assert.equal(fields.title, "Window 01234567…cdef");
  assert.equal(fields.source, "web");
  assert.equal(fields.activeCount, "2");
  assert.equal(fields.activeStatus, "Active request");
  assert.match(fields.lastCall, /ago|just now/);
  assert.match(fields.lastMeaningful, /ago|just now/);
  assert.equal(fields.linkedStatus, "3 Sessions · bounded");
  assert.equal(fields.activityStatus, "10 events");

  const emptyDetail = {
    client_window_key: "",
    active_count: 0,
  };
  const emptyFields = formatWindowDetailFields(emptyDetail, "fallback-key");
  assert.ok(emptyFields);
  assert.equal(emptyFields.title, "Window fallback-key");
  assert.equal(emptyFields.lastCall, "No completed tools/call activity");
  assert.equal(emptyFields.lastMeaningful, "No meaningful WebPi work recorded");
  assert.equal(emptyFields.activeStatus, "No active request");
});

test("renderWindowCards populates container with window cards", () => {
  withMockDom(() => {
    const container = document.createElement("div");
    let clicked = "";
    const rows = [
      { client_window_key: "w1", active_count: 1 },
      { client_window_key: "w2", active_count: 0 },
    ];
    renderWindowCards(container, rows, "w1", (k) => {
      clicked = k;
    });
    assert.equal(container.children.length, 2);
    assert.ok(container.children[0].className.includes("selected"));
    assert.equal(container.children[0].getAttribute("aria-current"), "true");
    container.children[1].click();
    assert.equal(clicked, "w2");
  });
});

test("createWindowCard correctly renders 0 linked Sessions, active count, and last meaningful work", () => {
  withMockDom(() => {
    let clickedKey = "";
    const card = createWindowCard(
      {
        client_window_key: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        active_count: 1,
        source: "openai-session",
        last_seen_at_ms: 9800,
        last_tool_call_at_ms: 9500,
        last_meaningful_activity_at_ms: 9000,
        linked_session_count: 0,
        recorder_gap_count: 0,
      },
      "",
      (key) => {
        clickedKey = key;
      },
      10000,
      "en",
    );

    assert.ok(card);
    assert.equal(card.className.includes("selected"), false);
    assert.equal(card.getAttribute("aria-current"), null);
    assert.equal(card.querySelector(".chip")?.textContent, "1 active");
    assert.match(card.textContent, /Last WebPi call/);
    assert.match(card.textContent, /Last meaningful work/);
    assert.match(card.textContent, /0 linked Sessions/);

    card.click();
    assert.equal(clickedKey, "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
  });
});

test("renderProjectWindowCards populates project-scoped window cards with inspector title", () => {
  withMockDom(() => {
    const container = document.createElement("div");
    let inspectedKey = "";
    const rows = [
      {
        client_window_key: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        active_count: 0,
        source: "openai-session",
        last_seen_at_ms: 9000,
        last_meaningful_activity_at_ms: 9000,
        linked_session_count: 0,
      },
    ];

    renderProjectWindowCards(
      container,
      rows,
      (k) => {
        inspectedKey = k;
      },
      10000,
      "en",
    );

    assert.equal(container.children.length, 1);
    const card = container.children[0];
    assert.equal(card.title, "Open Window inspector");
    assert.equal(card.className.includes("selected"), false);
    assert.equal(card.querySelector(".chip")?.textContent, "openai-session");
    assert.match(card.textContent, /0 linked Sessions/);

    card.click();
    assert.equal(inspectedKey, "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789");
  });
});

test("formatProjectWindowStatusText formats non-truncated and truncated status in en and zh-CN", () => {
  assert.equal(formatProjectWindowStatusText(5, 5, false, "en"), "");
  assert.equal(formatProjectWindowStatusText(5, 5, false, "zh-CN"), "");
  assert.equal(formatProjectWindowStatusText(10, 25, true, "en"), "10 of 25 Windows · bounded");
  assert.equal(formatProjectWindowStatusText(10, 25, true, "zh-CN"), "10 / 25 个窗口 · 有界");
});
