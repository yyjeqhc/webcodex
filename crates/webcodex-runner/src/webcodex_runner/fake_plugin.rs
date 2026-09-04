// Standalone newline-delimited WebCodex Tool Plugin fixture. Runner tests
// compile this file directly with rustc, so the native Plugin ABI has no SDK or
// external runtime dependency.

use std::env;
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

fn main() -> io::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let scenario = args.first().map(String::as_str).unwrap_or("normal");
    let marker = args.get(1).map(Path::new);
    append(marker, "start\n")?;
    if scenario == "execution_context" {
        append(
            marker,
            if env::var("WEBCODEX_PLUGIN_TEST_ENV").as_deref() == Ok("profile-ready") {
                "profile-env-ok\n"
            } else {
                "profile-env-bad\n"
            },
        )?;
        append(
            marker,
            if env::var_os("WEBCODEX_AGENT_TOKEN").is_none() {
                "sensitive-env-cleared\n"
            } else {
                "sensitive-env-leaked\n"
            },
        )?;
        let cwd_ok = args
            .get(2)
            .and_then(|expected| env::current_dir().ok().map(|current| (expected, current)))
            .is_some_and(|(expected, current)| {
                std::fs::canonicalize(expected).ok() == std::fs::canonicalize(current).ok()
            });
        append(marker, if cwd_ok { "cwd-ok\n" } else { "cwd-bad\n" })?;
    }
    if scenario == "stderr" {
        eprintln!("diagnostic-only-secret-looking-stderr");
    }

    let mut reader = BufReader::new(io::stdin().lock());
    let mut writer = io::stdout().lock();
    let mut lists = 0usize;
    let mut calls = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let method = string_field(&line, "method").unwrap_or_default();
        let id = u64_field(&line, "id").unwrap_or(0);
        match method.as_str() {
            "initialize" => {
                append(marker, "initialize\n")?;
                if scenario == "init_crash" {
                    return Ok(());
                }
                let version = if scenario == "bad_version" {
                    "unsupported-plugin-v0"
                } else {
                    "webcodex-plugin-v1"
                };
                send(
                    &mut writer,
                    &format!(
                        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"protocolVersion":"{version}"}}}}"#
                    ),
                )?;
            }
            "tools/list" => {
                lists += 1;
                append(marker, "list\n")?;
                if scenario == "malformed" {
                    send(&mut writer, "{not-json")?;
                    continue;
                }
                if scenario == "oversized_message" {
                    send(
                        &mut writer,
                        &format!(
                            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"padding":"{}","tools":[]}}}}"#,
                            "x".repeat(1024 * 1024)
                        ),
                    )?;
                    continue;
                }
                if scenario == "invalid_tools" {
                    send(
                        &mut writer,
                        &format!(
                            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"tools":[{{"name":"echo","inputSchema":[]}}]}}}}"#
                        ),
                    )?;
                    continue;
                }
                let value_type = if scenario == "schema_change" && lists >= 2 {
                    "number"
                } else {
                    "string"
                };
                send(
                    &mut writer,
                    &format!(
                        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"tools":[{{"name":"echo","description":"Native plugin echo","inputSchema":{{"type":"object","properties":{{"value":{{"type":"{value_type}"}}}}}}}}]}}}}"#
                    ),
                )?;
            }
            "tools/call" => {
                calls += 1;
                append(marker, "call\n")?;
                match scenario {
                    "crash" => return Ok(()),
                    "timeout" => thread::sleep(Duration::from_secs(3)),
                    "slow" => {
                        thread::sleep(Duration::from_millis(750));
                        send_result(&mut writer, id, calls)?;
                    }
                    "rpc_error" => send(
                        &mut writer,
                        &format!(
                            r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":-32001,"message":"fixture error"}}}}"#
                        ),
                    )?,
                    "bad_result" => send(
                        &mut writer,
                        &format!(
                            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"content":[{{"type":"image","data":"AA=="}}],"isError":false}}}}"#
                        ),
                    )?,
                    _ => send_result(&mut writer, id, calls)?,
                }
            }
            _ => {}
        }
    }
}

fn send_result(writer: &mut impl Write, id: u64, calls: usize) -> io::Result<()> {
    send(
        writer,
        &format!(
            r#"{{"jsonrpc":"2.0","id":{id},"result":{{"content":[{{"type":"text","text":"call-{calls}"}}],"structuredContent":{{"call":{calls}}},"isError":false}}}}"#
        ),
    )
}

fn send(writer: &mut impl Write, message: &str) -> io::Result<()> {
    writer.write_all(message.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn append(path: Option<&Path>, value: &str) -> io::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(value.as_bytes())
}

fn string_field(body: &str, field: &str) -> Option<String> {
    let prefix = format!(r#""{field}":"#);
    let start = body.find(&prefix)? + prefix.len();
    let value = body.get(start..)?.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn u64_field(body: &str, field: &str) -> Option<u64> {
    let prefix = format!(r#""{field}":"#);
    let start = body.find(&prefix)? + prefix.len();
    body.get(start..)?
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}
