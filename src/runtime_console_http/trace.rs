//! Explicit on-demand operator diagnostics. No periodic capture/query loop and
//! no Session recording; the MCP reader uses exactly the same runtime method.
use super::*;

#[handler]
pub(super) async fn read(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> ConsoleResult {
    let (runtime, auth) = prepared(req, depot).await?;
    if !auth.has_scope(crate::auth::SCOPE_ADMIN) {
        return Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Administrator diagnostic access required",
        });
    }
    let arguments = parse_console_json(req).await?;
    let call = ToolCall::from_tool_name("read_tool_trace", arguments)
        .map_err(|_| RuntimeConsoleError::Invalid)?;
    let result = runtime.read_tool_trace_diagnostic(call, Some(&auth)).await;
    if !result.success {
        let code = result
            .output
            .get("error_kind")
            .and_then(Value::as_str)
            .unwrap_or("");
        res.status_code(match code {
            "insufficient_scope" => StatusCode::FORBIDDEN,
            "invalid_trace_request" | "invalid_trace_ref" | "invalid_payload_index" => {
                StatusCode::BAD_REQUEST
            }
            _ => StatusCode::SERVICE_UNAVAILABLE,
        });
    }
    Ok(Json(result.output))
}
