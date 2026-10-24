//! Penwise page state: the stroke list, its JSON file, and the fan-out channel.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

/// One finished stroke as the tablet sends it. The seed is a decimal string because JS numbers cannot hold a
/// 64-bit value exactly; the server treats it as opaque.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StrokeMsg {
    pub preset: u32,
    pub argb: i32,
    pub scale: f32,
    pub seed: String,
    /// Five floats per sample: x, y, pressure, tilt, azimuth.
    pub samples: Vec<f32>,
    /// Microseconds since the stroke started, one per sample.
    pub times: Vec<i32>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PageFile {
    version: u32,
    strokes: Vec<StrokeMsg>,
}

/// Minimum replayable payload for a stroke: one sample of geometry.
fn is_well_formed(stroke: &StrokeMsg) -> bool {
    stroke.samples.len() == stroke.times.len() * 5 && !stroke.times.is_empty()
}

pub struct ServerState {
    strokes: RwLock<Vec<StrokeMsg>>,
    tx: broadcast::Sender<String>,
    data_dir: PathBuf,
}

impl ServerState {
    /// Loads the saved page (if any) and returns the shared state.
    pub async fn load(data_dir: PathBuf) -> Arc<Self> {
        let strokes = read_page(&data_dir).await.unwrap_or_default();
        let (tx, _rx) = broadcast::channel(256);
        tracing::info!(strokes = strokes.len(), dir = %data_dir.display(), "page loaded");
        Arc::new(Self {
            strokes: RwLock::new(strokes),
            tx,
            data_dir,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// The `{"t":"page","strokes":[…]}` frame every client gets on connect.
    pub async fn snapshot_frame(&self) -> String {
        let strokes = self.strokes.read().await;
        page_frame(&strokes)
    }

    /// Appends a stroke, persists, and fans it out. Returns false for a malformed stroke.
    pub async fn append(&self, stroke: StrokeMsg) -> bool {
        if !is_well_formed(&stroke) {
            return false;
        }
        let frame = {
            let mut strokes = self.strokes.write().await;
            strokes.push(stroke.clone());
            let frame = frame_of("stroke", &stroke);
            let all = strokes.clone();
            drop(strokes);
            self.save(&all).await;
            frame
        };
        let _ = self.tx.send(frame);
        true
    }

    /// Replaces the whole page (tablet undo, clear or erase).
    pub async fn replace(&self, strokes: Vec<StrokeMsg>) -> usize {
        let strokes: Vec<StrokeMsg> = strokes.into_iter().filter(is_well_formed).collect();
        let all = {
            let mut current = self.strokes.write().await;
            *current = strokes;
            current.clone()
        };
        self.save(&all).await;
        let _ = self.tx.send(page_frame(&all));
        all.len()
    }

    async fn save(&self, strokes: &[StrokeMsg]) {
        let path = self.data_dir.join("page.json");
        let page = PageFile {
            version: 1,
            strokes: strokes.to_vec(),
        };
        let body = match serde_json::to_vec(&page) {
            Ok(body) => body,
            Err(error) => {
                tracing::error!(%error, "page serialize failed");
                return;
            }
        };
        let dir = self.data_dir.clone();
        let result = tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join("page.json"), body)
        })
        .await;
        match result {
            Ok(Ok(())) => {
                tracing::info!(path = %path.display(), strokes = strokes.len(), "page saved");
            }
            Ok(Err(error)) => tracing::error!(%error, "page save failed"),
            Err(error) => tracing::error!(%error, "page save task failed"),
        }
    }
}

fn frame_of(kind: &str, stroke: &StrokeMsg) -> String {
    serde_json::json!({ "t": kind, "stroke": stroke }).to_string()
}

fn page_frame(strokes: &[StrokeMsg]) -> String {
    serde_json::json!({ "t": "page", "strokes": strokes }).to_string()
}

async fn read_page(dir: &Path) -> Result<Vec<StrokeMsg>, String> {
    let path = dir.join("page.json");
    let body = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    let page: PageFile = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok(page.strokes.into_iter().filter(is_well_formed).collect())
}
