//! One lazily started headless Chromium per server, and a tiny raw CDP client over the `DevTools` WebSocket.
//!
//! The process prints its WebSocket URL on stderr (`--remote-debugging-port=0` chooses a free port); the client
//! speaks the protocol directly: numeric request ids answered through per-request channels, and events fanned
//! out on a broadcast channel (the CSS reader also keeps the stylesheet headers that `getMatchedStylesForNode`
//! does not include). A dead connection is noticed by the reader and by `try_wait`; the next request starts a
//! fresh Chromium.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use futures_util::SinkExt;
use futures_util::StreamExt;
use futures_util::stream::{SplitSink, SplitStream};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::client::Response;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{self, Message};

use super::Error;
use super::source::Header;

/// How long Chromium may take to print its `DevTools` URL.
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a single CDP command may take (screenshots of large pages included).
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a graceful `Browser.close` gets before the process is killed.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
const EVENT_CAPACITY: usize = 1024;
/// Largest `DevTools` message accepted, in bytes.
const MAX_MESSAGE: usize = 512 * 1024 * 1024;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<Ws, Message>;
type WsStream = SplitStream<Ws>;

/// A CDP event, tagged with the flattened target session that emitted it.
#[derive(Debug, Clone)]
pub(super) struct Event {
    pub session_id: Option<String>,
    pub method: String,
    #[allow(dead_code, reason = "events are routed before M1 consumes payloads")]
    pub params: Value,
}

/// The browser manager: starts Chromium on first use and restarts it after a crash.
pub(super) struct Browser {
    chromium: PathBuf,
    state: std::sync::Mutex<Option<Arc<Connection>>>,
    start: Mutex<()>,
}

impl Browser {
    pub(super) fn new(chromium: PathBuf) -> Self {
        Self {
            chromium,
            state: std::sync::Mutex::new(None),
            start: Mutex::new(()),
        }
    }

    /// The live browser connection, starting Chromium or replacing a dead one.
    pub(super) async fn connect(&self) -> Result<Arc<Connection>, Error> {
        let _start = self.start.lock().await;
        {
            let state = self.lock_state();
            if let Some(connection) = state.as_ref()
                && connection.alive()
            {
                return Ok(Arc::clone(connection));
            }
        }
        let stale = { self.lock_state().take() };
        if let Some(stale) = stale {
            stale.shutdown().await;
        }
        let connection = Arc::new(Connection::start(&self.chromium).await?);
        *self.lock_state() = Some(Arc::clone(&connection));
        Ok(connection)
    }

    /// Graceful shutdown: close the browser, wait for the process, then make sure it is gone.
    pub(super) async fn shutdown(&self) {
        let connection = { self.lock_state().take() };
        if let Some(connection) = connection {
            connection.shutdown().await;
        }
    }

    /// Synchronous kill for `Drop` paths (a server going away must not leave Chromium behind).
    pub(super) fn kill_now(&self) {
        let state = self.lock_state();
        if let Some(connection) = state.as_ref() {
            connection.process.kill_now();
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, Option<Arc<Connection>>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.kill_now();
    }
}

/// Chromium plus the browser-level CDP connection.
pub(super) struct Connection {
    pub(super) cdp: Cdp,
    process: Process,
}

impl Connection {
    async fn start(chromium: &Path) -> Result<Self, Error> {
        let process = Process::spawn(chromium).await?;
        match Cdp::connect(&process.ws_url).await {
            Ok(cdp) => Ok(Self { cdp, process }),
            Err(error) => {
                process.kill_now();
                Err(error)
            }
        }
    }

    fn alive(&self) -> bool {
        self.cdp.alive() && self.process.alive()
    }

    async fn shutdown(&self) {
        if self.cdp.alive() {
            let _ = tokio::time::timeout(
                CLOSE_TIMEOUT,
                self.cdp.command(None, "Browser.close", json!({})),
            )
            .await;
        }
        self.process.stop().await;
    }
}

/// The Chromium child and its private profile directory.
struct Process {
    child: std::sync::Mutex<Option<Child>>,
    stderr: std::sync::Mutex<Option<JoinHandle<()>>>,
    dir: PathBuf,
    ws_url: String,
}

impl Process {
    async fn spawn(chromium: &Path) -> Result<Self, Error> {
        if !chromium.is_file() {
            return Err(Error::Launch(format!(
                "chromium not found at {} (set PENWISE_CHROMIUM)",
                chromium.display()
            )));
        }
        let dir = unique_profile_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|error| Error::Launch(format!("profile {}: {error}", dir.display())))?;
        let mut child = chromium_command(chromium, &dir)
            .spawn()
            .map_err(|error| Error::Launch(format!("spawn {}: {error}", chromium.display())))?;
        let Some(stderr) = child.stderr.take() else {
            let _ = child.start_kill();
            return Err(Error::Launch("chromium has no stderr".to_owned()));
        };
        let mut lines = BufReader::new(stderr).lines();
        let deadline = tokio::time::Instant::now() + LAUNCH_TIMEOUT;
        let mut recent: VecDeque<String> = VecDeque::new();
        let ws_url = loop {
            match tokio::time::timeout_at(deadline, lines.next_line()).await {
                Err(_) => {
                    let _ = child.start_kill();
                    return Err(Error::Launch(format!(
                        "chromium printed no DevTools URL within {}s",
                        LAUNCH_TIMEOUT.as_secs()
                    )));
                }
                Ok(Err(error)) => {
                    let _ = child.start_kill();
                    return Err(Error::Launch(format!("chromium stderr: {error}")));
                }
                Ok(Ok(None)) => {
                    let _ = child.try_wait();
                    return Err(Error::Launch(format!(
                        "chromium exited before DevTools was ready: {}",
                        recent.into_iter().collect::<Vec<_>>().join(" | ")
                    )));
                }
                Ok(Ok(Some(line))) => {
                    if let Some(url) = line.split("DevTools listening on ").nth(1) {
                        break url.trim().to_owned();
                    }
                    if recent.len() == 20 {
                        recent.pop_front();
                    }
                    recent.push_back(line);
                }
            }
        };
        tracing::debug!(url = %ws_url, "chromium ready");
        // Keep draining stderr so a full pipe can never stall the browser.
        let stderr_task =
            tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
        Ok(Self {
            child: std::sync::Mutex::new(Some(child)),
            stderr: std::sync::Mutex::new(Some(stderr_task)),
            dir,
            ws_url,
        })
    }

    fn alive(&self) -> bool {
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    async fn stop(&self) {
        let child = {
            self.child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
        };
        if let Some(mut child) = child
            && tokio::time::timeout(CLOSE_TIMEOUT, child.wait())
                .await
                .is_err()
        {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        self.abort_stderr();
        let _ = tokio::fs::remove_dir_all(&self.dir).await;
    }

    fn kill_now(&self) {
        let child = {
            self.child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
        };
        if let Some(mut child) = child {
            let _ = child.start_kill();
        }
        self.abort_stderr();
        let _ = std::fs::remove_dir_all(&self.dir);
    }

    fn abort_stderr(&self) {
        let stderr = self
            .stderr
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(task) = stderr {
            task.abort();
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill_now();
    }
}

fn unique_profile_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("penwise-chromium-{}-{nth}", std::process::id()))
}

/// Headless with its own profile, no proxy and no first-run UI; the viewport comes from device metrics.
fn chromium_command(chromium: &Path, dir: &Path) -> Command {
    let mut command = Command::new(chromium);
    command
        .args([
            "--headless",
            "--remote-debugging-port=0",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            "--disable-background-networking",
            "--disable-background-timer-throttling",
            "--disable-renderer-backgrounding",
            "--disable-features=Translate,MediaRouter,OptimizationHints",
            "--hide-scrollbars",
            "--mute-audio",
            "--no-proxy-server",
        ])
        .arg(format!("--user-data-dir={}", dir.display()))
        .arg("about:blank")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    command
}

/// Connects with limits far above the library's 16 MiB per frame: a noisy strip's screenshot is a base64 message
/// of tens of MiB, and exceeding the limit would drop the connection every page shares.
async fn connect(ws_url: &str) -> Result<(Ws, Response), tungstenite::Error> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    tokio_tungstenite::connect_async_with_config(ws_url, Some(config), false).await
}

struct Shared {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    events: broadcast::Sender<Event>,
    sheets: tokio::sync::RwLock<HashMap<String, Header>>,
    dead: AtomicBool,
}

/// A browser-level CDP connection; targets are sessions within it.
pub(super) struct Cdp {
    writer: Mutex<WsSink>,
    shared: Arc<Shared>,
    reader: JoinHandle<()>,
}

impl Cdp {
    async fn connect(ws_url: &str) -> Result<Self, Error> {
        let (stream, _) = tokio::time::timeout(CONNECT_TIMEOUT, connect(ws_url))
            .await
            .map_err(|_| Error::Timeout("devtools websocket"))?
            .map_err(|error| Error::Cdp(format!("connect {ws_url}: {error}")))?;
        let (writer, reader) = stream.split();
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let shared = Arc::new(Shared {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            events,
            sheets: tokio::sync::RwLock::new(HashMap::new()),
            dead: AtomicBool::new(false),
        });
        let reader_shared = Arc::clone(&shared);
        let task = tokio::spawn(async move { read_loop(reader, reader_shared).await });
        Ok(Self {
            writer: Mutex::new(writer),
            shared,
            reader: task,
        })
    }

    pub(super) fn alive(&self) -> bool {
        !self.shared.dead.load(Ordering::SeqCst)
    }

    pub(super) fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.shared.events.subscribe()
    }

    pub(super) async fn style_sheets(&self) -> HashMap<String, Header> {
        self.shared.sheets.read().await.clone()
    }

    /// Sends one command and waits for the matching response id.
    pub(super) async fn command(
        &self,
        session_id: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<Value, Error> {
        if !self.alive() {
            return Err(Error::Cdp("devtools connection closed".to_owned()));
        }
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.shared.pending.lock().await.insert(id, sender);
        let mut message = serde_json::Map::new();
        message.insert("id".to_owned(), Value::from(id));
        message.insert("method".to_owned(), Value::from(method));
        message.insert("params".to_owned(), params);
        if let Some(session_id) = session_id {
            message.insert("sessionId".to_owned(), Value::from(session_id));
        }
        let text = serde_json::to_string(&Value::Object(message))
            .map_err(|error| Error::Cdp(error.to_string()))?;
        let sent = self
            .writer
            .lock()
            .await
            .send(Message::Text(text.into()))
            .await;
        if let Err(error) = sent {
            self.shared.pending.lock().await.remove(&id);
            return Err(Error::Cdp(format!("send {method}: {error}")));
        }
        match tokio::time::timeout(COMMAND_TIMEOUT, receiver).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(message))) => Err(Error::Cdp(message)),
            Ok(Err(_)) => Err(Error::Cdp(format!("{method}: response channel closed"))),
            Err(_) => {
                self.shared.pending.lock().await.remove(&id);
                Err(Error::Timeout("devtools command"))
            }
        }
    }
}

impl Drop for Cdp {
    fn drop(&mut self) {
        self.shared.dead.store(true, Ordering::SeqCst);
        self.reader.abort();
    }
}

async fn read_loop(mut reader: WsStream, shared: Arc<Shared>) {
    while let Some(message) = reader.next().await {
        match message {
            Ok(Message::Text(text)) => handle_message(&shared, &text).await,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }
    shared.dead.store(true, Ordering::SeqCst);
    let mut pending = shared.pending.lock().await;
    for (_, sender) in pending.drain() {
        let _ = sender.send(Err("devtools connection closed".to_owned()));
    }
    drop(pending);
}

async fn handle_message(shared: &Shared, text: &str) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        let Some(sender) = shared.pending.lock().await.remove(&id) else {
            return;
        };
        if let Some(error) = value.get("error") {
            let code = error["code"].as_i64().unwrap_or(0);
            let message = error["message"].as_str().unwrap_or("unknown error");
            let _ = sender.send(Err(format!("CDP error {code}: {message}")));
        } else {
            let _ = sender.send(Ok(value.get("result").cloned().unwrap_or(Value::Null)));
        }
        return;
    }
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return;
    };
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "CSS.styleSheetAdded" => {
            if let Ok(header) = serde_json::from_value::<Header>(params["header"].clone()) {
                shared
                    .sheets
                    .write()
                    .await
                    .insert(header.style_sheet_id.clone(), header);
            }
        }
        "CSS.styleSheetRemoved" => {
            if let Some(id) = params["styleSheetId"].as_str() {
                shared.sheets.write().await.remove(id);
            }
        }
        _ => {}
    }
    let _ = shared.events.send(Event {
        session_id: value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        method: method.to_owned(),
        params,
    });
}
