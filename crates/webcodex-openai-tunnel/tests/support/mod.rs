#![allow(dead_code)]
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    ControlPlaneIdentity, Credential, FixedMcpTarget, TunnelClient,
};

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    reply: oneshot::Sender<Option<String>>,
}
impl Request {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
    pub fn respond(self, status: u16, body: &str) {
        self.raw(status, "Content-Type: application/json\r\n", body);
    }
    pub fn raw(self, status: u16, headers: &str, body: &str) {
        let _ = self.reply.send(Some(format!("HTTP/1.1 {status} Test\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}", body.len())));
    }
    pub fn disconnect(self) {
        let _ = self.reply.send(None);
    }
}
pub struct Server {
    pub url: String,
    pub requests: mpsc::Receiver<Request>,
    task: JoinHandle<()>,
}
impl Server {
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, requests) = mpsc::channel(64);
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut socket, _) = accepted.unwrap(); let tx = tx.clone();
                        tasks.spawn(async move {
                            let mut bytes = Vec::new();
                            let end = loop {
                                let mut b = [0;1024]; let n = socket.read(&mut b).await.unwrap();
                                if n == 0 { return; } bytes.extend_from_slice(&b[..n]);
                                assert!(bytes.len() < 8 * 1024 * 1024);
                                if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") { break pos+4; }
                            };
                            let head = std::str::from_utf8(&bytes[..end]).unwrap();
                            let mut lines = head.split("\r\n"); let mut first = lines.next().unwrap().split(' ');
                            let method = first.next().unwrap().to_string(); let path = first.next().unwrap().to_string();
                            let headers: BTreeMap<_,_> = lines.filter_map(|l| l.split_once(':'))
                                .map(|(k,v)| (k.to_ascii_lowercase(),v.trim().to_string())).collect();
                            let len: usize = headers.get("content-length").map(|v| v.parse().unwrap()).unwrap_or(0);
                            while bytes.len() < end+len {
                                let mut b=[0;4096]; let n=socket.read(&mut b).await.unwrap();
                                if n==0 { return; } bytes.extend_from_slice(&b[..n]);
                            }
                            let (reply, rx)=oneshot::channel();
                            if tx.send(Request { method, path, headers, body: bytes[end..end+len].to_vec(), reply }).await.is_err() { return; }
                            if let Ok(Some(response))=rx.await { let _=socket.write_all(response.as_bytes()).await; }
                        });
                    },
                    Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
                }
            }
        });
        Self {
            url,
            requests,
            task,
        }
    }
    pub async fn next(&mut self) -> Request {
        next(&mut self.requests).await
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub async fn next(rx: &mut mpsc::Receiver<Request>) -> Request {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap()
}
pub fn command(id: &str) -> Value {
    json!({"request_id":id,"shard_token":"fixture-shard", "command_type":"jsonrpc","channel":"main",
        "jsonrpc":{"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"effect"}}})
}
pub fn client(cp: &Server, mcp: &Server, limits: Limits, policy: DeadlinePolicy) -> TunnelClient {
    TunnelClient::new(
        ControlPlaneIdentity::new(
            &cp.url,
            "tunnel_fixture",
            Credential::bearer("control-secret").unwrap(),
        )
        .unwrap(),
        FixedMcpTarget::new(
            &format!("{}/mcp", mcp.url),
            Credential::bearer("local-secret").unwrap(),
        )
        .unwrap(),
        policy,
        limits,
    )
    .unwrap()
}
pub struct Polls {
    task: JoinHandle<()>,
    pub count: Arc<AtomicUsize>,
    pub responses: mpsc::Receiver<Request>,
}
impl Polls {
    pub fn start(cp: &mut Server, batches: Vec<Value>) -> Self {
        let (_, dummy) = mpsc::channel(1);
        let mut incoming = std::mem::replace(&mut cp.requests, dummy);
        let (tx, responses) = mpsc::channel(64);
        let count = Arc::new(AtomicUsize::new(0));
        let n = count.clone();
        let task = tokio::spawn(async move {
            while let Some(request) = incoming.recv().await {
                assert_eq!(
                    request.headers.get("authorization").unwrap(),
                    "Bearer control-secret"
                );
                assert!(!request.headers.contains_key("x-tunnel-client-capabilities"));
                if request.path.contains("/poll?") {
                    let index = n.fetch_add(1, Ordering::SeqCst);
                    if let Some(batch) = batches.get(index) {
                        request.respond(200, &json!({"commands":batch}).to_string());
                    } else {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        request.respond(204, "");
                    }
                } else {
                    assert!(request.path.ends_with("/response"));
                    if tx.send(request).await.is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            task,
            count,
            responses,
        }
    }
    pub async fn response(&mut self) -> Request {
        next(&mut self.responses).await
    }
}
impl Drop for Polls {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub fn start(
    client: TunnelClient,
) -> (
    oneshot::Sender<()>,
    JoinHandle<Result<(), webcodex_openai_tunnel::Error>>,
) {
    let (tx, rx) = oneshot::channel();
    (
        tx,
        tokio::spawn(client.run(async {
            let _ = rx.await;
        })),
    )
}
pub async fn finish(
    tx: oneshot::Sender<()>,
    task: JoinHandle<Result<(), webcodex_openai_tunnel::Error>>,
) -> Result<(), webcodex_openai_tunnel::Error> {
    let _ = tx.send(());
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
}
pub async fn settled(health: &webcodex_openai_tunnel::Health) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while health.has_uncertain_work() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
