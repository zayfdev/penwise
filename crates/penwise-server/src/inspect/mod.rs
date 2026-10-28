//! Headless Chromium inspection: a lazily started browser, page snapshots and element info with `file:line`
//! source mapping. The `/inspect/*` debug routes live in [`routes`].
//!
//! One Chromium serves every page; each [`Page`] is a target with its own device metrics. The browser starts on
//! the first request, is replaced when it dies, and is killed when the server shuts down. Nothing here reads
//! penwise's own data directory.

mod base64;
mod browser;
mod element;
mod html_map;
mod routes;
mod source;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::broadcast;

use self::base64::decode_base64;
use browser::{Browser, Connection, Event};
pub use element::{Clip, ElementInfo, SourceLocation};
pub(crate) use routes::routes;

/// Largest accepted viewport side, in CSS pixels.
const MAX_VIEWPORT: u32 = 8000;
/// How long a navigation may take.
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
/// Coordinates are rejected beyond this distance from the origin.
const MAX_COORDINATE: f64 = 1_000_000.0;

/// Why inspection failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("chromium: {0}")]
    Launch(String),
    #[error("devtools: {0}")]
    Cdp(String),
    #[error("page: {0}")]
    Page(String),
    #[error("timed out: {0}")]
    Timeout(&'static str),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("no element at ({x}, {y})")]
    NoElement { x: f64, y: f64 },
    #[error("element not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The server's headless browser: one Chromium for all projects, started on demand.
///
/// Cheap to clone through an [`Arc`] and safe to share across handlers; page creation is serialised inside the
/// browser, while pages themselves are independent targets.
pub struct Inspect {
    browser: Browser,
}

impl Inspect {
    /// Uses `PENWISE_CHROMIUM`, else `/usr/bin/chromium`. Chromium is not started yet.
    #[must_use]
    pub fn new() -> Self {
        let chromium = std::env::var_os("PENWISE_CHROMIUM")
            .map_or_else(|| PathBuf::from("/usr/bin/chromium"), PathBuf::from);
        Self::with_chromium(chromium)
    }

    /// Uses an explicit Chromium binary (tests, or a distribution that installs it elsewhere).
    #[must_use]
    pub fn with_chromium(chromium: impl Into<PathBuf>) -> Self {
        Self {
            browser: Browser::new(chromium.into()),
        }
    }

    /// Opens `url` as a new page with `width`×`height` CSS pixels of viewport, and waits for its load event.
    ///
    /// # Errors
    /// Chromium could not start, a CDP command failed, or the page did not finish loading in time.
    pub async fn open(&self, url: &str, width: u32, height: u32) -> Result<Page, Error> {
        if width == 0 || height == 0 || width > MAX_VIEWPORT || height > MAX_VIEWPORT {
            return Err(Error::Invalid(format!(
                "viewport must be 1..={MAX_VIEWPORT} on both sides"
            )));
        }
        if url.trim().is_empty() {
            return Err(Error::Invalid("url is empty".to_owned()));
        }
        let connection = self.browser.connect().await?;
        let target = connection
            .cdp
            .command(None, "Target.createTarget", json!({"url": "about:blank"}))
            .await?;
        let target_id = target["targetId"]
            .as_str()
            .ok_or_else(|| Error::Cdp("Target.createTarget returned no target".to_owned()))?
            .to_owned();
        let attached = match connection
            .cdp
            .command(
                None,
                "Target.attachToTarget",
                json!({"targetId": target_id, "flatten": true}),
            )
            .await
        {
            Ok(value) => value,
            Err(error) => {
                close_target(&connection, &target_id).await;
                return Err(error);
            }
        };
        let Some(session_id) = attached["sessionId"].as_str().map(str::to_owned) else {
            close_target(&connection, &target_id).await;
            return Err(Error::Cdp("target did not attach".to_owned()));
        };
        let mut page = Page {
            conn: connection,
            target_id,
            session_id,
            frame_id: String::new(),
            url: url.to_owned(),
            root: None,
            closed: false,
        };
        if let Err(error) = page.setup(width, height).await {
            page.close().await;
            return Err(error);
        }
        let mut events = page.conn.cdp.subscribe();
        let navigation = page.command("Page.navigate", json!({"url": url})).await;
        let navigation = match navigation {
            Ok(value) => value,
            Err(error) => {
                page.close().await;
                return Err(error);
            }
        };
        if let Some(reason) = navigation["errorText"].as_str() {
            page.close().await;
            return Err(Error::Page(reason.to_owned()));
        }
        if let Err(error) = wait_for_load(&page, &mut events).await {
            page.close().await;
            return Err(error);
        }
        let tree = page.command("Page.getFrameTree", json!({})).await?;
        let frame = &tree["frameTree"]["frame"];
        page.frame_id.clear();
        page.frame_id
            .push_str(frame["id"].as_str().unwrap_or_default());
        page.url = frame["url"].as_str().unwrap_or(url).to_owned();
        Ok(page)
    }

    /// Captures the page as PNG, optionally cropped.
    ///
    /// # Errors
    /// The CDP screenshot failed or Chromium returned something that is not base64 PNG data.
    pub async fn snapshot(&self, page: &Page, clip: Option<Clip>) -> Result<Vec<u8>, Error> {
        let mut params = json!({"format": "png", "fromSurface": true});
        if let Some(clip) = clip {
            if !clip.x.is_finite()
                || !clip.y.is_finite()
                || !clip.width.is_finite()
                || !clip.height.is_finite()
                || !clip.scale.is_finite()
                || clip.width <= 0.0
                || clip.height <= 0.0
                || clip.scale <= 0.0
            {
                return Err(Error::Invalid(
                    "clip must be finite and positive".to_owned(),
                ));
            }
            params["clip"] = json!({
                "x": clip.x,
                "y": clip.y,
                "width": clip.width,
                "height": clip.height,
                "scale": clip.scale,
            });
            params["captureBeyondViewport"] = Value::Bool(true);
        }
        let captured = page.command("Page.captureScreenshot", params).await?;
        let data = captured["data"]
            .as_str()
            .ok_or_else(|| Error::Cdp("screenshot returned no data".to_owned()))?;
        decode_base64(data)
    }

    /// The element at viewport point `(x, y)`.
    ///
    /// # Errors
    /// No element is there, or the browser could not be asked.
    pub async fn element_at(&self, page: &Page, x: f64, y: f64) -> Result<ElementInfo, Error> {
        if !x.is_finite() || !y.is_finite() {
            return Err(Error::Invalid("x and y must be finite".to_owned()));
        }
        let point_x = coordinate(x, "x")?;
        let point_y = coordinate(y, "y")?;
        // `DOM.getNodeForLocation` requires the page's document to have been requested after the last
        // navigation; `element()` does this implicitly via querySelector, a point lookup does not.
        page.command("DOM.getDocument", json!({"depth": 0})).await?;
        let located = match page
            .command(
                "DOM.getNodeForLocation",
                json!({"x": point_x, "y": point_y, "includeUserAgentShadowDOM": false}),
            )
            .await
        {
            Ok(value) => value,
            Err(Error::Cdp(message)) if message.contains("No node") => {
                return Err(Error::NoElement { x, y });
            }
            Err(error) => return Err(error),
        };
        let node_id = if let Some(node) = located["nodeId"].as_i64() {
            node
        } else {
            let backend = located["backendNodeId"]
                .as_i64()
                .ok_or_else(|| Error::Cdp("no node at point".to_owned()))?;
            let pushed = page
                .command(
                    "DOM.pushNodesByBackendIdsToFrontend",
                    json!({"backendNodeIds": [backend]}),
                )
                .await?;
            pushed["nodeIds"][0]
                .as_i64()
                .ok_or_else(|| Error::Cdp("point did not map to a node".to_owned()))?
        };
        element::info_for(page, node_id).await
    }

    /// The first element matching `selector`.
    ///
    /// # Errors
    /// The selector matches nothing, is not valid CSS, or the browser could not be asked.
    pub async fn element(&self, page: &Page, selector: &str) -> Result<ElementInfo, Error> {
        if selector.trim().is_empty() {
            return Err(Error::Invalid("selector is empty".to_owned()));
        }
        let document = page.command("DOM.getDocument", json!({"depth": 0})).await?;
        let root = document["root"]["nodeId"]
            .as_i64()
            .ok_or_else(|| Error::Cdp("document has no root".to_owned()))?;
        let found = page
            .command(
                "DOM.querySelector",
                json!({"nodeId": root, "selector": selector}),
            )
            .await
            .map_err(|error| match error {
                Error::Cdp(message) => Error::Invalid(format!("selector {selector:?}: {message}")),
                other => other,
            })?;
        let node = found["nodeId"].as_i64().unwrap_or(0);
        if node == 0 {
            return Err(Error::NotFound(selector.to_owned()));
        }
        element::info_for(page, node).await
    }

    /// Graceful shutdown: closes Chromium and waits for the child. Called by the server before it exits.
    pub async fn shutdown(&self) {
        self.browser.shutdown().await;
    }

    /// Synchronous best-effort kill for `Drop` paths.
    pub(crate) fn kill_now(&self) {
        self.browser.kill_now();
    }
}

impl Default for Inspect {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Inspect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Inspect").finish_non_exhaustive()
    }
}

/// One attached page target: its own viewport, DOM and stylesheets.
pub struct Page {
    conn: Arc<Connection>,
    target_id: String,
    session_id: String,
    frame_id: String,
    url: String,
    root: Option<PathBuf>,
    closed: bool,
}

impl Page {
    /// The project root used to turn served URLs into `file` paths. Without it, source entries keep the URL and
    /// line but have no file.
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        self.root = root;
    }

    /// The final document URL (after redirects).
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Closes the target. Dropping a page also asks the browser to close it, best effort.
    pub async fn close(mut self) {
        self.closed = true;
        close_target(&self.conn, &self.target_id).await;
    }

    async fn command(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.conn
            .cdp
            .command(Some(&self.session_id), method, params)
            .await
    }

    async fn style_sheets(&self) -> std::collections::HashMap<String, source::Header> {
        self.conn.cdp.style_sheets().await
    }

    /// The served HTML of the main frame, fetched through CDP (no second HTTP request).
    async fn document_html(&self) -> Option<String> {
        let content = self
            .command(
                "Page.getResourceContent",
                json!({"frameId": self.frame_id, "url": self.url}),
            )
            .await
            .ok()?;
        let text = content["content"].as_str()?;
        if content["base64Encoded"].as_bool() == Some(true) {
            String::from_utf8(decode_base64(text).ok()?).ok()
        } else {
            Some(text.to_owned())
        }
    }

    fn root(&self) -> Option<&std::path::Path> {
        self.root.as_deref()
    }

    async fn setup(&self, width: u32, height: u32) -> Result<(), Error> {
        for domain in ["Page.enable", "DOM.enable", "CSS.enable"] {
            self.command(domain, json!({})).await?;
        }
        self.command(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": 1,
                "mobile": false,
                "screenWidth": width,
                "screenHeight": height,
            }),
        )
        .await?;
        Ok(())
    }
}

impl std::fmt::Debug for Page {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Page")
            .field("url", &self.url)
            .field("target", &self.target_id)
            .finish_non_exhaustive()
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let connection = Arc::clone(&self.conn);
        let target = self.target_id.clone();
        handle.spawn(async move {
            close_target(&connection, &target).await;
        });
    }
}

async fn close_target(connection: &Connection, target_id: &str) {
    let _ = connection
        .cdp
        .command(None, "Target.closeTarget", json!({"targetId": target_id}))
        .await;
}

/// Waits for the main frame's load event; on a timeout, accepts an already-complete document.
async fn wait_for_load(page: &Page, events: &mut broadcast::Receiver<Event>) -> Result<(), Error> {
    let deadline = tokio::time::Instant::now() + LOAD_TIMEOUT;
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Err(_) => {
                return if document_complete(page).await {
                    Ok(())
                } else {
                    Err(Error::Timeout("page load"))
                };
            }
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                return Err(Error::Cdp("devtools events closed".to_owned()));
            }
            Ok(Ok(event)) => {
                if event.session_id.as_deref() == Some(page.session_id.as_str())
                    && event.method == "Page.loadEventFired"
                {
                    return Ok(());
                }
            }
        }
    }
}

async fn document_complete(page: &Page) -> bool {
    let evaluated = page
        .command(
            "Runtime.evaluate",
            json!({"expression": "document.readyState", "returnByValue": true}),
        )
        .await;
    matches!(
        evaluated,
        Ok(value) if value["result"]["value"].as_str() == Some("complete")
    )
}

fn coordinate(value: f64, name: &'static str) -> Result<i64, Error> {
    if value.abs() > MAX_COORDINATE {
        return Err(Error::Invalid(format!("{name} is out of range")));
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "bounded to ±1e6 above, and a viewport point is integral"
    )]
    Ok(value.round() as i64)
}
