use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;

/// A temp folder removed when the test ends.
struct Temp(PathBuf);

impl Temp {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "penwise-mcp-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn parse(path: &Path, project: &str, schema: bool) -> Value {
    let text = merged(path, project, schema).expect("merged");
    serde_json::from_str(&text).expect("valid json")
}

#[test]
fn the_claude_file_keeps_other_servers_and_other_keys() {
    let temp = Temp::new("claude-merge");
    let path = temp.path().join(".mcp.json");
    std::fs::write(
        &path,
        "{\n  \"mcpServers\": {\n    \"other\": { \"command\": \"other-mcp\" }\n  },\n  \"note\": \"mine\"\n}\n",
    )
    .expect("existing config");
    let root = parse(&path, "prj_site", false);
    let servers = &root["mcpServers"];
    assert_eq!(servers["other"]["command"], "other-mcp");
    assert_eq!(root["note"], "mine");
    let penwise = &servers[SERVER];
    assert!(
        penwise["command"]
            .as_str()
            .expect("command")
            .ends_with(".local/bin/penwise-mcp"),
        "{penwise}"
    );
    assert_eq!(penwise["env"]["PENWISE_PORT"], "7420");
    assert_eq!(penwise["env"]["PENWISE_PROJECT"], "prj_site");
    // Claude Code's file carries no omp schema.
    assert!(root.get("$schema").is_none());
}

#[test]
fn the_omp_file_gets_the_schema_and_the_same_entry() {
    let temp = Temp::new("omp-new");
    let targets = Targets::of(temp.path());
    assert_eq!(targets.claude, temp.path().join(".mcp.json"));
    assert_eq!(targets.omp, temp.path().join(".omp/mcp.json"));
    let root = parse(&targets.omp, "prj_site", true);
    assert!(
        root["$schema"]
            .as_str()
            .is_some_and(|url| url.ends_with("mcp-schema.json"))
    );
    assert_eq!(
        root["mcpServers"][SERVER]["env"]["PENWISE_PROJECT"],
        "prj_site"
    );
}

#[test]
fn a_previous_penwise_entry_is_updated_not_duplicated() {
    let temp = Temp::new("update");
    let path = temp.path().join(".mcp.json");
    std::fs::write(
        &path,
        "{\"mcpServers\": {\"penwise\": {\"command\": \"/old/penwise-mcp\", \"env\": {\"PENWISE_PROJECT\": \"old\"}}}}",
    )
    .expect("existing config");
    let root = parse(&path, "prj_new", false);
    let servers = root["mcpServers"].as_object().expect("servers");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[SERVER]["env"]["PENWISE_PROJECT"], "prj_new");
}

#[test]
fn write_fills_both_files() {
    let temp = Temp::new("write");
    std::fs::write(
        temp.path().join(".mcp.json"),
        "{\"mcpServers\": {\"other\": {}}}",
    )
    .expect("existing config");
    let written = write(temp.path(), "prj_site").expect("written");
    assert_eq!(written.len(), 2);
    assert!(written.iter().all(|(_, path)| path.is_file()));
    let claude: Value = serde_json::from_str(
        &std::fs::read_to_string(temp.path().join(".mcp.json")).expect("read"),
    )
    .expect("json");
    assert!(claude["mcpServers"]["other"].is_object());
    let omp: Value = serde_json::from_str(
        &std::fs::read_to_string(temp.path().join(".omp/mcp.json")).expect("read"),
    )
    .expect("json");
    assert_eq!(
        omp["mcpServers"][SERVER]["env"]["PENWISE_PROJECT"],
        "prj_site"
    );
}

#[test]
fn a_broken_config_is_reported_and_left_unchanged() {
    let temp = Temp::new("broken");
    let path = temp.path().join(".mcp.json");
    std::fs::write(&path, "{ not json").expect("broken config");
    assert!(merged(&path, "prj", false).is_err());
    assert!(write(temp.path(), "prj").is_err());
    assert_eq!(
        std::fs::read_to_string(&path).expect("still there"),
        "{ not json"
    );
}
