import test from "node:test";
import assert from "node:assert/strict";
import {
  formatWindowEmptyState,
  formatWindowListStatusText,
  formatWindowDetailFields,
  renderWindowCards,
  renderProjectWindowCards,
  renderWindowActivityRows,
  renderWindowActiveRequests,
  renderSessionWindowCorrelationLinks,
} from "../dist/runtime_window.js";
import {
  runtimeWindowActivityLabel,
  runtimeWindowAvailabilityAfterHttpResponse,
} from "../dist/runtime_console_state.js";
import {
  activityFacts,
  activityDescription,
} from "../dist/runtime_activity.js";
import {
  renderProjectSelectorTree,
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
      const handlers = listeners.get("click") || [];
      for (const h of handlers) h({ preventDefault: () => {}, target: el });
    },
    querySelector(selector) {
      const normalized = selector.trim();
      if (normalized.startsWith(".")) {
        const classes = normalized.split(".").filter(Boolean);
        for (const child of this.children) {
          if (classes.every((c) => child.classList.contains(c))) return child;
          const found = child.querySelector?.(selector);
          if (found) return found;
        }
      } else {
        const tag = normalized.toUpperCase();
        for (const child of this.children) {
          if (child.tagName === tag) return child;
          const found = child.querySelector?.(selector);
          if (found) return found;
        }
      }
      return null;
    },
    querySelectorAll(selector) {
      const normalized = selector.trim();
      const results = [];
      if (normalized.startsWith(".")) {
        const classes = normalized.split(".").filter(Boolean);
        for (const child of this.children) {
          if (classes.every((c) => child.classList.contains(c))) results.push(child);
          if (child.querySelectorAll) results.push(...child.querySelectorAll(selector));
        }
      } else {
        const tag = normalized.toUpperCase();
        for (const child of this.children) {
          if (child.tagName === tag) results.push(child);
          if (child.querySelectorAll) results.push(...child.querySelectorAll(selector));
        }
      }
      return results;
    },
  };
  return el;
}

function withMockDom(run) {
  const previousDocument = globalThis.document;
  try {
    globalThis.document = {
      createElement(tag) {
        return createMockElement(tag);
      },
      createElementNS(_ns, tag) {
        return createMockElement(tag);
      },
    };
    run();
  } finally {
    globalThis.document = previousDocument;
  }
}

test("formatWindowEmptyState correctly covers all empty-state semantics across 4 quadrants", () => {
  // 1. Permission unavailable (403)
  assert.equal(
    formatWindowEmptyState("unavailable", "principal", false, "en"),
    "Window activity requires runtime:read.",
  );
  assert.equal(
    formatWindowEmptyState("unavailable", "principal", false, "zh-CN"),
    "查看窗口活动需要 runtime:read 权限。",
  );

  // 2. Stale / refresh failed (empty state does not pretend to show previous data)
  assert.equal(
    formatWindowEmptyState("stale", "principal", false, "en"),
    "Window activity could not be refreshed.",
  );
  assert.equal(
    formatWindowEmptyState("stale", "principal", false, "zh-CN"),
    "窗口活动无法刷新。",
  );

  // 3a. Available + Project scoped + Principal scope empty
  assert.equal(
    formatWindowEmptyState("available", "principal", true, "en"),
    "No Window activity is visible for this Project to this credential.",
  );
  assert.equal(
    formatWindowEmptyState("available", "principal", true, "zh-CN"),
    "当前凭证范围内，此项目没有可见的窗口活动。",
  );

  // 3b. Available + Project scoped + Global scope empty
  assert.equal(
    formatWindowEmptyState("available", "global", true, "en"),
    "No Window activity has been observed for this Project.",
  );
  assert.equal(
    formatWindowEmptyState("available", "global", true, "zh-CN"),
    "此项目尚未观察到窗口活动。",
  );

  // 4. Available + Principal scope empty (global view)
  assert.equal(
    formatWindowEmptyState("available", "principal", false, "en"),
    "No Window activity is visible to this credential.",
  );
  assert.equal(
    formatWindowEmptyState("available", "principal", false, "zh-CN"),
    "当前凭证范围内没有可见的窗口活动。",
  );

  // 5. Available + Global scope empty (admin/bootstrap view)
  assert.equal(
    formatWindowEmptyState("available", "global", false, "en"),
    "No Window activity is visible.",
  );
  assert.equal(
    formatWindowEmptyState("available", "global", false, "zh-CN"),
    "当前没有可见的窗口活动。",
  );

  // 6. Loading / Idle state
  assert.equal(
    formatWindowEmptyState("loading", "principal", false, "en"),
    "Loading Window activity…",
  );
  assert.equal(
    formatWindowEmptyState("loading", "principal", false, "zh-CN"),
    "正在加载窗口活动…",
  );
});

test("formatWindowListStatusText covers stale with/without rows, available, and permission failure", () => {
  // Stale with rows
  assert.equal(
    formatWindowListStatusText("stale", 14, "principal", "en"),
    "14 Windows · refresh failed, showing previous data",
  );
  assert.equal(
    formatWindowListStatusText("stale", 14, "principal", "zh-CN"),
    "14 个窗口 · 刷新失败，正在显示之前的数据",
  );
  // Stale without rows
  assert.equal(
    formatWindowListStatusText("stale", 0, "principal", "en"),
    "Window activity could not be refreshed.",
  );
  assert.equal(
    formatWindowListStatusText("stale", 0, "principal", "zh-CN"),
    "窗口活动无法刷新。",
  );
  // Available with rows
  assert.equal(
    formatWindowListStatusText("available", 14, "principal", "en"),
    "14 Windows",
  );
  assert.equal(
    formatWindowListStatusText("available", 14, "principal", "zh-CN"),
    "14 个窗口",
  );
  // Available without rows (principal)
  assert.equal(
    formatWindowListStatusText("available", 0, "principal", "en"),
    "No Window activity is visible to this credential.",
  );
  assert.equal(
    formatWindowListStatusText("available", 0, "principal", "zh-CN"),
    "当前凭证范围内没有可见的窗口活动。",
  );
  // Available without rows (global)
  assert.equal(
    formatWindowListStatusText("available", 0, "global", "en"),
    "No Window activity is visible.",
  );
  assert.equal(
    formatWindowListStatusText("available", 0, "global", "zh-CN"),
    "当前没有可见的窗口活动。",
  );
  // Unavailable (403)
  assert.equal(
    formatWindowListStatusText("unavailable", 0, "principal", "en"),
    "runtime:read required",
  );
  assert.equal(
    formatWindowListStatusText("unavailable", 0, "principal", "zh-CN"),
    "需要 runtime:read 权限",
  );
  // Loading
  assert.equal(
    formatWindowListStatusText("loading", 0, "principal", "en"),
    "Loading Window activity…",
  );
  assert.equal(
    formatWindowListStatusText("loading", 0, "principal", "zh-CN"),
    "正在加载窗口活动…",
  );
});

test("runtimeWindowAvailabilityAfterHttpResponse covers successful, stale, and permission outcomes", () => {
  assert.equal(runtimeWindowAvailabilityAfterHttpResponse(200, true, true), "available");
  assert.equal(runtimeWindowAvailabilityAfterHttpResponse(200, true, false), "stale");
  assert.equal(runtimeWindowAvailabilityAfterHttpResponse(0, false, false), "stale");
  assert.equal(runtimeWindowAvailabilityAfterHttpResponse(500, false, true), "stale");
  assert.equal(runtimeWindowAvailabilityAfterHttpResponse(403, false, true), "unavailable");
});

test("runtimeWindowActivityLabel formats relative time in English and Chinese", () => {
  const now = 1_000_000_000;
  // < 1s
  assert.equal(runtimeWindowActivityLabel(now - 500, now, "en"), "just now");
  assert.equal(runtimeWindowActivityLabel(now - 500, now, "zh-CN"), "刚刚");
  // seconds (< 60s)
  assert.equal(runtimeWindowActivityLabel(now - 15_000, now, "en"), "15s ago");
  assert.equal(runtimeWindowActivityLabel(now - 15_000, now, "zh-CN"), "15 秒前");
  // minutes (< 60m)
  assert.equal(runtimeWindowActivityLabel(now - 180_000, now, "en"), "3m ago");
  assert.equal(runtimeWindowActivityLabel(now - 180_000, now, "zh-CN"), "3 分钟前");
  // hours (< 24h)
  assert.equal(runtimeWindowActivityLabel(now - 7_200_000, now, "en"), "2h ago");
  assert.equal(runtimeWindowActivityLabel(now - 7_200_000, now, "zh-CN"), "2 小时前");
  // days (>= 24h)
  assert.equal(runtimeWindowActivityLabel(now - 259_200_000, now, "en"), "3d ago");
  assert.equal(runtimeWindowActivityLabel(now - 259_200_000, now, "zh-CN"), "3 天前");
  // unavailable / non-positive
  assert.equal(runtimeWindowActivityLabel(null, now, "en"), "No WebPi activity");
  assert.equal(runtimeWindowActivityLabel(null, now, "zh-CN"), "无 WebPi 活动");
  assert.equal(runtimeWindowActivityLabel(0, now, "en"), "No WebPi activity");
  assert.equal(runtimeWindowActivityLabel(0, now, "zh-CN"), "无 WebPi 活动");
});

test("renderSessionWindowCorrelationLinks supports Chinese localization and relative time", () => {
  withMockDom(() => {
    const container = createMockElement("div");
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
      "zh-CN",
    );
    assert.equal(container.children.length, 1);
    const card = container.children[0];
    assert.ok(card.className.includes("recorder-gap"));
    assert.match(
      card.querySelector(".muted")?.textContent || "",
      /cli · 最后活动 刚刚 · 2 个记录断层/,
    );
  });
});

test("activityFacts labels exit code as 'process exit N' so state=failed + exit_code=0 is unambiguous", () => {
  const failedActivity = {
    tool: "bash",
    state: "failed",
    exit_code: 0,
    duration_ms: 125,
  };
  const factsEn = activityFacts(failedActivity, true, "en");
  assert.ok(
    factsEn.includes("process exit 0"),
    "must say 'process exit 0', not 'exit 0', to avoid implying action success",
  );
  assert.ok(!factsEn.includes("exit 0"));
  assert.ok(factsEn.includes("125 ms"));

  const factsZh = activityFacts(failedActivity, true, "zh-CN");
  assert.ok(
    factsZh.includes("进程退出 0"),
    "must say '进程退出 0' in Chinese",
  );
});

test("formatWindowDetailFields translates fallbacks and bounded indicators", () => {
  const detail = {
    client_window_key: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    source: "chatgpt",
    active_count: 0,
    sessions_returned: 3,
    sessions_truncated: true,
    activity_returned: 50,
    activity_truncated: true,
  };

  const enFields = formatWindowDetailFields(detail, "", 1000, "en");
  assert.equal(enFields.title, "Window 01234567…cdef");
  assert.equal(enFields.activeStatus, "No active request");
  assert.equal(enFields.lastCall, "No completed tools/call activity");
  assert.equal(enFields.lastMeaningful, "No meaningful WebPi work recorded");
  assert.equal(enFields.linkedStatus, "3 Sessions · bounded");
  assert.equal(enFields.activityStatus, "50 events · bounded");

  const zhFields = formatWindowDetailFields(detail, "", 1000, "zh-CN");
  assert.equal(zhFields.activeStatus, "无活跃请求");
  assert.equal(zhFields.lastCall, "没有已完成的 tools/call 活动");
  assert.equal(zhFields.lastMeaningful, "未记录到有效 WebPi 工作");
  assert.equal(zhFields.linkedStatus, "3 个会话 · 有界");
  assert.equal(zhFields.activityStatus, "50 个事件 · 有界");
});

test("renderWindowActivityRows renders chips and recorder gap notes with localization", () => {
  withMockDom(() => {
    const container = document.createElement("div");
    let copiedTrace = "";
    const activities = [
      {
        tool_name: "test_runner",
        status: "success",
        started_at_ms: 1700000000000,
        meaningful: true,
        service_ms: 45,
        next_call_gap_ms: 120,
        cycle_ms: 165,
        recorder_gap_session_id: "sess-gap-123",
        server_trace_id: "trace-xyz-789",
      },
    ];

    // English rendering
    renderWindowActivityRows(container, activities, {
      compact: false,
      language: "en",
      onCopyTrace: (t) => {
        copiedTrace = t;
      },
    });

    assert.equal(container.children.length, 1);
    const item = container.children[0];
    assert.ok(item.classList.contains("recorder-gap"));
    const chips = item.querySelectorAll(".chip");
    const chipTexts = chips.map((c) => c.textContent);
    assert.ok(chipTexts.includes("meaningful"));
    assert.ok(chipTexts.includes("recorder gap"));
    assert.ok(chipTexts.includes("service 45 ms"));
    assert.ok(chipTexts.includes("next gap 120 ms"));
    assert.ok(chipTexts.includes("cycle 165 ms"));

    const note = item.querySelector(".window-gap-note");
    assert.ok(note?.textContent?.includes("Recording was not continued for sess-gap-123."));

    const traceBtn = item.querySelector(".window-trace-copy");
    assert.ok(traceBtn);
    traceBtn.click();
    assert.equal(copiedTrace, "trace-xyz-789");

    // Chinese rendering
    const containerZh = document.createElement("div");
    renderWindowActivityRows(containerZh, activities, {
      compact: false,
      language: "zh-CN",
    });
    const itemZh = containerZh.children[0];
    const chipsZh = itemZh.querySelectorAll(".chip");
    const chipTextsZh = chipsZh.map((c) => c.textContent);
    assert.ok(chipTextsZh.includes("有效工作"));
    assert.ok(chipTextsZh.includes("记录断层"));
    assert.ok(chipTextsZh.includes("服务耗时 45 ms"));
    assert.ok(chipTextsZh.includes("下次间隔 120 ms"));
    assert.ok(chipTextsZh.includes("周期 165 ms"));

    const noteZh = itemZh.querySelector(".window-gap-note");
    assert.ok(noteZh?.textContent?.includes("记录未继续于会话 sess-gap-123."));
  });
});

test("renderWindowActiveRequests renders active request items or localized empty placeholder", () => {
  withMockDom(() => {
    const emptyContainer = document.createElement("div");
    renderWindowActiveRequests(emptyContainer, [], { language: "zh-CN" });
    assert.equal(emptyContainer.children.length, 1);
    assert.equal(emptyContainer.children[0].textContent, "当前没有活跃的 WebPi 请求。");

    const populatedContainer = document.createElement("div");
    let copied = "";
    renderWindowActiveRequests(
      populatedContainer,
      [
        {
          tool_name: "edit_file",
          project: "my-project",
          started_at_ms: 1000,
          elapsed_ms: 42,
          server_trace_id: "trace-act-1",
        },
      ],
      {
        now: 1500,
        language: "en",
        onCopyTrace: (t) => {
          copied = t;
        },
      },
    );
    assert.equal(populatedContainer.children.length, 1);
    const req = populatedContainer.children[0];
    assert.equal(req.querySelector("strong")?.textContent, "edit_file");
    assert.match(req.textContent, /my-project/);
    assert.match(req.textContent, /42 ms elapsed/);
    const trace = req.querySelector(".window-trace-copy");
    trace?.click();
    assert.equal(copied, "trace-act-1");
  });
});

test("renderProjectSelectorTree displays window count in meta when project is selected", () => {
  withMockDom(() => {
    const deviceSelect = createMockElement("select");
    const projectList = createMockElement("div");
    const sessionsPanel = createMockElement("section");
    const windowPanel = createMockElement("section");

    const effectiveProjects = [
      {
        id: "proj-obs",
        client_id: "runner-main",
        sessions: {
          returned_sessions: 5,
          retained_sessions: 5,
          running_sessions: 1,
          attention_count: 0,
          latest_updated_at: 1000,
        },
      },
    ];

    renderProjectSelectorTree(deviceSelect, projectList, sessionsPanel, {
      effectiveProjects,
      devices: ["runner-main"],
      runnerRows: [{ client_id: "runner-main", status: "online" }],
      selectedDevice: "runner-main",
      selectedProject: "proj-obs",
      projectDeviceFilter: "",
      language: "en",
      storedDeviceDisclosure: () => true,
      onPersistDeviceDisclosure: () => {},
      onSelectProject: () => {},
      windowPanel,
      selectedProjectWindowActiveCount: 0,
      selectedProjectWindowCount: 4,
    });

    const activeRow = projectList.querySelector(".project-row.selected");
    assert.ok(activeRow, "must find selected project row");
    const meta = activeRow.querySelector(".project-row-meta");
    assert.ok(meta, "must find meta line");
    assert.match(meta.textContent, /5 Sessions/);
    assert.match(meta.textContent, /4 Windows/);

    // Verify DOM order: sessionsPanel MUST precede windowPanel
    const workspace = projectList.querySelector(".workspace-group");
    assert.ok(workspace);
    const sessionsIdx = workspace.children.indexOf(sessionsPanel);
    const windowIdx = workspace.children.indexOf(windowPanel);
    assert.ok(sessionsIdx >= 0 && windowIdx >= 0);
    assert.ok(sessionsIdx < windowIdx, "Sessions must precede Window Activity in workspace DOM");
  });
});
