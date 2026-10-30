use std::fs;

use super::{find_link, segment};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("penwise-inbox-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_link_is_found_in_a_parent_folder() {
    let root = scratch("ancestor");
    fs::write(
        root.join(".penwise.json"),
        r#"{"project":"prj_site","name":"site"}"#,
    )
    .unwrap();
    let deep = root.join("src/components");
    fs::create_dir_all(&deep).unwrap();
    assert_eq!(Some("prj_site".to_owned()), find_link(&deep));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_project_id_is_one_path_segment() {
    assert_eq!("prj_site", segment("prj_site"));
    assert_eq!("a%2Fb%20c", segment("a/b c"));
}
