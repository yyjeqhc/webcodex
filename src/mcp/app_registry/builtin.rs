//! One inventory owns template identity, discovery, binding and optional metadata.
//! v0.5 serves exact current URIs only; retired identities are not read aliases.
//! Advance an App URI when shipping changed HTML or an incompatible App contract.

use super::{AppListing, BundledMcpApp};

pub(in crate::mcp) const MCP_COMPUTER_UI_RESOURCE_URI: &str = "ui://webcodex/computer/v12";
// Existing gray-card diagnostic: do not introduce a cache reuse policy change.
pub(in crate::mcp) const MCP_COMPUTER_UI_RESOURCE_TTL_MS: u64 = 0;
pub(in crate::mcp) const MCP_RESULT_UI_RESOURCE_URI: &str = "ui://webcodex/changes/v4";
pub(in crate::mcp) const MCP_WORKBENCH_UI_RESOURCE_URI: &str = "ui://webcodex/workbench/v2";
pub(in crate::mcp) const MCP_WORK_RESULT_UI_RESOURCE_URI: &str = "ui://webcodex/work-result/v28";
pub(in crate::mcp) const MCP_GOAL_PLAN_UI_RESOURCE_URI: &str = "ui://webcodex/goal-plan/v7";
pub(in crate::mcp) const MCP_AGENT_CONTINUATION_UI_RESOURCE_URI: &str =
    "ui://webcodex/agent-continuation/v18";
pub(in crate::mcp) const MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI: &str =
    "ui://webcodex/job-terminal-continuation/v2";

pub(in crate::mcp) const MCP_COMPUTER_APP_HTML: &str = include_str!("../../mcp_computer_app.html");
pub(in crate::mcp) const MCP_RESULT_APP_HTML: &str = include_str!("../../mcp_result_app.html");
const MCP_WORKBENCH_APP_HTML: &str = include_str!("../../mcp_workbench_app.html");
pub(in crate::mcp) const MCP_WORK_RESULT_APP_HTML: &str =
    include_str!("../../mcp_work_result_app.html");
pub(in crate::mcp) const MCP_GOAL_PLAN_APP_HTML: &str =
    include_str!("../../mcp_goal_plan_app.html");
pub(in crate::mcp) const MCP_AGENT_CONTINUATION_APP_HTML: &str =
    include_str!("../../mcp_agent_continuation_app.html");
pub(in crate::mcp) const MCP_JOB_TERMINAL_CONTINUATION_APP_HTML: &str =
    include_str!("../../mcp_job_terminal_continuation_app.html");

pub(in crate::mcp) const MCP_PDF_UI_RESOURCE_URI: &str = "ui://webcodex/pdf/v5";

pub(super) static BUILTIN_MCP_APPS: &[BundledMcpApp] = &[
    BundledMcpApp {
        listing: Some(AppListing { name: "WebCodex PDF", description: "Dedicated read-only PDF reader for one explicitly selected project document." }),
        tools: &["present_pdf"],
        tool_title: Some("Open PDF"),
        read_display_modes: &["fullscreen", "inline"],
        ..BundledMcpApp::template(MCP_PDF_UI_RESOURCE_URI, include_str!("../../mcp_pdf_app.html"))
    },
    BundledMcpApp {
        listing: Some(AppListing {
            name: "WebCodex Computer",
            description: "Minimal read-only WebCodex Computer screenshot card that performs only the standard MCP Apps handshake and renders native images returned by observe_computer snapshot actions.",
        }),
        cache_ttl_ms: Some(MCP_COMPUTER_UI_RESOURCE_TTL_MS),
        ..BundledMcpApp::template(MCP_COMPUTER_UI_RESOURCE_URI, MCP_COMPUTER_APP_HTML)
    },
    BundledMcpApp {
        listing: Some(AppListing {
            name: "WebCodex Projects & Resources",
            description: "Readonly project selection, work overview and authorized resource references.",
        }),
        tools: &["open_webcodex_workbench"],
        tool_title: Some("Projects & Resources"),
        tool_entrypoints: &["global", "thread"],
        read_display_modes: &["inline", "fullscreen"],
        ..BundledMcpApp::template(MCP_WORKBENCH_UI_RESOURCE_URI, MCP_WORKBENCH_APP_HTML)
    },
    BundledMcpApp {
        listing: Some(AppListing {
            name: "WebCodex",
            description: "Persistent user-facing card for one client Window and Project. Present it once near the start of substantial work; the mounted App refreshes the same bounded Window ActionAudit activity used by WebUI, including observe/diagnostic actions, without creating extra cards. Workflow Session collaboration and immutable final changes are optional linked evidence that may appear later; live internal checks/review state is not the primary UI.",
        }),
        tools: &["present_work_result"],
        ..BundledMcpApp::template(MCP_WORK_RESULT_UI_RESOURCE_URI, MCP_WORK_RESULT_APP_HTML)
    },
    BundledMcpApp {
        listing: Some(AppListing {
            name: "WebCodex Goal Plan",
            description: "Sparse read-only durable Goal presentation. One explicit present_goal_plan call creates the card; the View converges by app-only exact polling of authoritative Goal state and never owns execution or lifecycle state.",
        }),
        tools: &["present_goal_plan"],
        ..BundledMcpApp::template(MCP_GOAL_PLAN_UI_RESOURCE_URI, MCP_GOAL_PLAN_APP_HTML)
    },
    BundledMcpApp {
        listing: Some(AppListing {
            name: "WebCodex Agent Continuation",
            description: "Sparse Host controller for one explicit Durable Agent Endpoint generation. The View is a process-local carrier only: SQLite Wake/Wake Delivery Attempt remains authoritative, and Host dispatch is considered actually resumed only after exact consume_agent_wake.",
        }),
        // Registry membership is not model visibility or App-tool admission.
        tools: &["present_agent_continuation", "wait_for_agent_events"],
        ..BundledMcpApp::template(MCP_AGENT_CONTINUATION_UI_RESOURCE_URI, MCP_AGENT_CONTINUATION_APP_HTML)
    },
    BundledMcpApp {
        listing: Some(AppListing {
            name: "WebCodex Job Continuation",
            description: "Job-native Host continuation carrier for one explicit caller-owned Job terminal wait. The View is routing-only: canonical Job terminal truth and the durable delivery fence remain in the existing Job terminal wait ledger, while the App performs bounded state polling and at most one prepared ui/message dispatch.",
        }),
        tools: &["present_job_terminal_continuation"],
        ..BundledMcpApp::template(MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI, MCP_JOB_TERMINAL_CONTINUATION_APP_HTML)
    },
    // Readable for already-cached descriptors, but never newly listed or bound.
    BundledMcpApp::template(MCP_RESULT_UI_RESOURCE_URI, MCP_RESULT_APP_HTML),
];
