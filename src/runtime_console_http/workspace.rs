//! Product views over existing authorized instruction, Skill, Plugin and Git owners.
//! No second registry, credential model, Session or execution policy lives here.
use super::*;
use serde_json::json;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectInput {
    project: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstructionInput {
    project: String,
    source_scope: String,
    path: String,
    fingerprint: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginReloadInput {
    project: String,
    plugin: String,
}

async fn product_call(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    name: &'static str,
    arguments: Value,
) -> Result<Value, RuntimeConsoleError> {
    let call =
        ToolCall::from_tool_name(name, arguments).map_err(|_| RuntimeConsoleError::Invalid)?;
    let result = runtime.dispatch_with_auth(call, Some(auth)).await;
    if result.success {
        Ok(result.output)
    } else {
        Err(RuntimeConsoleError::Internal)
    }
}

fn catalog(value: Result<Value, RuntimeConsoleError>) -> Value {
    match value {
        Ok(value) => json!({"available": true, "catalog": value}),
        Err(_) => json!({"available": false}),
    }
}

#[handler]
pub(super) async fn extensions(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<ProjectInput>().await {
        Ok(value) => value,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    if let Err(error) = authorize_exact_project(&runtime, &auth, &input.project).await {
        return render_error(res, error);
    }
    let project = match runtime
        .resolve_project_input_for_auth(&input.project, Some(&auth))
        .await
    {
        Ok(project) => project,
        Err(_) => return render_error(res, RuntimeConsoleError::NotFound),
    };
    let instructions = runtime.load_effective_coding_instructions(&project, Some(&auth));
    let skills = product_call(
        &runtime,
        &auth,
        "list_skills",
        json!({"project":input.project,"limit":64}),
    );
    let plugins = product_call(
        &runtime,
        &auth,
        "plugin_tool",
        json!({"action":"list","runner":project.config.client_id}),
    );
    let (instructions, skills, plugins) = tokio::join!(instructions, skills, plugins);
    // File bodies are opened explicitly in a separate, fingerprint-fenced read.
    let files: Vec<Value> = instructions
        .files
        .iter()
        .map(|file| {
            json!({
                "source_scope":file.source_scope, "path":file.path, "fingerprint":file.fingerprint,
                "truncated":file.truncated, "total_lines":file.total_lines,
            })
        })
        .collect();
    res.render(Json(json!({
        "project":project.resolved_id,
        "runner":project.config.client_id,
        "instructions":{"files":files,"scan_complete":instructions.scan_complete,"truncated":instructions.truncated},
        "skills":catalog(skills), "plugins":catalog(plugins),
        "can_reload_plugins":auth.has_scope(webcodex_core::authority::SCOPE_PLUGIN_MANAGE),
    })));
}

#[handler]
pub(super) async fn instruction(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<InstructionInput>().await {
        Ok(value) => value,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    if input.path.len() > 4096
        || input.fingerprint.len() > 128
        || !matches!(input.source_scope.as_str(), "runner" | "project")
    {
        return render_error(res, RuntimeConsoleError::Invalid);
    }
    if let Err(error) = authorize_exact_project(&runtime, &auth, &input.project).await {
        return render_error(res, error);
    }
    let project = match runtime
        .resolve_project_input_for_auth(&input.project, Some(&auth))
        .await
    {
        Ok(project) => project,
        Err(_) => return render_error(res, RuntimeConsoleError::NotFound),
    };
    let snapshot = runtime
        .load_effective_coding_instructions(&project, Some(&auth))
        .await;
    let file = snapshot.files.iter().find(|file| {
        file.path == input.path
            && serde_json::to_value(file.source_scope)
                .ok()
                .and_then(|s| s.as_str().map(str::to_owned))
                .as_deref()
                == Some(input.source_scope.as_str())
    });
    match file {
        Some(file) if file.fingerprint == input.fingerprint => res.render(Json(
            json!({"content":file.content,"truncated":file.truncated,"path":file.path}),
        )),
        Some(_) => render_error(res, RuntimeConsoleError::Conflict),
        None => render_error(res, RuntimeConsoleError::NotFound),
    }
}

#[handler]
pub(super) async fn project_git(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<ProjectInput>().await {
        Ok(value) => value,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    if let Err(error) = authorize_exact_project(&runtime, &auth, &input.project).await {
        return render_error(res, error);
    }
    match product_call(&runtime, &auth, "read_workspace_changes", json!({"project":input.project,"include_diff":false})).await {
        Ok(value) => res.render(Json(json!({
            "branch":value["branch"], "clean":value["clean"], "git_available":value["git_available"],
            "non_git_project":value["non_git_project"], "files":value["files"],
            "files_total":value["files_total"], "files_truncated":value["files_truncated"],
        }))),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn plugin_reload(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<PluginReloadInput>().await {
        Ok(value) => value,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    if let Err(error) = authorize_exact_project(&runtime, &auth, &input.project).await {
        return render_error(res, error);
    }
    let project = match runtime
        .resolve_project_input_for_auth(&input.project, Some(&auth))
        .await
    {
        Ok(project) => project,
        Err(_) => return render_error(res, RuntimeConsoleError::NotFound),
    };
    // The canonical Plugin operation independently enforces manage scope and effect gates.
    match product_call(
        &runtime,
        &auth,
        "plugin_tool",
        json!({"action":"reload","runner":project.config.client_id,"plugin":input.plugin}),
    )
    .await
    {
        Ok(value) => res.render(Json(value)),
        Err(error) => render_error(res, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn product_inputs_cannot_choose_tools_runners_or_paths_to_read() {
        assert!(
            serde_json::from_value::<ProjectInput>(json!({"project":"p","tool":"run_shell"}))
                .is_err()
        );
        assert!(serde_json::from_value::<PluginReloadInput>(
            json!({"project":"p","plugin":"x","runner":"other"})
        )
        .is_err());
        assert!(serde_json::from_value::<InstructionInput>(
            json!({"project":"p","path":"AGENTS.md"})
        )
        .is_err());
    }
    #[test]
    fn unavailable_catalog_is_not_an_empty_success() {
        assert_eq!(
            catalog(Err(RuntimeConsoleError::Internal)),
            json!({"available":false})
        );
        assert_eq!(catalog(Ok(json!({"skills":[]})))["available"], true);
    }
}
