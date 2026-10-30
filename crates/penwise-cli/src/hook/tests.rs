use std::ffi::OsString;
use std::path::Path;

use serde_json::{Value, json};

use super::*;

const PATH: &str = ".claude/settings.json";

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn inbox_hooks(settings: &Value) -> Vec<String> {
    settings["hooks"][EVENT]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .flat_map(|group| group["hooks"].as_array().cloned().unwrap_or_default())
                .filter_map(|hook| hook["command"].as_str().map(str::to_owned))
                .filter(|command| is_ours(command))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_missing_file_gets_just_the_hook() {
    let after = merged(None, Path::new(PATH), "penwise inbox")
        .unwrap()
        .unwrap();
    assert_eq!(
        json!({ "hooks": { "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "penwise inbox" }] }] } }),
        parse(&after)
    );
    assert!(after.ends_with('\n'));
}

#[test]
fn other_keys_and_other_hooks_are_kept() {
    let before = json!({
        "model": "opus",
        "permissions": { "allow": ["Bash(ls)"] },
        "hooks": {
            "Stop": [{ "hooks": [{ "type": "command", "command": "say done" }] }],
            "UserPromptSubmit": [{ "matcher": "", "hooks": [{ "type": "command", "command": "date" }] }]
        }
    })
    .to_string();
    let after = merged(Some(&before), Path::new(PATH), "penwise inbox")
        .unwrap()
        .unwrap();
    let settings = parse(&after);
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["permissions"]["allow"][0], "Bash(ls)");
    assert_eq!(
        settings["hooks"]["Stop"][0]["hooks"][0]["command"],
        "say done"
    );
    let groups = settings["hooks"][EVENT].as_array().unwrap();
    assert_eq!(groups.len(), 2, "the owner's group stays, ours is added");
    assert_eq!(groups[0]["hooks"][0]["command"], "date");
    assert_eq!(inbox_hooks(&settings), ["penwise inbox"]);
}

#[test]
fn merging_twice_adds_nothing() {
    let once = merged(None, Path::new(PATH), "penwise inbox")
        .unwrap()
        .unwrap();
    assert_eq!(
        None,
        merged(Some(&once), Path::new(PATH), "penwise inbox").unwrap()
    );
    // An absolute-path form written by an earlier link counts as the same hook.
    let absolute = merged(
        None,
        Path::new(PATH),
        "\"/home/me/.local/bin/penwise\" inbox",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        None,
        merged(Some(&absolute), Path::new(PATH), "penwise inbox").unwrap()
    );
}

#[test]
fn unexpected_shapes_are_refused_not_overwritten() {
    for bad in [
        "[]",
        r#"{"hooks": []}"#,
        r#"{"hooks": {"UserPromptSubmit": {}}}"#,
    ] {
        assert!(
            merged(Some(bad), Path::new(PATH), "penwise inbox").is_err(),
            "{bad}"
        );
    }
    assert!(merged(Some("{not json"), Path::new(PATH), "penwise inbox").is_err());
}

#[test]
fn removing_takes_out_only_our_hook_and_the_containers_it_emptied() {
    let ours = merged(None, Path::new(PATH), "penwise inbox")
        .unwrap()
        .unwrap();
    assert_eq!(Removal::Empty, removed(&ours, Path::new(PATH)).unwrap());

    let mixed = json!({
        "model": "opus",
        "hooks": { "UserPromptSubmit": [
            { "hooks": [{ "type": "command", "command": "date" }, { "type": "command", "command": "penwise inbox" }] },
            { "hooks": [{ "type": "command", "command": "\"/x/penwise\" inbox" }] }
        ] }
    })
    .to_string();
    let Removal::Rewritten(after) = removed(&mixed, Path::new(PATH)).unwrap() else {
        panic!("rewritten");
    };
    assert_eq!(
        json!({ "model": "opus", "hooks": { "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "date" }] }] } }),
        parse(&after)
    );
    assert_eq!(
        Removal::Nothing,
        removed(
            r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"date"}]}]}}"#,
            Path::new(PATH)
        )
        .unwrap()
    );
    assert_eq!(Removal::Nothing, removed("{}", Path::new(PATH)).unwrap());
}

#[test]
fn our_command_is_recognised_only_when_it_is_penwise_inbox() {
    for yes in [
        "penwise inbox",
        "  penwise inbox ",
        "\"/usr/bin/penwise\" inbox",
        "/a/b/penwise inbox",
        "'penwise' inbox",
    ] {
        assert!(is_ours(yes), "{yes}");
    }
    for no in [
        "penwise status",
        "penwise inbox --all",
        "mypenwise inbox",
        "echo penwise inbox",
        "inbox",
    ] {
        assert!(!is_ours(no), "{no}");
    }
}

#[test]
fn the_command_is_portable_when_penwise_is_on_the_path_and_absolute_otherwise() {
    let dir = std::env::temp_dir().join(format!("penwise-hook-path-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = Path::new("/opt/penwise/bin/penwise");
    let without = OsString::from(dir.as_os_str());
    assert_eq!(
        "\"/opt/penwise/bin/penwise\" inbox",
        command_for(Some(&without), Some(exe)),
        "not on PATH: the running binary's own path, quoted"
    );
    std::fs::write(dir.join("penwise"), "").unwrap();
    assert_eq!("penwise inbox", command_for(Some(&without), Some(exe)));
    assert_eq!("penwise inbox", command_for(None, None));
    std::fs::remove_dir_all(&dir).unwrap();
}
