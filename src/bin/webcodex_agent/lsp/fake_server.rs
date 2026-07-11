// Standalone Rust fake LSP server used by `tests.rs`. The test suite compiles
// this file directly with rustc so it never depends on rust-analyzer or a
// scripting runtime and never becomes a production binary target.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

fn main() {
    if let Err(error) = run() {
        eprintln!("fake LSP server failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let scenario = args.next().unwrap_or_else(|| "normal".to_string());
    let marker = args.next().map(PathBuf::from);
    let exit_marker = args.next().map(PathBuf::from);
    if let Some(marker) = &marker {
        append_marker(
            marker,
            &format!(
                "start:{}:{}\n",
                std::process::id(),
                env::current_dir()?.display()
            ),
        )?;
    }
    if scenario == "stderr_flood" {
        let mut stderr = io::stderr().lock();
        let chunk = vec![b'x'; 4096];
        for _ in 0..128 {
            stderr.write_all(&chunk)?;
        }
        stderr.flush()?;
    }

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    loop {
        let body = match read_frame(&mut reader)? {
            Some(body) => body,
            None => return Ok(()),
        };
        let method = json_string_field(&body, "method");
        let id = json_u64_field(&body, "id");
        match method.as_deref() {
            Some("initialize") => {
                if let Some(marker) = &marker {
                    append_marker(marker, &format!("initialize:{body}\n"))?;
                }
                if scenario == "initialize_exit" {
                    return Ok(());
                }
                if scenario == "initialize_hang" {
                    // Never answer initialize; keep the process alive so the
                    // client cleanup path must kill/wait under the configured
                    // shutdown budget.
                    loop {
                        thread::sleep(Duration::from_secs(60));
                    }
                }
                if scenario == "interleaved" {
                    write_frame(
                        &mut writer,
                        r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{"type":3,"message":"ready"}}"#,
                    )?;
                }
                let encoding = match scenario.as_str() {
                    "utf8" => Some("utf-8"),
                    "utf16" => Some("utf-16"),
                    "utf32" => Some("utf-32"),
                    _ => None,
                };
                let capabilities = encoding
                    .map(|encoding| format!(r#"{{"positionEncoding":"{encoding}"}}"#))
                    .unwrap_or_else(|| "{}".to_string());
                write_frame(
                    &mut writer,
                    &format!(
                        r#"{{"jsonrpc":"2.0","id":{},"result":{{"capabilities":{capabilities}}}}}"#,
                        id.unwrap_or(0)
                    ),
                )?;
                if scenario == "exit_after_initialize" {
                    return Ok(());
                }
            }
            Some("initialized") => {}
            Some("textDocument/didOpen") => {
                if let Some(marker) = &marker {
                    append_marker(
                        marker,
                        &format!("didOpen:{}:{body}\n", std::process::id()),
                    )?;
                }
                maybe_publish_diagnostics(
                    &mut writer,
                    &scenario,
                    &body,
                    marker.as_deref(),
                )?;
            }
            Some("textDocument/didChange") => {
                if let Some(marker) = &marker {
                    append_marker(
                        marker,
                        &format!("didChange:{}:{body}\n", std::process::id()),
                    )?;
                }
                maybe_publish_diagnostics(
                    &mut writer,
                    &scenario,
                    &body,
                    marker.as_deref(),
                )?;
            }
            Some("shutdown") => {
                if scenario == "shutdown_hang" {
                    // Acknowledge nothing and never exit on its own.
                    continue;
                }
                write_frame(
                    &mut writer,
                    &format!(
                        r#"{{"jsonrpc":"2.0","id":{},"result":null}}"#,
                        id.unwrap_or(0)
                    ),
                )?;
            }
            Some("exit") => {
                if scenario == "shutdown_hang" {
                    continue;
                }
                if let Some(path) = exit_marker {
                    fs::write(path, b"exited")?;
                }
                return Ok(());
            }
            Some("$/cancelRequest") => {
                if let Some(marker) = &marker {
                    append_marker(marker, &format!("cancel:{body}\n"))?;
                }
            }
            Some(method) => match scenario.as_str() {
                // Never answer business requests; keep the process alive.
                "timeout" | "timeout_cancel" | "shutdown_hang" => {}
                "malformed_json" => write_frame(&mut writer, "{not-json")?,
                "malformed_alive_then_success" => {
                    let starts = start_count(marker.as_deref());
                    if starts <= 1 {
                        // Malformed response while keeping the process alive so
                        // the client reader crashes but try_wait still sees the
                        // child as running.
                        write_frame(&mut writer, "{not-json")?;
                    } else {
                        write_result(&mut writer, id, method)?;
                    }
                }
                "malformed_alive_always" => {
                    write_frame(&mut writer, "{not-json")?;
                }
                "invalid_length" => {
                    writer.write_all(b"Content-Length: invalid\r\n\r\n")?;
                    writer.flush()?;
                }
                "json_error" => write_frame(
                    &mut writer,
                    &format!(
                        r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32001,"message":"fake failure","data":{{"retry":false}}}}}}"#,
                        id.unwrap_or(0)
                    ),
                )?,
                "crash_request" | "restart_exhausted" => return Ok(()),
                "restart_then_success" => {
                    let crashed = marker
                        .as_deref()
                        .and_then(|path| fs::read_to_string(path).ok())
                        .is_some_and(|contents| contents.contains("crashed-once"));
                    if !crashed {
                        if let Some(marker) = &marker {
                            append_marker(marker, "crashed-once\n")?;
                        }
                        return Ok(());
                    }
                    write_result(&mut writer, id, method)?;
                }
                "unknown_id" => {
                    write_frame(
                        &mut writer,
                        r#"{"jsonrpc":"2.0","id":999999,"result":{"ignored":true}}"#,
                    )?;
                    write_result(&mut writer, id, method)?;
                }
                "server_request" => {
                    write_frame(
                        &mut writer,
                        r#"{"jsonrpc":"2.0","id":"server-request","method":"fake/serverRequest","params":{}}"#,
                    )?;
                    write_result(&mut writer, id, method)?;
                }
                _ => write_result(&mut writer, id, method)?,
            },
            None => {
                // JSON-RPC response to a fake server->client request.
            }
        }
    }
}

fn maybe_publish_diagnostics(
    writer: &mut impl Write,
    scenario: &str,
    document_notification: &str,
    marker: Option<&Path>,
) -> io::Result<()> {
    if !scenario.starts_with("diagnostics_") || scenario == "diagnostics_timeout" {
        return Ok(());
    }
    let Some(mut uri) = json_string_field(document_notification, "uri") else {
        return Ok(());
    };
    let mut version = json_u64_field(document_notification, "version").unwrap_or(1);
    if scenario == "diagnostics_wrong_uri" {
        uri = path_to_file_uri(&env::current_dir()?.join("src/other.rs"));
    } else if scenario == "diagnostics_external_uri" {
        uri = "file:///usr/lib/rustlib/src/rust/library/core/src/lib.rs".to_string();
    }
    if scenario == "diagnostics_delayed" {
        thread::sleep(Duration::from_millis(50));
    }
    if scenario == "diagnostics_malformed_notification" {
        write_frame(
            writer,
            r#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":7,"diagnostics":"bad"}}"#,
        )?;
    }
    if scenario == "diagnostics_stale_then_timeout" {
        let already_published = marker
            .and_then(|path| fs::read_to_string(path).ok())
            .is_some_and(|contents| contents.contains("diagnostics-publication"));
        if already_published {
            return Ok(());
        }
        version = 0;
    }
    let diagnostics = match scenario {
        "diagnostics_empty"
        | "diagnostics_delayed"
        | "diagnostics_wrong_uri"
        | "diagnostics_external_uri"
        | "diagnostics_stale_then_timeout"
        | "diagnostics_malformed_notification" => "[]".to_string(),
        "diagnostics_one" => r#"[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":4}},"severity":1,"code":"E0308","source":"rust-analyzer","message":"type mismatch","tags":[]}]"#.to_string(),
        "diagnostics_mixed" => r#"[{"range":{"start":{"line":3,"character":0},"end":{"line":3,"character":2}},"severity":4,"message":"hint","tags":[2,9]},{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"severity":2,"code":7,"source":"rust-analyzer","message":"warning at file:///secret/source.rs\nnext","tags":[1],"data":{"private":"file:///secret/data"},"relatedInformation":[{"location":{"uri":"file:///secret/related.rs"},"message":"hidden"}]},{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"severity":3,"message":"information","tags":[]},{"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":1}},"severity":9,"message":"unknown","tags":[]},{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"severity":1,"message":"error","tags":[]}]"#.to_string(),
        "diagnostics_duplicates" => r#"[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"severity":1,"message":"same","tags":[]},{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"severity":1,"message":"same","tags":[]}]"#.to_string(),
        "diagnostics_malformed_range" => r#"[{"range":{"start":{"line":999,"character":0},"end":{"line":999,"character":1}},"severity":1,"message":"bad range"},{"range":{"start":{"line":1,"character":1},"end":{"line":1,"character":0}},"severity":2,"message":"reversed"},{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"severity":2,"message":"valid"},7]"#.to_string(),
        "diagnostics_utf16" => r#"[{"range":{"start":{"line":2,"character":3},"end":{"line":2,"character":5}},"severity":1,"message":"emoji"}]"#.to_string(),
        "diagnostics_oversized_message" => {
            let message = format!("{} file:///secret/source.rs", "x".repeat(5000));
            format!(r#"[{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":1}}}},"severity":1,"source":"{}","code":"{}","message":"{}"}}]"#, "s".repeat(300), "c".repeat(300), json_escape(&message))
        }
        "diagnostics_overflow" => {
            let items = (0..520)
                .map(|index| format!(r#"{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":1}}}},"severity":2,"message":"diagnostic {index}"}}"#))
                .collect::<Vec<_>>();
            format!("[{}]", items.join(","))
        }
        "diagnostics_text_budget" => {
            let message = "m".repeat(4096);
            let items = (0..30)
                .map(|index| format!(r#"{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":1}}}},"severity":2,"code":"{index}","message":"{message}"}}"#))
                .collect::<Vec<_>>();
            format!("[{}]", items.join(","))
        }
        _ => return Ok(()),
    };
    write_frame(
        writer,
        &format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{{"uri":"{}","version":{version},"diagnostics":{diagnostics}}}}}"#,
            json_escape(&uri)
        ),
    )?;
    if let Some(marker) = marker {
        append_marker(marker, "diagnostics-publication\n")?;
    }
    Ok(())
}

fn start_count(marker: Option<&Path>) -> usize {
    marker
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|contents| {
            contents
                .lines()
                .filter(|line| line.starts_with("start:"))
                .count()
        })
        .unwrap_or(0)
}

fn write_result(writer: &mut impl Write, id: Option<u64>, method: &str) -> io::Result<()> {
    let cwd = env::current_dir()?.display().to_string();
    let main_uri = path_to_file_uri(&Path::new(&cwd).join("src/main.rs"));
    let other_uri = path_to_file_uri(&Path::new(&cwd).join("src/other.rs"));
    let external_uri = "file:///usr/lib/rustlib/src/rust/library/core/src/lib.rs";
    // Scenario is argv[1]; navigation scenarios encode response shape without
    // process-global env vars (which race under parallel tests).
    let scenario = env::args().nth(1).unwrap_or_else(|| "normal".to_string());
    let body = match method {
        "textDocument/documentSymbol" => match scenario.as_str() {
            "symbol_information" => format!(
                r#"[{{"name":"main","kind":12,"location":{{"uri":"{main_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}}}}]"#
            ),
            "symbols_malformed" => {
                r#"[{"name":"bad","kind":12,"range":{"start":{"line":999,"character":0},"end":{"line":999,"character":1}}}]"#.to_string()
            }
            _ => format!(
                r#"[{{"name":"outer","kind":5,"range":{{"start":{{"line":0,"character":0}},"end":{{"line":3,"character":1}}}},"selectionRange":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":5}}}},"children":[{{"name":"inner","kind":12,"range":{{"start":{{"line":2,"character":0}},"end":{{"line":2,"character":5}}}},"selectionRange":{{"start":{{"line":2,"character":0}},"end":{{"line":2,"character":2}}}},"children":[]}}]}}]"#
            ),
        },
        "textDocument/definition" => match scenario.as_str() {
            "definition_null" => "null".to_string(),
            "definition_array" => format!(
                r#"[{{"uri":"{main_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}},{{"uri":"{other_uri}","range":{{"start":{{"line":1,"character":0}},"end":{{"line":1,"character":3}}}}}}]"#
            ),
            "definition_link" => format!(
                r#"[{{"targetUri":"{main_uri}","targetRange":{{"start":{{"line":0,"character":0}},"end":{{"line":3,"character":1}}}},"targetSelectionRange":{{"start":{{"line":0,"character":3}},"end":{{"line":0,"character":7}}}}}}]"#
            ),
            "definition_external" => format!(
                r#"[{{"uri":"{external_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}}]"#
            ),
            "definition_malformed" => format!(
                r#"[{{"uri":"{main_uri}","range":{{"start":{{"line":999,"character":0}},"end":{{"line":999,"character":1}}}}}}]"#
            ),
            _ => format!(
                r#"{{"uri":"{main_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}}"#
            ),
        },
        "textDocument/references" => match scenario.as_str() {
            "references_empty" => "null".to_string(),
            "references_duplicates" => format!(
                r#"[{{"uri":"{main_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}},{{"uri":"{main_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}},{{"uri":"{other_uri}","range":{{"start":{{"line":2,"character":0}},"end":{{"line":2,"character":3}}}}}}]"#
            ),
            "references_overflow" => {
                let mut items = Vec::new();
                for i in 0..30 {
                    items.push(format!(
                        r#"{{"uri":"{main_uri}","range":{{"start":{{"line":{i},"character":0}},"end":{{"line":{i},"character":1}}}}}}"#
                    ));
                }
                format!("[{}]", items.join(","))
            }
            "references_external" => format!(
                r#"[{{"uri":"{main_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}},{{"uri":"{external_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}}]"#
            ),
            _ => format!(
                r#"[{{"uri":"{main_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}},{{"uri":"{main_uri}","range":{{"start":{{"line":3,"character":0}},"end":{{"line":3,"character":4}}}}}}]"#
            ),
        },
        "textDocument/hover" => match scenario.as_str() {
            "hover_markup_markdown" => r#"{"contents":{"kind":"markdown","value":"**main** docs"},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":4}}}"#.to_string(),
            "hover_markup_plaintext" => r#"{"contents":{"kind":"plaintext","value":"plain docs"}}"#.to_string(),
            "hover_string" => r#"{"contents":"string docs"}"#.to_string(),
            "hover_marked_string" => r#"{"contents":{"language":"rust","value":"fn main() {}"}}"#.to_string(),
            "hover_array" => r#"{"contents":["first",{"language":"rust","value":"let value = 1;"},"last"]}"#.to_string(),
            "hover_null" => "null".to_string(),
            "hover_oversized" => format!(
                r#"{{"contents":{{"kind":"markdown","value":"{}"}}}}"#,
                "h".repeat(20_000)
            ),
            "hover_invalid_range" => r#"{"contents":"docs","range":{"start":{"line":999,"character":0},"end":{"line":999,"character":1}}}"#.to_string(),
            "hover_utf16" => r#"{"contents":"emoji","range":{"start":{"line":2,"character":3},"end":{"line":2,"character":5}}}"#.to_string(),
            "hover_sanitizer" => r#"{"contents":{"kind":"markdown","value":"source file:///secret/main.rs\u0001\nnext"}}"#.to_string(),
            "hover_malformed" => r#"{"contents":{"kind":"html","value":"bad"}}"#.to_string(),
            _ => r#"{"contents":{"kind":"markdown","value":"hover"}}"#.to_string(),
        },
        "workspace/symbol" => match scenario.as_str() {
            "workspace_symbol_information" => format!(
                r#"[{{"name":"ToolRuntime","kind":23,"containerName":"runtime","location":{{"uri":"{main_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":4}}}}}}}}]"#
            ),
            "workspace_symbol" => format!(
                r#"[{{"name":"AgentBridge","kind":5,"tags":[1],"location":{{"uri":"{other_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":3}}}}}},"data":{{"hidden":true}}}}]"#
            ),
            "workspace_uri_only" => format!(
                r#"[{{"name":"Deferred","kind":12,"location":{{"uri":"{main_uri}"}}}}]"#
            ),
            "workspace_empty" => "null".to_string(),
            "workspace_duplicates" => format!(
                r#"[{{"name":"Zulu","kind":12,"location":{{"uri":"{main_uri}","range":{{"start":{{"line":3,"character":0}},"end":{{"line":3,"character":2}}}}}}}},{{"name":"Alpha","kind":23,"location":{{"uri":"{other_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":2}}}}}}}},{{"name":"Alpha","kind":23,"location":{{"uri":"{other_uri}","range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":2}}}}}}}}]"#
            ),
            "workspace_external" => format!(
                r#"[{{"name":"Local","kind":12,"location":{{"uri":"{main_uri}"}}}},{{"name":"External","kind":12,"location":{{"uri":"{external_uri}"}}}}]"#
            ),
            "workspace_malformed" => format!(
                r#"[7,{{"name":"","kind":12,"location":{{"uri":"{main_uri}"}}}},{{"name":"NoKind","location":{{"uri":"{main_uri}"}}}},{{"name":"BadRange","kind":12,"location":{{"uri":"{main_uri}","range":{{"start":{{"line":999,"character":0}},"end":{{"line":999,"character":1}}}}}}}},{{"name":"Valid","kind":12,"location":{{"uri":"{main_uri}"}}}}]"#
            ),
            "workspace_overflow" => {
                let items = (0..230)
                    .map(|index| format!(r#"{{"name":"Symbol{index:03}","kind":12,"location":{{"uri":"{main_uri}"}}}}"#))
                    .collect::<Vec<_>>();
                format!("[{}]", items.join(","))
            }
            "workspace_sanitizer" => format!(
                r#"[{{"name":"file:///secret/name","kind":12,"containerName":"/secret/container","location":{{"uri":"{main_uri}"}}}}]"#
            ),
            _ => "[]".to_string(),
        },
        _ => format!(
            r#"{{"method":"{}","cwd":"{}"}}"#,
            json_escape(method),
            json_escape(&cwd)
        ),
    };
    write_frame(
        writer,
        &format!(
            r#"{{"jsonrpc":"2.0","id":{},"result":{body}}}"#,
            id.unwrap_or(0),
        ),
    )
}

fn path_to_file_uri(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
    };
    let text = absolute.display().to_string();
    if text.starts_with('/') {
        format!("file://{}", text)
    } else {
        format!("file:///{}", text.replace('\\', "/"))
    }
}

fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.trim().split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let length = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut body = vec![0_u8; length];
    reader.read_exact(&mut body)?;
    String::from_utf8(body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_frame(writer: &mut impl Write, body: &str) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body.as_bytes())?;
    writer.flush()
}

fn json_u64_field(body: &str, field: &str) -> Option<u64> {
    let marker = format!(r#""{field}""#);
    let after = body.split_once(&marker)?.1;
    let after_colon = after.split_once(':')?.1.trim_start();
    let digits = after_colon
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

fn json_string_field(body: &str, field: &str) -> Option<String> {
    let marker = format!(r#""{field}""#);
    let after = body.split_once(&marker)?.1;
    let after_colon = after.split_once(':')?.1.trim_start();
    let quoted = after_colon.strip_prefix('"')?;
    let end = quoted.find('"')?;
    Some(quoted[..end].to_string())
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn append_marker(path: &Path, value: &str) -> io::Result<()> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(value.as_bytes())
}
