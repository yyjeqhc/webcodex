use futures_util::{stream::FuturesUnordered, StreamExt};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use webcodex_openai_tunnel::{Health, HealthSnapshot};

/// A bounded read-only local status server. Reading health never dispatches MCP.
pub async fn serve(listener: TcpListener, health: Health) -> Result<(), &'static str> {
    let mut clients = FuturesUnordered::new();
    loop {
        tokio::select! {
            accepted = listener.accept(), if clients.len() < 16 => {
                let (stream, peer) = accepted.map_err(|_| "health listener failed")?;
                if peer.ip().is_loopback() {
                    clients.push(handle(stream, health.clone()));
                }
            }
            Some(_) = clients.next(), if !clients.is_empty() => {},
        }
    }
}
async fn handle(mut stream: TcpStream, health: Health) {
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0; 512];
            let n = stream.read(&mut buffer).await?;
            if n == 0 { return Ok::<_,std::io::Error>(()); }
            bytes.extend_from_slice(&buffer[..n]);
            if bytes.len() > 4096 { return Ok(()); }
            if bytes.windows(4).any(|s|s==b"\r\n\r\n") { break; }
        }
        let snapshot = health.snapshot();
        let line = std::str::from_utf8(&bytes).ok().and_then(|s|s.split("\r\n").next()).unwrap_or("");
        let fields: Vec<_> = line.split_whitespace().collect();
        let (code, allow) = match fields.as_slice() {
            ["GET", "/health" | "/healthz", "HTTP/1.1" | "HTTP/1.0"] => (200,""),
            ["GET", "/readyz", "HTTP/1.1" | "HTTP/1.0"] => (if snapshot.ready {200} else {503},""),
            ["GET", _, "HTTP/1.1" | "HTTP/1.0"] => (404,""),
            [_, _, "HTTP/1.1" | "HTTP/1.0"] => (405,"Allow: GET\r\n"),
            _ => (400,""),
        };
        let body = serde_json::to_vec(&snapshot)?;
        let header = format!("HTTP/1.1 {code} Status\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nConnection: close\r\n{allow}Content-Length: {}\r\n\r\n",body.len());
        stream.write_all(header.as_bytes()).await?;
        stream.write_all(&body).await?;
        stream.shutdown().await
    }).await;
}
pub async fn status(address: std::net::SocketAddr) -> Result<HealthSnapshot, &'static str> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| "status client unavailable")?;
    let mut response = client
        .get(format!("http://{address}/health"))
        .send()
        .await
        .map_err(|_| "Tunnel health endpoint unavailable")?;
    if response.status() != 200 {
        return Err("unexpected health endpoint response");
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "health response incomplete")?
    {
        if body.len() + chunk.len() > 8192 {
            return Err("health response exceeded limit");
        }
        body.extend_from_slice(&chunk);
    }
    let value: HealthSnapshot =
        serde_json::from_slice(&body).map_err(|_| "invalid health response")?;
    if value.schema_version != 1 {
        return Err("unsupported health response schema");
    }
    Ok(value)
}
