use serde_json::{json, Value};

use crate::{
    discover_binary, read_optional_token, resolve_doctor_general_token, run_local_agent_doctor,
    run_quic_doctor_checks, DoctorOptions,
};

use super::{
    doctor_revision_check, http_get_json_status, http_post_json_status, local_cli_build_metadata,
    runtime_build_metadata, DoctorCheck,
};

pub(crate) async fn run_doctor(opts: DoctorOptions) -> Result<(String, bool), String> {
    let mut checks = Vec::new();
    for name in ["webcodex", "webcodex-agent", "webcodex-cli"] {
        match discover_binary(name) {
            Some(path) => checks.push(DoctorCheck::pass(
                format!("binary {}", name),
                path.display().to_string(),
            )),
            None => checks.push(DoctorCheck::warn(
                format!("binary {}", name),
                "not found in PATH",
            )),
        }
    }

    // Local agent-config doctor (shell profiles / projects). Runs without
    // contacting the server; never prints init_script bodies or env values.
    if let Some(agent_config) = opts.agent_config.as_deref() {
        checks.extend(run_local_agent_doctor(agent_config));
    } else {
        checks.push(DoctorCheck::warn(
            "agent config",
            "--agent-config not provided; skipped local shell-profile/project checks",
        ));
    }

    let general_token = resolve_doctor_general_token(&opts)?;
    let user_token = read_optional_token(&opts.user_token_file, "--user-token-file")?;
    let agent_token = read_optional_token(&opts.agent_token_file, "--agent-token-file")?;
    let preferred_token = user_token.as_deref().or(general_token.as_deref());
    let local_build = local_cli_build_metadata();
    if opts.server_url.is_none() || preferred_token.is_none() {
        checks.push(doctor_revision_check(&local_build, None));
    }

    if let Some(server_url) = opts.server_url.as_deref() {
        match http_post_json_status(
            server_url,
            "/api/runtime/status",
            preferred_token,
            json!({}),
        )
        .await
        {
            Ok((status, _content_type, Some(value))) if (200..300).contains(&status) => {
                let output = value.get("output").unwrap_or(&value);
                let auth_enabled = output.get("auth_enabled").and_then(Value::as_bool);
                let public_url = output
                    .get("configured_public_url")
                    .cloned()
                    .unwrap_or(Value::Null);
                let tools = output.pointer("/tools/count").and_then(Value::as_u64);
                let online = output
                    .pointer("/agents/online_count")
                    .and_then(Value::as_u64);
                checks.push(DoctorCheck::pass(
                    "runtime status",
                    format!(
                        "auth_enabled={:?} configured_public_url={} tools.count={} agents.online_count={}",
                        auth_enabled,
                        public_url,
                        tools.map(|v| v.to_string()).unwrap_or_else(|| "unknown".to_string()),
                        online.map(|v| v.to_string()).unwrap_or_else(|| "unknown".to_string())
                    ),
                ));
                if preferred_token.is_some() {
                    let remote_build = runtime_build_metadata(Some(output));
                    checks.push(doctor_revision_check(&local_build, Some(&remote_build)));
                }
            }
            Ok((status, content_type, Some(_))) => checks.push(DoctorCheck::fail(
                "runtime status",
                format!("HTTP {} content-type {}", status, content_type),
            )),
            Ok((status, content_type, None)) => checks.push(DoctorCheck::fail(
                "runtime status",
                format!(
                    "HTTP {} non-JSON response content-type {}",
                    status, content_type
                ),
            )),
            Err(e) => checks.push(DoctorCheck::fail("runtime status", e)),
        }

        match http_get_json_status(server_url, "/openapi.json").await {
            Ok((status, _content_type, Some(value))) if (200..300).contains(&status) => {
                let paths = value["paths"].as_object();
                let op_count: usize = paths
                    .map(|p| {
                        p.values()
                            .map(|m| m.as_object().map(|o| o.len()).unwrap_or(0))
                            .sum()
                    })
                    .unwrap_or(0);
                let forbidden = [
                    "/api/pairing/create",
                    "/api/pairing/enroll",
                    "/api/tokens/create",
                    "/api/agent-tokens/create",
                    "/api/users/create",
                ];
                let leaked: Vec<&str> = forbidden
                    .iter()
                    .copied()
                    .filter(|p| paths.is_some_and(|paths| paths.contains_key(*p)))
                    .collect();
                if leaked.is_empty() {
                    checks.push(DoctorCheck::pass(
                        "openapi",
                        format!(
                            "reachable; operation_count={}; management/enrollment absent",
                            op_count
                        ),
                    ));
                } else {
                    checks.push(DoctorCheck::fail(
                        "openapi",
                        format!("management/enrollment paths exposed: {}", leaked.join(", ")),
                    ));
                }
            }
            Ok((status, content_type, None)) => checks.push(DoctorCheck::fail(
                "openapi",
                format!(
                    "HTTP {} non-JSON response content-type {}",
                    status, content_type
                ),
            )),
            Ok((status, content_type, Some(_))) => checks.push(DoctorCheck::fail(
                "openapi",
                format!("HTTP {} content-type {}", status, content_type),
            )),
            Err(e) => checks.push(DoctorCheck::fail("openapi", e)),
        }

        if let Some(token) = preferred_token {
            match http_post_json_status(
                server_url,
                "/api/tools/call",
                Some(token),
                json!({"tool":"list_agents","params":{}}),
            )
            .await
            {
                Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                    let count = value
                        .pointer("/output/agents")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    checks.push(DoctorCheck::pass(
                        "agent visibility",
                        format!("agents.count={}", count),
                    ));
                }
                Ok((status, content_type, _)) => checks.push(DoctorCheck::warn(
                    "agent visibility",
                    format!("HTTP {} content-type {}", status, content_type),
                )),
                Err(e) => checks.push(DoctorCheck::warn("agent visibility", e)),
            }
            match http_post_json_status(server_url, "/api/projects/list", Some(token), json!({}))
                .await
            {
                Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                    let count = value
                        .pointer("/output/projects")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    checks.push(DoctorCheck::pass(
                        "projects",
                        format!("projects.count={}", count),
                    ));
                }
                Ok((status, content_type, _)) => checks.push(DoctorCheck::warn(
                    "projects",
                    format!("HTTP {} content-type {}", status, content_type),
                )),
                Err(e) => checks.push(DoctorCheck::warn("projects", e)),
            }

            // Basic remote shell roundtrip: run `printf webcodex-doctor-ok`
            // through the run_shell tool on the requested project and verify
            // the marker comes back. Requires --project. Non-strict: a failure
            // is a WARN (the project/agent may be offline). Never prints
            // command output beyond the marker check.
            if let Some(project) = opts.project.as_deref() {
                match http_post_json_status(
                    server_url,
                    "/api/tools/call",
                    Some(token),
                    json!({"tool":"run_shell","params":{"project":project,"command":"printf webcodex-doctor-ok"}}),
                )
                .await
                {
                    Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                        let stdout = value
                            .pointer("/output/stdout")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let exit_code =
                            value.pointer("/output/exit_code").and_then(Value::as_i64);
                        if stdout.contains("webcodex-doctor-ok") && exit_code == Some(0) {
                            checks.push(DoctorCheck::pass(
                                "shell roundtrip",
                                format!("project '{}' roundtrip ok", project),
                            ));
                        } else {
                            checks.push(DoctorCheck::warn(
                                "shell roundtrip",
                                format!(
                                    "project '{}' returned exit_code={:?} without the expected marker",
                                    project, exit_code
                                ),
                            ));
                        }
                    }
                    Ok((status, content_type, _)) => checks.push(DoctorCheck::warn(
                        "shell roundtrip",
                        format!("HTTP {} content-type {}", status, content_type),
                    )),
                    Err(e) => checks.push(DoctorCheck::warn("shell roundtrip", e)),
                }
            } else {
                checks.push(DoctorCheck::warn(
                    "shell roundtrip",
                    "--project not provided; skipped remote shell roundtrip",
                ));
            }
        } else {
            checks.push(DoctorCheck::warn(
                "tokened checks",
                "no user/bootstrap token provided; skipped agents/projects",
            ));
        }

        if let Some(token) = agent_token.as_deref() {
            match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({}))
                .await
            {
                Ok((status, _, _)) if status == 401 || status == 403 => {
                    checks.push(DoctorCheck::pass(
                        "agent token boundary",
                        "agent token cannot call runtime status",
                    ))
                }
                Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
                    "agent token boundary",
                    format!("unexpected HTTP {} content-type {}", status, content_type),
                )),
                Err(e) => checks.push(DoctorCheck::warn("agent token boundary", e)),
            }
        }
        if let Some(token) = user_token.as_deref() {
            match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({}))
                .await
            {
                Ok((status, _, _)) if (200..300).contains(&status) => checks.push(
                    DoctorCheck::pass("user token boundary", "user token can call runtime status"),
                ),
                Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
                    "user token boundary",
                    format!("HTTP {} content-type {}", status, content_type),
                )),
                Err(e) => checks.push(DoctorCheck::warn("user token boundary", e)),
            }
        }
    } else {
        checks.push(DoctorCheck::warn(
            "server checks",
            "--server-url not provided; skipped HTTP/OpenAPI checks",
        ));
    }

    if opts.quic {
        checks.extend(run_quic_doctor_checks(&opts, preferred_token).await);
    }

    let has_fail = checks.iter().any(|c| c.status == "FAIL");
    if opts.json {
        let summary = json!({
            "ok": !has_fail,
            "strict": opts.strict,
            "checks": checks.iter().map(|c| {
                json!({"name": c.name, "status": c.status, "detail": c.detail})
            }).collect::<Vec<_>>(),
        });
        Ok((
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?,
            has_fail,
        ))
    } else {
        let mut out = String::new();
        out.push_str("WebCodex doctor:\n\n");
        for check in &checks {
            out.push_str(&format!(
                "{} {:<22} {}\n",
                check.status, check.name, check.detail
            ));
        }
        Ok((out, has_fail))
    }
}
