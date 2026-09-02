use std::path::Path;

use super::profile::ResolvedKey;

pub(super) fn render_connect_output(
    server_url: &str,
    profile: &str,
    client_id: &str,
    runtime_project_id: &str,
    config_path: &Path,
    log_path: &Path,
    resolved_key: &ResolvedKey,
) -> String {
    let credential = if resolved_key.generated {
        format!("{} (shown once)", resolved_key.value)
    } else if resolved_key.recovered_profile.is_some() {
        format!(
            "saved shared key in {} (top-level token; not reprinted)",
            config_path.display()
        )
    } else {
        "the shared key supplied to this command (not reprinted)".to_string()
    };
    let mut output = String::new();
    output.push_str("WebCodex connected\n\nWhat to do next\n");
    output.push_str("1. In ChatGPT Developer Mode, create a custom MCP app.\n");
    output.push_str(&format!("2. MCP URL: {}/mcp\n", server_url));
    output.push_str("3. Authentication: Bearer token\n");
    output.push_str(&format!("4. Credential: {credential}\n"));
    output.push_str("5. Scan Tools.\n");
    output.push_str(
        "6. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n",
    );
    if resolved_key.warn_short {
        output.push_str(
            "\nWarning: the supplied shared key is short; use a long random value when possible.\n",
        );
    }
    if resolved_key.generated {
        output.push_str(
            "\nCopy this credential now. It will not be printed in full by status commands.\n",
        );
    }
    output.push_str("\nDetails\n");
    output.push_str(&format!("Server:          {server_url}\n"));
    output.push_str("Runner:          running\n");
    output.push_str(&format!("Profile:         {profile}\n"));
    output.push_str(&format!("Client:          {client_id}\n"));
    output.push_str(&format!("Runtime project: {runtime_project_id}\n"));
    output.push_str(&format!("Config:          {}\n", config_path.display()));
    output.push_str(&format!("Logs:            {}\n", log_path.display()));
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(value: &str, generated: bool, recovered_profile: Option<&str>) -> ResolvedKey {
        ResolvedKey {
            value: value.to_string(),
            generated,
            recovered_profile: recovered_profile.map(str::to_string),
            warn_short: false,
        }
    }

    #[test]
    fn generated_key_output_prioritizes_chatgpt_setup_and_discloses_once() {
        let secret = "wck_generated_once_for_test";
        let output = render_connect_output(
            "https://webcodex.example",
            "profile",
            "runner",
            "agent:runner:project",
            Path::new("runner.toml"),
            Path::new("runner.log"),
            &key(secret, true, None),
        );
        assert!(output.starts_with("WebCodex connected\n\nWhat to do next"));
        assert_eq!(output.matches("https://webcodex.example/mcp").count(), 1);
        assert_eq!(output.matches(secret).count(), 1);
        assert!(output.contains("Authentication: Bearer token"));
        assert!(output.contains("Scan Tools"));
        assert!(output.contains("First prompt:"));
        assert!(output.find("What to do next").unwrap() < output.find("Details").unwrap());
    }

    #[test]
    fn existing_key_output_never_reprints_secret() {
        let secret = "wck_existing_never_print";
        let output = render_connect_output(
            "https://webcodex.example",
            "profile",
            "runner",
            "agent:runner:project",
            Path::new("runner.toml"),
            Path::new("runner.log"),
            &key(secret, false, None),
        );
        assert!(!output.contains(secret));
        assert!(output.contains("shared key supplied to this command"));
        assert!(!output.contains("Copy this credential now"));
    }

    #[test]
    fn recovered_profile_output_points_to_credential_source_without_reprinting_it() {
        let secret = "wck_recovered_never_print";
        let output = render_connect_output(
            "https://webcodex.example",
            "profile",
            "runner",
            "agent:runner:project",
            Path::new("/protected/profile/agent.toml"),
            Path::new("runner.log"),
            &key(secret, false, Some("profile")),
        );
        assert!(!output.contains(secret));
        assert!(output.contains("saved shared key in /protected/profile/agent.toml"));
        assert!(output.contains("top-level token; not reprinted"));
    }
}
