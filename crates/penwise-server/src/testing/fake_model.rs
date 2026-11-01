//! A fake Ollama for the handwriting tests.

use std::sync::Arc;
use std::sync::atomic::Ordering;

/// A fake Ollama: answers every `/api/generate` with the current `reply` and counts the calls.
pub(crate) struct FakeModel {
    pub url: String,
    pub reply: Arc<std::sync::Mutex<String>>,
    pub calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl FakeModel {
    pub(crate) async fn start(reply: &str) -> Self {
        let reply = Arc::new(std::sync::Mutex::new(reply.to_owned()));
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let state = (Arc::clone(&reply), Arc::clone(&calls));
        let router = axum::Router::new()
            .route(
                "/api/generate",
                axum::routing::post(
                    |axum::extract::State((reply, calls)): axum::extract::State<(
                        Arc<std::sync::Mutex<String>>,
                        Arc<std::sync::atomic::AtomicUsize>,
                    )>| async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let text = reply.lock().expect("reply").clone();
                        axum::Json(serde_json::json!({ "response": text }))
                    },
                ),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, router).await });
        Self {
            url: format!("http://{addr}"),
            reply,
            calls,
        }
    }

    pub(crate) fn set(&self, text: &str) {
        text.clone_into(&mut self.reply.lock().expect("reply"));
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}
