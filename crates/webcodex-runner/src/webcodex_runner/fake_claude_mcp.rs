// Standalone newline-delimited JSON-RPC MCP fixture. Tests compile this file
// directly with rustc; it is not a production binary target.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

fn main() -> io::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let scenario = args.first().map(String::as_str).unwrap_or("normal");
    #[cfg(unix)]
    if scenario == "ignore_term" {
        unsafe {
            signal(SIGTERM, SIG_IGN);
        }
    }
    let marker = args.get(1).map(Path::new);
    if scenario == "sleep_descendant" {
        // Test-only grandchild mode: keep inherited stdout open and sleep so
        // the parent can observe that a descendant still owns the tree.
        let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
        thread::sleep(Duration::from_secs(secs));
        return Ok(());
    }
    append(marker, "start\n")?;
    let mut reader = BufReader::new(io::stdin().lock());
    let mut writer = io::stdout().lock();
    loop {
        let mut body = String::new();
        if reader.read_line(&mut body)? == 0 {
            return Ok(());
        }
        append(marker, &format!("request:{}\n", body.trim_end()))?;
        let method = string_field(&body, "method");
        let id = u64_field(&body, "id").unwrap_or(0);
        match method.as_deref() {
            Some("initialize") => send(
                &mut writer,
                &format!(
                    r#"{{"jsonrpc":"2.0","id":{id},"result":{{"protocolVersion":"2025-06-18","capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"fake","version":"Claude Fake 1.2.3"}}}}}}"#
                ),
            )?,
            Some("notifications/initialized") => {}
            Some("tools/list") => {
                if scenario == "spawn_descendant" {
                    // Spawn a descendant that inherits stdout, then exit right
                    // after responding. The descendant outlives the direct child
                    // and keeps the piped stdout write end open, so the
                    // connection reader must NOT see EOF while it lives.
                    let child = spawn_sleep_descendant()?;
                    append(marker, &format!("GRANDCHILD_PID={}\n", child.id()))?;
                    let tools = tools_list_json(scenario);
                    send(
                        &mut writer,
                        &format!(
                            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"tools":[{tools}]}}}}"#
                        ),
                    )?;
                    return Ok(());
                }
                let tools = tools_list_json(scenario);
                send(
                    &mut writer,
                    &format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{"tools":[{tools}]}}}}"#),
                )?;
            }
            Some("tools/call") => match scenario {
                "invalid_json" => send(&mut writer, "{invalid")?,
                "timeout" | "ignore_term" => thread::sleep(Duration::from_secs(5)),
                "oversized" => {
                    let text = "x".repeat(1024 * 1024 + 100);
                    send(&mut writer, &tool_result(id, &text, false))?;
                }
                "exp_soft_oversized" => {
                    // Above soft bound (256 KiB) and below hard fail (512 KiB).
                    let text = "x".repeat(300 * 1024);
                    send(&mut writer, &tool_result(id, &text, false))?;
                }
                "exp_oversized" => {
                    // Above hard fail threshold (512 KiB) and below MCP message bound.
                    let text = "x".repeat(600 * 1024);
                    send(&mut writer, &tool_result(id, &text, false))?;
                }
                "exp_mutate_exit" => {
                    // Mutating tool may write, then process exits before tools/call response.
                    let name = string_field(&body, "name").unwrap_or_default();
                    match name.as_str() {
                        "Write" => {
                            let path = string_field(&body, "file_path").unwrap_or_default();
                            let content = string_field(&body, "content").unwrap_or_default();
                            let target = resolve_path(&path);
                            let _ = fs::write(&target, content);
                            append(marker, "mutated_then_exit\n")?;
                            return Ok(());
                        }
                        "Edit" => {
                            let path = string_field(&body, "file_path").unwrap_or_default();
                            let old = string_field(&body, "old_string").unwrap_or_default();
                            let new = string_field(&body, "new_string").unwrap_or_default();
                            let target = resolve_path(&path);
                            if let Ok(before) = fs::read_to_string(&target) {
                                let _ = fs::write(&target, before.replacen(&old, &new, 1));
                            }
                            append(marker, "mutated_then_exit\n")?;
                            return Ok(());
                        }
                        _ => {
                            let (text, is_error) = dispatch_tool(&body)?;
                            send(&mut writer, &tool_result(id, &text, is_error))?;
                        }
                    }
                }
                "exit" => return Ok(()),
                "restart_once" if !marker_contains(marker, "crashed") => {
                    append(marker, "crashed\n")?;
                    return Ok(());
                }
                _ => {
                    if scenario == "delayed" {
                        thread::sleep(Duration::from_millis(250));
                    }
                    if scenario == "unknown_id" {
                        send(
                            &mut writer,
                            r#"{"jsonrpc":"2.0","id":999999,"result":{"ignored":true}}"#,
                        )?;
                    }
                    if scenario == "server_request" {
                        send(
                            &mut writer,
                            r#"{"jsonrpc":"2.0","id":"server-request-1","method":"sampling/createMessage","params":{}}"#,
                        )?;
                        let mut response = String::new();
                        reader.read_line(&mut response)?;
                        if !response.contains(r#""id":"server-request-1""#)
                            || !response.contains(r#""code":-32601"#)
                            || !response.contains(r#""message":"Method not found""#)
                        {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "client did not reject unsupported server request",
                            ));
                        }
                        append(marker, "server_request_error_received\n")?;
                    }
                    let (text, is_error) = dispatch_tool(&body)?;
                    send(&mut writer, &tool_result(id, &text, is_error))?;
                }
            },
            _ => {}
        }
    }
}

#[cfg(unix)]
const SIGTERM: i32 = 15;
#[cfg(unix)]
const SIG_IGN: usize = 1;
#[cfg(unix)]
unsafe extern "C" {
    fn signal(signal: i32, handler: usize) -> usize;
}

fn dispatch_tool(body: &str) -> io::Result<(String, bool)> {
    let name = string_field(body, "name").unwrap_or_default();
    Ok(match name.as_str() {
        "fake_search" => (
            format!("{}/src/lib.rs:2:needle", env::current_dir()?.display()),
            false,
        ),
        "fake_edit" => {
            let path = env::current_dir()?.join("edit.txt");
            let before = fs::read_to_string(&path)?;
            fs::write(path, before.replacen("before", "after", 1))?;
            ("edited".to_string(), false)
        }
        "Read" => {
            let path = string_field(body, "file_path").unwrap_or_default();
            let target = resolve_path(&path);
            match fs::read_to_string(&target) {
                Ok(content) => (content, false),
                Err(error) => (format!("ENOENT: {error}"), true),
            }
        }
        "Edit" => {
            let path = string_field(body, "file_path").unwrap_or_default();
            let old = string_field(body, "old_string").unwrap_or_default();
            let new = string_field(body, "new_string").unwrap_or_default();
            let target = resolve_path(&path);
            match fs::read_to_string(&target) {
                Ok(before) => {
                    let count = before.matches(&old).count();
                    if count == 0 {
                        ("old_string not found".to_string(), true)
                    } else if count > 1 {
                        ("old_string matched multiple times".to_string(), true)
                    } else {
                        fs::write(&target, before.replacen(&old, &new, 1))?;
                        ("ok".to_string(), false)
                    }
                }
                Err(error) => (format!("edit failed: {error}"), true),
            }
        }
        "Write" => {
            let path = string_field(body, "file_path").unwrap_or_default();
            let content = string_field(body, "content").unwrap_or_default();
            let target = resolve_path(&path);
            match fs::write(&target, content) {
                Ok(()) => ("wrote".to_string(), false),
                Err(error) => (format!("write failed: {error}"), true),
            }
        }
        "Bash" => {
            let command = string_field(body, "command").unwrap_or_default();
            if command.contains("nonzero") {
                ("exit=1\nstderr=fail".to_string(), true)
            } else {
                (format!("stdout:{command}"), false)
            }
        }
        "TaskCreate" => ("task created".to_string(), false),
        "LargeSchemaTool" => ("large".to_string(), false),
        _ => ("unknown tool".to_string(), true),
    })
}

fn send(writer: &mut impl Write, body: &str) -> io::Result<()> {
    writer.write_all(body.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// Re-invoke this fixture as a `sleep_descendant` grandchild that inherits all
/// std handles, so the piped stdout write end stays open for its lifetime.
fn spawn_sleep_descendant() -> io::Result<std::process::Child> {
    let self_exe = std::env::current_exe()?;
    let mut command = Command::new(self_exe);
    command
        .args(["sleep_descendant", "60"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command.spawn()
}

// Search fixtures plus extra discovery tools used by schema and bound tests.
const FAKE_TOOLS_BASE: &str = r#"{"name":"fake_search","description":"search","inputSchema":{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"output_mode":{"type":"string"},"head_limit":{"type":"integer"},"-n":{"type":"boolean"},"-B":{"type":"integer"},"-A":{"type":"integer"}}}},{"name":"fake_edit","description":"edit","inputSchema":{"type":"object","properties":{"file_path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"}}}},{"name":"Read","description":"read a file","inputSchema":{"type":"object","properties":{"file_path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}},"required":["file_path"]}},{"name":"Edit","description":"edit a file","inputSchema":{"type":"object","properties":{"file_path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"}},"required":["file_path","old_string","new_string"]}},{"name":"Write","description":"write a file","inputSchema":{"type":"object","properties":{"file_path":{"type":"string"},"content":{"type":"string"}},"required":["file_path","content"]}},{"name":"Bash","description":"run a shell command","inputSchema":{"type":"object","properties":{"command":{"type":"string"},"timeout":{"type":"integer"}},"required":["command"]}},{"name":"TaskCreate","description":"create a task outside the configured search mapping","inputSchema":{"type":"object","properties":{"subject":{"type":"string"},"description":{"type":"string"}},"required":["subject"]}}"#;

fn tools_list_json(scenario: &str) -> String {
    match scenario {
        "exp_many_tools" => {
            // 7 base tools + 59 padding tools = 66 total (past the 64 bound).
            let mut parts = vec![FAKE_TOOLS_BASE.to_string()];
            for index in 0..59 {
                parts.push(format!(
                    r#"{{"name":"PadTool{index}","description":"padding","inputSchema":{{"type":"object","properties":{{"n":{{"type":"integer"}}}}}}}}"#
                ));
            }
            parts.join(",")
        }
        "exp_large_schema" => {
            // Schema body deliberately exceeds the 64 KiB discovery schema bound.
            let pad = "x".repeat(70 * 1024);
            format!(
                r#"{base},{{"name":"LargeSchemaTool","description":"oversized schema tool","inputSchema":{{"type":"object","properties":{{"payload":{{"type":"string","description":"{pad}"}}}},"required":["payload"]}}}}"#,
                base = FAKE_TOOLS_BASE
            )
        }
        _ => FAKE_TOOLS_BASE.to_string(),
    }
}

fn tool_result(id: u64, text: &str, is_error: bool) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"content":[{{"type":"text","text":"{}"}}],"isError":{}}}}}"#,
        escape(text),
        if is_error { "true" } else { "false" }
    )
}

fn resolve_path(path: &str) -> std::path::PathBuf {
    let candidate = std::path::Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(candidate)
    }
}

fn u64_field(body: &str, field: &str) -> Option<u64> {
    let after = body.split_once(&format!(r#""{field}""#))?.1;
    let value = after.split_once(':')?.1.trim_start();
    value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

fn string_field(body: &str, field: &str) -> Option<String> {
    let after = body.split_once(&format!(r#""{field}""#))?.1;
    let value = after.split_once(':')?.1.trim_start().strip_prefix('"')?;
    Some(value[..value.find('"')?].to_string())
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn append(marker: Option<&Path>, text: &str) -> io::Result<()> {
    if let Some(marker) = marker {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(marker)?
            .write_all(text.as_bytes())?;
    }
    Ok(())
}

fn marker_contains(marker: Option<&Path>, needle: &str) -> bool {
    marker
        .and_then(|path| fs::read_to_string(path).ok())
        .is_some_and(|text| text.contains(needle))
}
