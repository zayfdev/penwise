//! Real-socket tests for the project registry: allocation, live start/stop, idempotent relink and ports that
//! survive a restart. Every test picks its own free base port; the production 7421+ range is never bound.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{LinkRequest, Project, Registry};
use crate::testing::Rig;

const INDEX: &str = "<!doctype html><html><head></head><body><h1>portfolio</h1></body></html>";

fn site(dir: &Path, name: &str) -> PathBuf {
    let root = dir.join(name);
    std::fs::create_dir_all(&root).expect("site");
    std::fs::write(root.join("index.html"), INDEX).expect("index");
    root
}

/// A free base port from the OS (always above the production range).
async fn free_base() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("probe");
    let port = listener.local_addr().expect("probe addr").port();
    assert!(port >= 17_421, "ephemeral port below the test range");
    port
}

fn registry(rig: &Rig, base: u16) -> Registry {
    Registry::new(
        rig.storage.clone(),
        Arc::new(rig.dir.path().to_path_buf()),
        Arc::new("test-token".to_owned()),
        "127.0.0.1".to_owned(),
        base,
    )
}

async fn link(registry: &Registry, root: &Path) -> Project {
    registry
        .link(LinkRequest {
            name: Some("portfolio".to_owned()),
            root: root.to_path_buf(),
            folder: Some(root.to_path_buf()),
            ..LinkRequest::default()
        })
        .await
        .expect("link")
}

async fn body(port: u16) -> String {
    reqwest::get(format!("http://127.0.0.1:{port}/"))
        .await
        .expect("fetch")
        .error_for_status()
        .expect("status")
        .text()
        .await
        .expect("body")
}

#[tokio::test]
async fn link_starts_a_static_port_and_lists_the_project() {
    let rig = Rig::new();
    let root = site(rig.dir.path(), "portfolio");
    let base = free_base().await;
    let projects = registry(&rig, base);
    let project = link(&projects, &root).await;

    assert_eq!(project.port, base, "a free base port is used as-is");
    assert_eq!(project.kind(), "static");
    assert_eq!(project.static_dir(), Some(root.as_path()));
    assert_eq!(project.origin(), None);
    assert!(body(project.port).await.contains("portfolio"));
    assert_eq!(projects.list().await.unwrap(), vec![project.clone()]);
    assert_eq!(
        projects.get(&project.id).await.unwrap(),
        Some(project.clone())
    );
    assert!(projects.get("prj_missing").await.unwrap().is_none());

    projects.shutdown().await;
}

#[tokio::test]
async fn relink_by_root_is_idempotent_and_updates_the_name() {
    let rig = Rig::new();
    let root = site(rig.dir.path(), "portfolio");
    let base = free_base().await;
    let projects = registry(&rig, base);
    let first = link(&projects, &root).await;
    let again = projects
        .link(LinkRequest {
            name: Some("renamed".to_owned()),
            root: root.clone(),
            folder: Some(root.clone()),
            ..LinkRequest::default()
        })
        .await
        .expect("re-link");

    assert_eq!(again.id, first.id);
    assert_eq!(again.port, first.port);
    assert_eq!(again.name, "renamed");
    assert_eq!(projects.list().await.unwrap().len(), 1);
    assert!(body(again.port).await.contains("portfolio"));

    projects.shutdown().await;
}

#[tokio::test]
async fn a_busy_port_is_skipped_and_remove_stops_the_listener() {
    let rig = Rig::new();
    let root = site(rig.dir.path(), "portfolio");
    let held = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("hold");
    let base = held.local_addr().expect("addr").port();
    assert!(base >= 17_421, "ephemeral port below the test range");
    let projects = registry(&rig, base);
    let project = link(&projects, &root).await;
    assert!(project.port > base, "the held base port cannot be used");
    drop(held);

    assert!(projects.remove(&project.id).await.unwrap());
    assert!(projects.list().await.unwrap().is_empty());
    assert!(
        reqwest::get(format!("http://127.0.0.1:{}/", project.port))
            .await
            .is_err(),
        "the port stops listening"
    );
    assert!(!projects.remove(&project.id).await.unwrap());

    projects.shutdown().await;
}

#[tokio::test]
async fn a_restart_reuses_the_persisted_port() {
    let rig = Rig::new();
    let root = site(rig.dir.path(), "portfolio");
    let base = free_base().await;
    let first = registry(&rig, base);
    let project = link(&first, &root).await;
    first.shutdown().await;
    drop(first);

    let second = registry(&rig, base);
    second.start_all().await;
    let listed = second.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, project.id);
    assert_eq!(listed[0].port, project.port);
    assert!(body(project.port).await.contains("portfolio"));

    second.shutdown().await;
}

#[tokio::test]
async fn concurrent_links_of_one_root_make_one_project_and_one_notebook() {
    let rig = Rig::new();
    let root = site(rig.dir.path(), "portfolio");
    let projects = registry(&rig, free_base().await);
    let request = LinkRequest {
        name: Some("portfolio".to_owned()),
        root: root.clone(),
        folder: Some(root),
        ..LinkRequest::default()
    };
    let (first, second) = tokio::join!(projects.link(request.clone()), projects.link(request));
    let (first, second) = (first.expect("first link"), second.expect("second link"));
    assert_eq!(first.id, second.id);
    assert_eq!(first.port, second.port);
    assert_eq!(first.notebook, second.notebook);
    assert!(first.notebook.is_some());
    assert_eq!(projects.list().await.unwrap().len(), 1);
    assert_eq!(rig.storage.catalog().await.unwrap().notebooks().len(), 1);

    projects.shutdown().await;
}

#[tokio::test]
async fn a_folder_that_is_not_a_directory_is_refused() {
    let rig = Rig::new();
    let file = rig.dir.path().join("index.html");
    std::fs::write(&file, INDEX).expect("file");
    let projects = registry(&rig, free_base().await);
    let error = projects
        .link(LinkRequest {
            name: Some("bad".to_owned()),
            root: rig.dir.path().to_path_buf(),
            folder: Some(file),
            ..LinkRequest::default()
        })
        .await
        .expect_err("a file is not a static folder");
    assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
    assert!(projects.list().await.unwrap().is_empty());
}
