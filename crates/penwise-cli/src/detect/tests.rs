use std::path::{Path, PathBuf};

use super::dev::{framework_in_script, port_in_script, port_in_text};
use super::*;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn detected(name: &str) -> Detected {
    detect(&fixture(name)).unwrap()
}

fn static_in(name: &str, folder: &str) -> Detected {
    let root = fixture(name).canonicalize().unwrap();
    let folder = if folder.is_empty() {
        root.clone()
    } else {
        root.join(folder)
    };
    Detected {
        root,
        name: name.to_owned(),
        kind: Kind::Static { folder },
        framework: None,
    }
}

fn dev_at(name: &str, port: u16, framework: Option<&'static str>) -> Detected {
    Detected {
        root: fixture(name).canonicalize().unwrap(),
        name: name.to_owned(),
        kind: Kind::Dev {
            origin: format!("http://127.0.0.1:{port}"),
        },
        framework,
    }
}

#[test]
fn an_index_at_the_root_is_a_static_site() {
    assert_eq!(static_in("static-root", ""), detected("static-root"));
}

#[test]
fn a_site_or_output_subfolder_is_a_static_site() {
    assert_eq!(static_in("site-subdir", "site"), detected("site-subdir"));
    assert_eq!(static_in("dist-subdir", "dist"), detected("dist-subdir"));
}

#[test]
fn vite_uses_its_default_port() {
    assert_eq!(
        dev_at("vite-default", 5173, Some("vite")),
        detected("vite-default")
    );
}

#[test]
fn the_port_in_the_config_wins_over_the_default() {
    assert_eq!(
        dev_at("vite-config-port", 5181, Some("vite")),
        detected("vite-config-port")
    );
}

#[test]
fn the_port_in_the_dev_script_wins_over_the_config() {
    assert_eq!(
        dev_at("vite-script-port", 5200, Some("vite")),
        detected("vite-script-port")
    );
}

#[test]
fn next_and_astro_use_their_default_ports() {
    assert_eq!(dev_at("next", 3000, Some("next")), detected("next"));
    assert_eq!(dev_at("astro", 4321, Some("astro")), detected("astro"));
}

#[test]
fn sveltekit_wins_over_the_vite_config_it_also_has() {
    assert_eq!(
        dev_at("sveltekit", 5173, Some("sveltekit")),
        detected("sveltekit")
    );
}

#[test]
fn the_dev_script_names_the_framework_when_nothing_local_does() {
    assert_eq!(
        dev_at("next-script-only", 3000, Some("next")),
        detected("next-script-only")
    );
}

#[test]
fn a_plain_dev_script_is_a_dev_server_with_its_own_port() {
    assert_eq!(dev_at("generic-dev", 8090, None), detected("generic-dev"));
}

#[test]
fn a_broken_package_json_falls_back_to_the_static_site() {
    assert_eq!(static_in("broken-package", ""), detected("broken-package"));
}

#[test]
fn an_empty_folder_names_what_was_looked_for() {
    let error = detect(&fixture("unknown")).unwrap_err().to_string();
    assert!(error.contains("index.html"), "{error}");
    assert!(error.contains("site, public, dist"), "{error}");
}

#[test]
fn a_missing_folder_is_reported() {
    assert!(detect(&fixture("no-such-fixture")).is_err());
}

#[test]
fn script_ports_cover_the_forms_dev_servers_use() {
    assert_eq!(Some(4000), port_in_script("next dev -p 4000"));
    assert_eq!(Some(4000), port_in_script("vite --port=4000"));
    assert_eq!(Some(4000), port_in_script("PORT=4000 node server.js"));
    assert_eq!(None, port_in_script("vite"));
    assert_eq!(None, port_in_script("vite --host"));
}

#[test]
fn the_dev_command_is_read_as_a_word() {
    assert_eq!(
        Some("next"),
        framework_in_script("next dev").map(|f| f.name)
    );
    assert_eq!(
        Some("astro"),
        framework_in_script("astro dev --port 1").map(|f| f.name)
    );
    assert_eq!(None, framework_in_script("node server.js").map(|f| f.name));
}

#[test]
fn a_port_in_a_config_is_only_read_after_the_word() {
    assert_eq!(Some(5181), port_in_text("server: { port: 5181 }"));
    assert_eq!(Some(4321), port_in_text("export default { port = 4321 }"));
    // `import` and `export` contain "port"; neither is a setting.
    assert_eq!(None, port_in_text("import { defineConfig } from \"vite\";"));
}
