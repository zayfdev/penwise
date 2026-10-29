//! Unit-test rig for the feature modules: a storage in a temp dir with one authoring device, plus a PNG
//! asset store beside it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};

use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, Layer};
use penwise_proto::{
    AnyOp, Background, DeviceId, HlcClock, ItemRef, NotebookChange, NotebookOp, Op, PageChange,
    PageId, PageKind, PageOp,
};
use penwise_testkit::{IdGen, TempDir, stroke_item};
use tiny_skia::Pixmap;

use crate::api::AppState;
use crate::assets::{Assets, StoredAsset};
use crate::pairing::Pairing;
use crate::storage::{PageView, Storage, StorageConfig};

pub(crate) const T0: u64 = 1_800_000_000_000;

/// Each rig gets its own project-port range, so parallel tests cannot grab a port another test just released.
/// The ranges start at 17,421: tests never bind the production 7421+ ports.
static NEXT_PORT_BASE: AtomicU16 = AtomicU16::new(17_421);

pub(crate) struct Rig {
    pub dir: TempDir,
    pub storage: Storage,
    pub assets: Arc<Assets>,
    /// Handwriting model endpoint handed to handlers (empty = none reachable).
    pub ocr_url: String,
    now: Arc<AtomicU64>,
    ids: IdGen,
    device: DeviceId,
    clock: HlcClock,
    next_item: u128,
}

impl Rig {
    pub(crate) fn new() -> Self {
        let dir = TempDir::new("features").expect("tempdir");
        let now = Arc::new(AtomicU64::new(T0));
        let mut config = StorageConfig::new(dir.path().join(crate::serve::DB_FILE));
        let clock = Arc::clone(&now);
        config.now_ms = Arc::new(move || clock.load(Ordering::SeqCst));
        let storage = Storage::open(config).expect("storage");
        let assets = Assets::load(dir.path().join("assets"));
        let mut ids = IdGen::new(11);
        let device = ids.device();
        Self {
            dir,
            storage,
            assets,
            ocr_url: String::new(),
            now,
            ids,
            device,
            clock: HlcClock::new(device),
            next_item: 1,
        }
    }

    /// The handler state over this rig's storage and data directory.
    pub(crate) fn app(&self) -> AppState {
        self.app_with_projects(self.projects())
    }

    /// This rig's data directory and a fresh registry (tests that link projects share one).
    pub(crate) fn projects(&self) -> Arc<crate::frames::registry::Registry> {
        Arc::new(crate::frames::registry::Registry::new(
            self.storage.clone(),
            Arc::new(self.dir.path().to_path_buf()),
            Arc::new(String::new()),
            "127.0.0.1".to_owned(),
            NEXT_PORT_BASE.fetch_add(16, Ordering::Relaxed),
        ))
    }

    /// The handler state with a caller-owned registry (so a test can see what a link started).
    pub(crate) fn app_with_projects(
        &self,
        projects: Arc<crate::frames::registry::Registry>,
    ) -> AppState {
        AppState {
            storage: self.storage.clone(),
            token: Arc::new(String::new()),
            data_dir: Arc::new(self.dir.path().to_path_buf()),
            web: Arc::new(self.dir.path().to_path_buf()),
            pairing: Arc::new(Pairing::new()),
            urls: Arc::new(crate::urls::Urls::fixed(Vec::new())),
            assets: Arc::clone(&self.assets),
            ocr_url: Arc::new(self.ocr_url.clone()),
            inspect: Arc::new(crate::inspect::Inspect::new()),
            projects,
        }
    }

    /// Serves `routes` (no auth layer) on 127.0.0.1:0; returns the base URL. The peer address is available, so
    /// handlers that care about loopback behave as in production.
    pub(crate) async fn serve(&self, routes: axum::Router<AppState>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let router = routes.with_state(self.app());
        tokio::spawn(async move {
            let router = router.into_make_service_with_connect_info::<std::net::SocketAddr>();
            axum::serve(listener, router).await
        });
        format!("http://{addr}")
    }

    fn hlc(&mut self) -> penwise_proto::Hlc {
        self.clock.send(self.now.load(Ordering::SeqCst))
    }

    /// A notebook with one page of `kind`; returns the page.
    pub(crate) async fn page(&mut self, kind: PageKind, background: Background) -> PageId {
        let notebook = self.ids.notebook();
        let page = self.ids.page();
        let (op_id, hlc) = (self.ids.op(), self.hlc());
        let book = AnyOp::Notebook(NotebookOp {
            op_id,
            device: self.device,
            hlc,
            notebook,
            change: NotebookChange::Create {
                title: "nb".into(),
                position: 0,
            },
        });
        let (op_id, hlc) = (self.ids.op(), self.hlc());
        let leaf = AnyOp::Page(PageOp {
            op_id,
            device: self.device,
            hlc,
            page,
            change: PageChange::Create {
                notebook,
                title: "p".into(),
                position: 0,
                kind,
                background,
            },
        });
        self.storage
            .append_ops(self.device, vec![book, leaf])
            .await
            .expect("page");
        page
    }

    pub(crate) async fn fixed_page(&mut self) -> PageId {
        self.page(
            PageKind::Fixed {
                width: 1000.0,
                height: 1000.0,
            },
            Background::Blank,
        )
        .await
    }

    pub(crate) fn item_id(&mut self) -> ItemId {
        self.next_item += 1;
        ItemRef::from_parts(self.now.load(Ordering::SeqCst), self.next_item).into()
    }

    /// Adds `edit` to `page` as the test device.
    pub(crate) async fn edit(&mut self, page: PageId, edit: Edit) {
        let (op_id, hlc) = (self.ids.op(), self.hlc());
        let op = AnyOp::Item(Box::new(Op {
            op_id,
            device: self.device,
            hlc,
            page,
            edit,
        }));
        self.storage
            .append_ops(self.device, vec![op])
            .await
            .expect("edit");
    }

    /// A pen stroke through `points` on `layer`.
    pub(crate) async fn stroke(
        &mut self,
        page: PageId,
        layer: Layer,
        seed: u64,
        points: &[[f32; 2]],
    ) -> ItemId {
        let id = self.item_id();
        self.edit(page, Edit::Add(stroke_item(id, 1, layer, seed, points)))
            .await;
        id
    }

    pub(crate) async fn view(&self, page: PageId) -> PageView {
        self.storage
            .page_view(page)
            .await
            .expect("read")
            .expect("page")
    }

    /// Stores a real `w × h` PNG filled with one color.
    pub(crate) fn picture(&self, w: u32, h: u32, rgba: [u8; 4]) -> StoredAsset {
        let mut pixmap = Pixmap::new(w, h).expect("pixmap");
        pixmap.fill(tiny_skia::Color::from_rgba8(
            rgba[0], rgba[1], rgba[2], rgba[3],
        ));
        let png = pixmap.encode_png().expect("png");
        self.assets.put(&png).expect("stores")
    }
}

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
