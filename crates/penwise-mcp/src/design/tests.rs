//! Unit tests for the design-harness argument encoding. The end-to-end behavior is in `tests/design.rs`.

use serde_json::json;

use super::{Images, images_option, marks_path};

#[test]
fn the_default_check_names_an_agent_and_asks_for_the_last_checkpoint() {
    assert_eq!(
        "/marks?since=last&agent=agent",
        marks_path(&json!({}), None)
    );
}

#[test]
fn filters_are_encoded_as_one_query_value_each() {
    let path = marks_path(
        &json!({
            "project": "prj_site",
            "page": "pg 1",
            "frame": "fr_1",
            "status": "new",
            "since": "none",
            "agent": "omp"
        }),
        None,
    );
    assert_eq!(
        "/marks?project=prj_site&page=pg%201&frame=fr_1&status=new&agent=omp",
        path
    );
}

#[test]
fn an_empty_status_or_agent_falls_back_to_the_defaults() {
    assert_eq!(
        "/marks?since=last&agent=agent",
        marks_path(&json!({ "project": "", "agent": "" }), None)
    );
}

#[test]
fn the_configured_project_is_the_default_filter() {
    assert_eq!(
        "/marks?project=prj_site&since=last&agent=agent",
        marks_path(&json!({}), Some("prj_site"))
    );
    // An explicit argument wins over the configured default.
    assert_eq!(
        "/marks?project=prj_other&since=last&agent=agent",
        marks_path(&json!({ "project": "prj_other" }), Some("prj_site"))
    );
    assert_eq!(
        "/marks?project=prj_site&since=last&agent=agent",
        marks_path(&json!({ "project": "" }), Some("prj_site"))
    );
}

#[test]
fn image_options_are_the_three_documented_values() {
    assert!(matches!(
        images_option(&json!({ "images": "none" })),
        Ok(Images::None)
    ));
    assert!(matches!(
        images_option(&json!({ "images": "with_ink" })),
        Ok(Images::WithInk)
    ));
    assert!(matches!(
        images_option(&json!({ "images": "all" })),
        Ok(Images::All)
    ));
    assert!(matches!(images_option(&json!({})), Ok(Images::WithInk)));
    assert!(images_option(&json!({ "images": "all " })).is_err());
}
