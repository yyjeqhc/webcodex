use salvo::cors::Cors;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageKind {
    Text,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub channel: String,
    pub kind: MessageKind,
    pub title: Option<String>,
    pub text: Option<String>,
    pub file_name: Option<String>,
    pub file_path: Option<String>,
    pub file_size: Option<i64>,
    pub mime_type: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMessageRequest {
    pub channel: String,
    pub title: Option<String>,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct Channel {
    pub name: String,
    pub display_name: String,
    pub message_count: i64,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub addr: String,
    pub data_dir: PathBuf,
    pub token: Option<String>,
    pub max_text_size: usize,
    pub max_file_size: usize,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            addr: std::env::var("DROP_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            data_dir: std::env::var("DROP_DATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./data")),
            token: std::env::var("DROP_TOKEN").ok(),
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
        }
    }
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("drop.db")
    }
    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }
    pub fn is_auth_enabled(&self) -> bool {
        self.token.is_some()
    }
    pub fn validate_token(&self, token: &str) -> bool {
        self.token.as_ref().map(|t| t == token).unwrap_or(false)
    }
}

use rusqlite::{params, Connection};

pub struct Database {
    conn: Mutex<Connection>,
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get(0)?,
        channel: row.get(1)?,
        kind: match row.get::<_, String>(2)?.as_str() {
            "file" => MessageKind::File,
            _ => MessageKind::Text,
        },
        title: row.get(3)?,
        text: row.get(4)?,
        file_name: row.get(5)?,
        file_path: row.get(6)?,
        file_size: row.get(7)?,
        mime_type: row.get(8)?,
        created_at: row.get(9)?,
        expires_at: row.get(10)?,
    })
}

impl Database {
    pub fn open(db_path: &PathBuf) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                channel TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('text', 'file')),
                title TEXT,
                text TEXT,
                file_name TEXT,
                file_path TEXT,
                file_size INTEGER,
                mime_type TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel);
            CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at DESC);",
        )?;
        Ok(())
    }

    pub fn insert_message(&self, message: &Message) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                message.id, message.channel,
                match message.kind { MessageKind::Text => "text", MessageKind::File => "file" },
                message.title, message.text, message.file_name, message.file_path,
                message.file_size, message.mime_type, message.created_at, message.expires_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_message(&self, id: &str) -> anyhow::Result<Option<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages WHERE id = ?1"
        )?;
        let mut rows = stmt.query_map(params![id], row_to_message)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn delete_message(&self, id: &str) -> anyhow::Result<Option<Message>> {
        let message = self.get_message(id)?;
        if message.is_some() {
            let conn = self.conn.lock().unwrap();
            conn.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
        }
        Ok(message)
    }

    pub fn list_messages(
        &self,
        channel: Option<&str>,
        limit: usize,
        before: Option<i64>,
    ) -> anyhow::Result<(Vec<Message>, bool)> {
        let conn = self.conn.lock().unwrap();
        let mut messages = Vec::new();

        let sql = match (channel, before) {
            (Some(_), Some(_)) => "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages WHERE channel = ?1 AND created_at < ?2 ORDER BY created_at DESC LIMIT ?3",
            (Some(_), None) => "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages WHERE channel = ?1 ORDER BY created_at DESC LIMIT ?2",
            (None, Some(_)) => "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages WHERE created_at < ?1 ORDER BY created_at DESC LIMIT ?2",
            (None, None) => "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages ORDER BY created_at DESC LIMIT ?1",
        };

        let mut stmt = conn.prepare(sql)?;
        let query_limit = limit as i64 + 1;
        let rows = match (channel, before) {
            (Some(ch), Some(before_ts)) => {
                stmt.query_map(params![ch, before_ts, query_limit], row_to_message)?
            }
            (Some(ch), None) => stmt.query_map(params![ch, query_limit], row_to_message)?,
            (None, Some(before_ts)) => {
                stmt.query_map(params![before_ts, query_limit], row_to_message)?
            }
            (None, None) => stmt.query_map(params![query_limit], row_to_message)?,
        };

        for row in rows {
            messages.push(row?);
        }
        let has_more = messages.len() > limit;
        messages.truncate(limit);
        Ok((messages, has_more))
    }

    pub fn list_channels(&self) -> anyhow::Result<Vec<Channel>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT channel, COUNT(*) as cnt FROM messages GROUP BY channel ORDER BY cnt DESC",
        )?;
        let default_channels = vec![
            ("inbox", "Inbox"),
            ("xline", "Xline"),
            ("thesis", "Thesis"),
            ("packfix", "Packfix"),
            ("omo", "OMO"),
            ("files", "Files"),
        ];
        let mut channels: Vec<Channel> = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                let display_name = default_channels
                    .iter()
                    .find(|(n, _)| *n == name.as_str())
                    .map(|(_, d)| d.to_string())
                    .unwrap_or_else(|| name.clone());
                Ok(Channel {
                    name,
                    display_name,
                    message_count: count,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for (name, display_name) in default_channels {
            if !channels.iter().any(|c| c.name == name) {
                channels.push(Channel {
                    name: name.to_string(),
                    display_name: display_name.to_string(),
                    message_count: 0,
                });
            }
        }
        Ok(channels)
    }
}

fn get_config(depot: &Depot) -> Option<Arc<Config>> {
    depot.obtain::<Arc<Config>>().ok().cloned()
}

fn get_db(depot: &Depot) -> Option<Arc<Database>> {
    depot.obtain::<Arc<Database>>().ok().cloned()
}

fn check_auth(req: &Request, config: &Config) -> bool {
    if !config.is_auth_enabled() {
        return true;
    }
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if let Some(t) = token {
        return config.validate_token(t);
    }
    req.query::<String>("token")
        .map(|t| config.validate_token(&t))
        .unwrap_or(false)
}

fn json_error(_status: StatusCode, msg: &str) -> Json<serde_json::Value> {
    Json(serde_json::json!({"error": msg}))
}

struct AuthMiddleware;

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let Some(config) = get_config(depot) else {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No config"));
            ctrl.skip_rest();
            return;
        };
        if !check_auth(req, &config) {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
            ctrl.skip_rest();
            return;
        }
        ctrl.call_next(req, depot, res).await;
    }
}

#[handler]
pub async fn health(res: &mut Response) {
    res.render(Json(
        serde_json::json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}),
    ));
}

#[handler]
pub async fn list_channels(depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    match db.list_channels() {
        Ok(channels) => res.render(Json(channels)),
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    }
}

#[handler]
pub async fn list_messages(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let channel = req.query::<String>("channel");
    let limit = req.query::<usize>("limit").unwrap_or(50).min(200);
    let before = req.query::<i64>("before");
    match db.list_messages(channel.as_deref(), limit, before) {
        Ok((messages, has_more)) => res.render(Json(serde_json::json!({
            "messages": messages, "total": messages.len(), "has_more": has_more
        }))),
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    }
}

#[handler]
pub async fn create_message(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No config"));
        return;
    };
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let body: CreateMessageRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                &format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    if body.text.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(StatusCode::BAD_REQUEST, "Text cannot be empty"));
        return;
    }
    if body.text.len() > config.max_text_size {
        res.status_code(StatusCode::PAYLOAD_TOO_LARGE);
        res.render(json_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Payload too large",
        ));
        return;
    }
    let channel = if body.channel.is_empty() {
        "inbox".to_string()
    } else {
        body.channel
    };
    let now = chrono::Utc::now().timestamp();
    let message = Message {
        id: Uuid::new_v4().to_string(),
        channel,
        kind: MessageKind::Text,
        title: body.title,
        text: Some(body.text),
        file_name: None,
        file_path: None,
        file_size: None,
        mime_type: None,
        created_at: now,
        expires_at: None,
    };
    match db.insert_message(&message) {
        Ok(_) => res.render(Json(message)),
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    }
}

#[handler]
pub async fn get_message(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let id = req.param::<String>("id").unwrap_or_default();
    match db.get_message(&id) {
        Ok(Some(message)) => res.render(Json(message)),
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(json_error(StatusCode::NOT_FOUND, "Message not found"));
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    }
}

#[handler]
pub async fn delete_message(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No config"));
        return;
    };
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let id = req.param::<String>("id").unwrap_or_default();
    match db.delete_message(&id) {
        Ok(Some(message)) => {
            if message.kind == MessageKind::File {
                if let Some(file_path) = &message.file_path {
                    let full_path = config.uploads_dir().join(file_path);
                    if let Ok(canonical) = full_path.canonicalize() {
                        let canonical_uploads = config
                            .uploads_dir()
                            .canonicalize()
                            .unwrap_or_else(|_| config.uploads_dir());
                        if canonical.starts_with(&canonical_uploads) {
                            let _ = std::fs::remove_file(canonical);
                        }
                    }
                }
            }
            res.render(Json(serde_json::json!({"deleted": true, "id": id})));
        }
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(json_error(StatusCode::NOT_FOUND, "Message not found"));
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    }
}

#[handler]
pub async fn upload_file(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No config"));
        return;
    };
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let channel = req
        .query::<String>("channel")
        .unwrap_or_else(|| "files".to_string());
    let file = match req.file("file").await {
        Some(f) => f,
        None => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, "No file provided"));
            return;
        }
    };
    let file_size = file.size() as i64;
    if file_size > config.max_file_size as i64 {
        res.status_code(StatusCode::PAYLOAD_TOO_LARGE);
        res.render(json_error(StatusCode::PAYLOAD_TOO_LARGE, "File too large"));
        return;
    }
    let file_id = Uuid::new_v4().to_string();
    let original_name = file.name().unwrap_or("unknown").to_string();
    let extension = PathBuf::from(&original_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();
    let safe_filename = format!("{}{}", file_id, extension);
    let mime_type = file
        .content_type()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let file_path = config.uploads_dir().join(&safe_filename);
    if let Err(e) = std::fs::copy(file.path(), &file_path) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to save file: {}", e),
        ));
        return;
    }
    let now = chrono::Utc::now().timestamp();
    let message = Message {
        id: file_id,
        channel,
        kind: MessageKind::File,
        title: Some(original_name.clone()),
        text: None,
        file_name: Some(original_name),
        file_path: Some(safe_filename),
        file_size: Some(file_size),
        mime_type: Some(mime_type),
        created_at: now,
        expires_at: None,
    };
    match db.insert_message(&message) {
        Ok(_) => res.render(Json(message)),
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    }
}

#[handler]
pub async fn download_file(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No config"));
        return;
    };
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let id = req.param::<String>("file_id").unwrap_or_default();
    let message = match db.get_message(&id) {
        Ok(Some(msg)) => msg,
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(json_error(StatusCode::NOT_FOUND, "File not found"));
            return;
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
            return;
        }
    };
    if message.kind != MessageKind::File {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(json_error(StatusCode::NOT_FOUND, "Not a file message"));
        return;
    }
    let Some(file_path) = &message.file_path else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "File path missing",
        ));
        return;
    };
    let full_path = config.uploads_dir().join(file_path);
    let canonical = match full_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(json_error(StatusCode::NOT_FOUND, "File not found on disk"));
            return;
        }
    };
    let canonical_uploads = config
        .uploads_dir()
        .canonicalize()
        .unwrap_or_else(|_| config.uploads_dir());
    if !canonical.starts_with(&canonical_uploads) {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(json_error(StatusCode::NOT_FOUND, "File not found"));
        return;
    }
    let filename = message.file_name.unwrap_or_else(|| "download".to_string());
    res.add_header(
        "content-disposition",
        &format!("attachment; filename=\"{}\"", filename),
        true,
    )
    .ok();
    if let Some(mime) = message.mime_type {
        res.add_header("content-type", &mime, true).ok();
    }
    res.send_file(canonical, req.headers()).await;
}

#[handler]
pub async fn openapi_json(res: &mut Response) {
    let json_str = include_str!("../data/openapi.json");
    res.render(Text::Json(json_str.to_string()));
}

// ============================================================================
// Web UI
// ============================================================================

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn base_html(title: &str, content: &str, show_nav: bool) -> String {
    let nav = if show_nav {
        r#"<div class="nav"><a href="/">Home</a><a href="/c/inbox">Inbox</a><a href="/c/files">Files</a><a href="/send">Send</a></div>"#
    } else {
        ""
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title} - Private Drop</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f5f5f5;color:#333}}
.container{{max-width:800px;margin:0 auto;padding:16px}}
.header{{background:#2c3e50;color:white;padding:16px;margin-bottom:16px;border-radius:8px}}
.header h1{{font-size:1.5em}}
.nav{{display:flex;gap:8px;margin-bottom:16px;flex-wrap:wrap}}
.nav a{{padding:8px 16px;background:#3498db;color:white;text-decoration:none;border-radius:4px;font-size:14px}}
.nav a:hover{{background:#2980b9}}
.card{{background:white;border-radius:8px;padding:16px;margin-bottom:12px;box-shadow:0 2px 4px rgba(0,0,0,0.1)}}
.card-header{{display:flex;justify-content:space-between;align-items:center;margin-bottom:8px}}
.card-title{{font-weight:bold;font-size:1.1em;word-break:break-word}}
.card-meta{{color:#666;font-size:0.9em}}
.card-text{{white-space:pre-wrap;word-break:break-word;line-height:1.5}}
.btn{{padding:8px 16px;border:none;border-radius:4px;cursor:pointer;font-size:14px;text-decoration:none;display:inline-block}}
.btn-primary{{background:#3498db;color:white}}
.btn-danger{{background:#e74c3c;color:white}}
.btn-success{{background:#27ae60;color:white}}
.btn-sm{{padding:4px 12px;font-size:12px}}
.form-group{{margin-bottom:12px}}
.form-group label{{display:block;margin-bottom:4px;font-weight:bold}}
.form-group input,.form-group textarea,.form-group select{{width:100%;padding:10px;border:1px solid #ddd;border-radius:4px;font-size:14px}}
.form-group textarea{{min-height:150px;resize:vertical}}
.form-actions{{display:flex;gap:8px}}
.file-info{{display:flex;align-items:center;gap:8px}}
.file-icon{{font-size:24px}}
.file-size{{color:#666;font-size:0.9em}}
.token-form{{max-width:400px;margin:100px auto}}
.alert{{padding:12px;border-radius:4px;margin-bottom:12px}}
.alert-error{{background:#f8d7da;color:#721c24}}
.alert-success{{background:#d4edda;color:#155724}}
.channel-badge{{display:inline-block;padding:2px 8px;background:#ecf0f1;border-radius:12px;font-size:0.8em}}
@media(max-width:600px){{.container{{padding:8px}}.header{{padding:12px}}.card{{padding:12px}}}}
</style>
</head>
<body>
<div class="container">
<div class="header"><h1>Private Drop</h1></div>
{nav}
{content}
</div>
<script>
function getToken(){{return localStorage.getItem('drop_token')||''}}
function setToken(t){{localStorage.setItem('drop_token',t)}}
function clearToken(){{localStorage.removeItem('drop_token')}}
async function apiCall(url,options={{}}){{const token=getToken();const headers={{...options.headers}};if(token)headers['Authorization']='Bearer '+token;const resp=await fetch(url,{{...options,headers}});if(resp.status===401){{clearToken();window.location.href='/login';return null}}return resp}}
async function deleteMessage(id){{if(!confirm('Are you sure?'))return;const resp=await apiCall('/api/messages/'+id,{{method:'DELETE'}});if(resp&&resp.ok)window.location.reload()}}
function copyText(t){{navigator.clipboard.writeText(t).then(()=>alert('Copied!'))}}
</script>
</body>
</html>"#,
        title = html_escape(title),
        content = content,
        nav = nav
    )
}

fn format_size(size: i64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn mime_icon(mime: Option<&str>) -> &'static str {
    match mime {
        Some(m) if m.starts_with("image/") => "🖼",
        Some(m) if m.starts_with("video/") => "🎥",
        Some(m) if m.starts_with("audio/") => "🎵",
        Some(m) if m.starts_with("text/") => "📄",
        Some(m) if m.contains("pdf") => "📕",
        Some(m) if m.contains("zip") || m.contains("rar") => "📦",
        _ => "📎",
    }
}

fn render_card(m: &Message, show_channel: bool) -> String {
    let ch = if show_channel {
        format!(
            r#"<span class="channel-badge">{}</span>"#,
            html_escape(&m.channel)
        )
    } else {
        String::new()
    };
    let title = m
        .title
        .as_ref()
        .map(|t| html_escape(t))
        .unwrap_or_else(|| match &m.kind {
            MessageKind::Text => "Text".to_string(),
            MessageKind::File => m
                .file_name
                .as_ref()
                .map(|f| html_escape(f))
                .unwrap_or_else(|| "File".to_string()),
        });
    let content = match &m.kind {
        MessageKind::Text => {
            let text = m.text.as_deref().unwrap_or("");
            let preview = if text.len() > 200 {
                format!("{}...", &text[..200])
            } else {
                text.to_string()
            };
            format!(r#"<div class="card-text">{}</div>"#, html_escape(&preview))
        }
        MessageKind::File => {
            let size = m.file_size.unwrap_or(0);
            format!(
                r#"<div class="file-info"><span class="file-icon">{}</span><div><div style="font-weight:bold">{}</div><div class="file-size">{}</div></div></div>"#,
                mime_icon(m.mime_type.as_deref()),
                html_escape(m.file_name.as_deref().unwrap_or("unknown")),
                format_size(size)
            )
        }
    };
    let actions = match &m.kind {
        MessageKind::Text => format!(
            r#"<button class="btn btn-sm btn-primary" onclick="copyText(document.getElementById('t-{}').textContent)">Copy</button> <button class="btn btn-sm btn-danger" onclick="deleteMessage('{}')">Del</button>"#,
            m.id, m.id
        ),
        MessageKind::File => format!(
            r#"<a href="/api/files/{}" class="btn btn-sm btn-success" download>Download</a> <button class="btn btn-sm btn-danger" onclick="deleteMessage('{}')">Del</button>"#,
            m.id, m.id
        ),
    };
    let ts = chrono::DateTime::from_timestamp(m.created_at, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    format!(
        r#"<div class="card" id="t-{}"><div class="card-header"><div><div class="card-title">{}<a href="/m/{}" style="color:inherit;text-decoration:none">{}</a></div><div class="card-meta">{}</div></div><div class="form-actions">{}</div></div>{}</div>"#,
        m.id, ch, m.id, title, ts, actions, content
    )
}

#[handler]
pub async fn login_page(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    if !config.is_auth_enabled() {
        res.render(Redirect::found("/"));
        return;
    }
    let error = req
        .query::<String>("error")
        .map(|e| {
            format!(
                r#"<div class="alert alert-error">{}</div>"#,
                html_escape(&e)
            )
        })
        .unwrap_or_default();
    let content = format!(
        r#"<div class="token-form"><div class="card"><h2 style="margin-bottom:16px">Login</h2>{error}<form onsubmit="event.preventDefault();setToken(document.getElementById('token').value);window.location.href='/'"><div class="form-group"><label for="token">Access Token</label><input type="password" id="token" placeholder="Enter your token" required autofocus></div><div class="form-actions"><button type="submit" class="btn btn-primary">Login</button></div></form></div></div>"#
    );
    res.render(Text::Html(base_html("Login", &content, false)));
}

#[handler]
pub async fn home_page(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    if !check_auth(req, &config) {
        res.render(Redirect::found("/login"));
        return;
    }
    let channels = db.list_channels().unwrap_or_default();
    let (messages, _) = db.list_messages(None, 20, None).unwrap_or_default();
    let ch_html: String = channels.iter().map(|c| format!(r#"<a href="/c/{}" style="display:flex;justify-content:space-between;align-items:center;background:white;padding:12px;margin-bottom:8px;border-radius:8px;box-shadow:0 1px 3px rgba(0,0,0,0.1);text-decoration:none;color:#333"><span style="font-weight:bold">{}</span><span style="color:#666;font-size:0.9em">{} messages</span></a>"#, html_escape(&c.name), html_escape(&c.display_name), c.message_count)).collect();
    let msg_html: String = messages.iter().map(|m| render_card(m, true)).collect();
    let content = format!(
        r#"<h2 style="margin-bottom:16px">Channels</h2>{ch_html}<h2 style="margin:16px 0">Recent Messages</h2>{msg_html}"#
    );
    res.render(Text::Html(base_html("Home", &content, true)));
}

#[handler]
pub async fn channel_page(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    if !check_auth(req, &config) {
        res.render(Redirect::found("/login"));
        return;
    }
    let channel = req.param::<String>("channel").unwrap_or_default();
    let (messages, has_more) = db
        .list_messages(Some(&channel), 50, None)
        .unwrap_or_default();
    let msg_html: String = messages.iter().map(|m| render_card(m, false)).collect();
    let more = if has_more {
        let before = messages.last().map(|m| m.created_at).unwrap_or(0);
        format!(
            r#"<div style="text-align:center;margin-top:16px"><a href="/c/{}?before={}" class="btn btn-primary">Load More</a></div>"#,
            html_escape(&channel),
            before
        )
    } else {
        String::new()
    };
    let content = format!(
        r#"<div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:16px"><h2>{}</h2><a href="/send?channel={}" class="btn btn-primary">Send</a></div>{}{}"#,
        html_escape(&channel),
        html_escape(&channel),
        msg_html,
        more
    );
    res.render(Text::Html(base_html(&channel, &content, true)));
}

#[handler]
pub async fn message_page(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    if !check_auth(req, &config) {
        res.render(Redirect::found("/login"));
        return;
    }
    let id = req.param::<String>("id").unwrap_or_default();
    let message = match db.get_message(&id) {
        Ok(Some(m)) => m,
        _ => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Text::Html(base_html(
                "Not Found",
                r#"<div class="card"><h2>Message not found</h2></div>"#,
                true,
            )));
            return;
        }
    };
    let ts = chrono::DateTime::from_timestamp(message.created_at, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let content = match &message.kind {
        MessageKind::Text => {
            let text = message.text.as_deref().unwrap_or("");
            format!(
                r#"<div class="card"><div style="display:flex;justify-content:space-between;margin-bottom:12px"><div><span class="channel-badge">{}</span> <span class="card-meta">{}</span></div><div class="form-actions"><button class="btn btn-sm btn-primary" onclick="copyText(document.getElementById('ft').textContent)">Copy</button><button class="btn btn-sm btn-danger" onclick="deleteMessage('{}')">Del</button></div></div><div id="ft" class="card-text" style="max-height:none">{}</div></div>"#,
                html_escape(&message.channel),
                ts,
                message.id,
                html_escape(text)
            )
        }
        MessageKind::File => {
            let size = message.file_size.unwrap_or(0);
            format!(
                r#"<div class="card"><div style="display:flex;justify-content:space-between;margin-bottom:12px"><div><span class="channel-badge">{}</span> <span class="card-meta">{}</span></div><div class="form-actions"><a href="/api/files/{}" class="btn btn-sm btn-success" download>Download</a><button class="btn btn-sm btn-danger" onclick="deleteMessage('{}')">Del</button></div></div><div class="file-info" style="font-size:1.2em"><span class="file-icon" style="font-size:48px">{}</span><div><div style="font-weight:bold;font-size:1.2em">{}</div><div class="file-size">{}</div><div class="file-size">{}</div></div></div></div>"#,
                html_escape(&message.channel),
                ts,
                message.id,
                message.id,
                mime_icon(message.mime_type.as_deref()),
                html_escape(message.file_name.as_deref().unwrap_or("unknown")),
                format_size(size),
                message.mime_type.as_deref().unwrap_or("unknown")
            )
        }
    };
    let title = message.title.as_deref().unwrap_or("Message");
    res.render(Text::Html(base_html(
        title,
        &format!(
            r#"<h2 style="margin-bottom:16px">{}</h2>{}"#,
            html_escape(title),
            content
        ),
        true,
    )));
}

#[handler]
pub async fn send_page(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(config) = get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };
    if !check_auth(req, &config) {
        res.render(Redirect::found("/login"));
        return;
    }
    let channel = req
        .query::<String>("channel")
        .unwrap_or_else(|| "inbox".to_string());
    let success = req
        .query::<String>("success")
        .map(|s| {
            format!(
                r#"<div class="alert alert-success">{}</div>"#,
                html_escape(&s)
            )
        })
        .unwrap_or_default();
    let error = req
        .query::<String>("error")
        .map(|e| {
            format!(
                r#"<div class="alert alert-error">{}</div>"#,
                html_escape(&e)
            )
        })
        .unwrap_or_default();
    let sel = |ch: &str| if channel == ch { "selected" } else { "" };
    let content = format!(
        r#"<h2 style="margin-bottom:16px">Send Message</h2>{success}{error}
<div class="card"><h3 style="margin-bottom:12px">Text Message</h3><form id="textForm" onsubmit="sendText(event)"><div class="form-group"><label for="channel">Channel</label><select id="channel"><option value="inbox" {}>inbox</option><option value="xline" {}>xline</option><option value="thesis" {}>thesis</option><option value="packfix" {}>packfix</option><option value="omo" {}>omo</option><option value="files" {}>files</option></select></div><div class="form-group"><label for="title">Title (optional)</label><input type="text" id="title" placeholder="Message title"></div><div class="form-group"><label for="text">Text</label><textarea id="text" placeholder="Paste your text here..." rows="10" required></textarea></div><div class="form-actions"><button type="submit" class="btn btn-primary">Send</button></div></form></div>
<div class="card" style="margin-top:16px"><h3 style="margin-bottom:12px">Upload File</h3><form id="fileForm" onsubmit="uploadFile(event)"><div class="form-group"><label for="file">File</label><input type="file" id="file" required></div><div class="form-actions"><button type="submit" class="btn btn-success">Upload</button></div></form></div>
<script>
async function sendText(e){{e.preventDefault();const resp=await apiCall('/api/messages',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{channel:document.getElementById('channel').value,title:document.getElementById('title').value||null,text:document.getElementById('text').value}})}});if(resp&&resp.ok)window.location.href='/send?success=Message+sent';else if(resp){{const d=await resp.json();window.location.href='/send?error='+encodeURIComponent(d.error||'Failed')}}}}
async function uploadFile(e){{e.preventDefault();const fd=new FormData();fd.append('file',document.getElementById('file').files[0]);const resp=await apiCall('/api/files?channel='+document.getElementById('channel').value,{{method:'POST',body:fd}});if(resp&&resp.ok)window.location.href='/send?success=File+uploaded';else if(resp){{const d=await resp.json();window.location.href='/send?error='+encodeURIComponent(d.error||'Failed')}}}}
</script>"#,
        sel("inbox"),
        sel("xline"),
        sel("thesis"),
        sel("packfix"),
        sel("omo"),
        sel("files")
    );
    res.render(Text::Html(base_html("Send", &content, true)));
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let config = Config::from_env();
    if !config.is_auth_enabled() {
        tracing::warn!(
            "DROP_TOKEN is not set! Running in development mode without authentication."
        );
        tracing::warn!("Set DROP_TOKEN environment variable to enable authentication.");
    }
    tracing::info!("Starting Private Drop v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Data directory: {:?}", config.data_dir);
    let addr = config.addr.clone();
    tracing::info!("Listening on: {}", addr);
    std::fs::create_dir_all(config.uploads_dir())?;
    let db = Database::open(&config.db_path())?;
    tracing::info!("Database initialized at {:?}", config.db_path());

    // Set max payload size to 2MB for text messages
    salvo::http::request::set_global_secure_max_size(config.max_text_size);

    let cors = Cors::permissive();
    let config = Arc::new(config);
    let db = Arc::new(db);

    let api_router = Router::with_path("api")
        .push(Router::with_path("health").get(health))
        .push(
            Router::new()
                .hoop(AuthMiddleware)
                .push(Router::with_path("channels").get(list_channels))
                .push(
                    Router::with_path("messages")
                        .get(list_messages)
                        .post(create_message),
                )
                .push(
                    Router::with_path("messages/{id}")
                        .get(get_message)
                        .delete(delete_message),
                )
                .push(Router::with_path("files/{file_id}").get(download_file))
                .push(Router::with_path("files").post(upload_file)),
        );

    let web_router = Router::new()
        .push(Router::with_path("login").get(login_page))
        .push(Router::with_path("send").get(send_page))
        .push(Router::with_path("c/{channel}").get(channel_page))
        .push(Router::with_path("m/{id}").get(message_page))
        .push(Router::with_path("").get(home_page));

    let openapi_router = Router::with_path("openapi.json").get(openapi_json);

    let router = Router::new()
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .hoop(cors.into_handler())
        .push(api_router)
        .push(openapi_router)
        .push(web_router);

    let acceptor = TcpListener::new(addr.clone()).bind().await;
    tracing::info!("Server started successfully!");
    let port = addr.split(':').last().unwrap_or("8080");
    tracing::info!("Web UI: http://localhost:{}", port);
    tracing::info!("API: http://localhost:{}/api", port);
    tracing::info!("OpenAPI: http://localhost:{}/openapi.json", port);
    Server::new(acceptor).serve(router).await;
    Ok(())
}
