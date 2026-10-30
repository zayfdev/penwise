//! Waiting for a page's network to go quiet (the stills' pre-roll: lazy images and fetches must have landed).

use std::collections::HashSet;
use std::time::Duration;

use serde_json::json;
use tokio::sync::broadcast;
use tokio::time::Instant;

use super::{Error, Page};

/// Resource types that stay open on purpose and never mean "still loading".
const OPEN_ENDED: [&str; 3] = ["WebSocket", "EventSource", "Ping"];

impl Page {
    /// Waits until no request has been in flight for `quiet`, or until `limit` has passed. `true` when the
    /// network went quiet, `false` when the limit ended the wait.
    ///
    /// Requests that were already in flight before the call are not known to it, so callers that need
    /// certainty (images) check the page as well.
    ///
    /// # Errors
    /// The `Network` domain could not be enabled or the browser connection ended.
    pub(crate) async fn wait_network_idle(
        &self,
        quiet: Duration,
        limit: Duration,
    ) -> Result<bool, Error> {
        let mut events = self.conn.cdp.subscribe();
        self.command("Network.enable", json!({})).await?;
        // `Network` stays enabled: disabling it would also drop the capture header of `open_capture`.
        self.watch_requests(&mut events, quiet, limit).await
    }

    /// Sends `headers` with every request of this page (a no-op for none).
    pub(super) async fn set_headers(&self, headers: &[(&str, &str)]) -> Result<(), Error> {
        if headers.is_empty() {
            return Ok(());
        }
        let map: serde_json::Map<String, serde_json::Value> = headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), json!(value)))
            .collect();
        self.command("Network.enable", json!({})).await?;
        self.command("Network.setExtraHTTPHeaders", json!({ "headers": map }))
            .await?;
        Ok(())
    }

    async fn watch_requests(
        &self,
        events: &mut broadcast::Receiver<super::Event>,
        quiet: Duration,
        limit: Duration,
    ) -> Result<bool, Error> {
        let deadline = Instant::now() + limit;
        let mut inflight: HashSet<String> = HashSet::new();
        let mut quiet_since = Some(Instant::now());
        loop {
            let wake = quiet_since.map_or(deadline, |since| (since + quiet).min(deadline));
            match tokio::time::timeout_at(wake, events.recv()).await {
                Err(_) => {
                    let settled = quiet_since.is_some_and(|since| Instant::now() >= since + quiet);
                    return Ok(settled);
                }
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => {}
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    return Err(Error::Cdp("devtools events closed".to_owned()));
                }
                Ok(Ok(event)) => {
                    if event.session_id.as_deref() != Some(self.session_id.as_str()) {
                        continue;
                    }
                    let id = event.params["requestId"].as_str().unwrap_or_default();
                    match event.method.as_str() {
                        "Network.requestWillBeSent"
                            if !OPEN_ENDED
                                .contains(&event.params["type"].as_str().unwrap_or_default()) =>
                        {
                            inflight.insert(id.to_owned());
                            quiet_since = None;
                        }
                        "Network.loadingFinished" | "Network.loadingFailed" => {
                            inflight.remove(id);
                            if inflight.is_empty() && quiet_since.is_none() {
                                quiet_since = Some(Instant::now());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
