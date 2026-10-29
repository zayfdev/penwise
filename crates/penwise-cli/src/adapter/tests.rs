use super::*;

fn vite_import() -> &'static str {
    "import penwise from '/repo/adapters/vite/index.js';"
}

fn next_import() -> &'static str {
    "const { withPenwise } = require('/repo/adapters/next/index.cjs');"
}

#[test]
fn a_vite_plugins_array_gets_penwise_first() {
    let config = "import { defineConfig } from 'vite';\nimport react from '@vitejs/plugin-react';\n\nexport default defineConfig({\n  plugins: [react()],\n});\n";
    let edited = vite_edit(config, vite_import()).expect("recognised");
    assert_eq!(
        edited,
        "import penwise from '/repo/adapters/vite/index.js';\n\
         import { defineConfig } from 'vite';\n\
         import react from '@vitejs/plugin-react';\n\
         \n\
         export default defineConfig({\n\
         \x20 plugins: [penwise(), react()],\n\
         });\n"
    );
}

#[test]
fn a_multiline_vite_plugins_array_keeps_its_layout() {
    let config = "export default defineConfig({\n  plugins: [\n    react(),\n  ],\n});\n";
    let edited = vite_edit(config, vite_import()).expect("recognised");
    assert_eq!(
        edited,
        "import penwise from '/repo/adapters/vite/index.js';\n\
         export default defineConfig({\n\
         \x20 plugins: [\n\
         \x20   penwise(),\n\
         \x20   react(),\n\
         \x20 ],\n\
         });\n"
    );
}

#[test]
fn an_empty_plugins_array_gets_one_item_without_a_dangling_comma() {
    let config = "export default defineConfig({\n  plugins: [],\n});\n";
    let edited = vite_edit(config, vite_import()).expect("recognised");
    assert!(edited.contains("plugins: [penwise()],"), "{edited}");
}

#[test]
fn a_vite_config_without_plugins_gains_the_key() {
    let config = "import { defineConfig } from 'vite';\n\nexport default defineConfig({\n  server: { port: 5173 },\n});\n";
    let edited = vite_edit(config, vite_import()).expect("recognised");
    assert!(
        edited.contains(
            "export default defineConfig({\n  plugins: [penwise()],\n  server: { port: 5173 },"
        ),
        "{edited}"
    );
}

#[test]
fn a_vite_config_without_plugins_gains_the_key_in_a_plain_object_export() {
    let config = "export default {\n  base: '/site/',\n};\n";
    let edited = vite_edit(config, vite_import()).expect("recognised");
    assert!(
        edited.contains("export default {\n  plugins: [penwise()],\n  base: '/site/',"),
        "{edited}"
    );
}

#[test]
fn an_unrecognised_vite_config_is_left_alone() {
    let config = "export default defineConfig(({ mode }) => build(mode));\n";
    assert_eq!(vite_edit(config, vite_import()), None);
    assert_eq!(vite_edit("justSomeCall();\n", vite_import()), None);
}

#[test]
fn a_next_config_identifier_export_is_wrapped() {
    let config = "/** @type {import('next').NextConfig} */\nconst nextConfig = { reactStrictMode: true };\n\nmodule.exports = nextConfig;\n";
    let edited = next_edit(config, next_import()).expect("recognised");
    assert!(
        edited.contains("module.exports = withPenwise(nextConfig);"),
        "{edited}"
    );
    assert!(
        edited.starts_with("const { withPenwise } = require('/repo/adapters/next/index.cjs');\n"),
        "{edited}"
    );
}

#[test]
fn a_next_object_literal_export_is_wrapped_with_its_braces() {
    let config = "export default {\n  reactStrictMode: true,\n};\n";
    let edited = next_edit(
        config,
        "import { withPenwise } from '/repo/adapters/next/index.mjs';",
    )
    .expect("recognised");
    assert!(
        edited.contains("export default withPenwise({\n  reactStrictMode: true,\n});"),
        "{edited}"
    );
}

#[test]
fn a_next_export_that_is_already_wrapped_is_not_wrapped_again() {
    let config = "const nextConfig = {};\nmodule.exports = withPenwise(nextConfig);\n";
    assert_eq!(next_edit(config, next_import()), None);
}

#[test]
fn an_unreadable_next_config_is_left_alone() {
    assert_eq!(
        next_edit("export const config = {};\n", next_import()),
        None
    );
    assert_eq!(next_edit("", next_import()), None);
}

#[test]
fn the_dev_dependency_is_added_to_an_existing_map() {
    let package =
        "{\n  \"name\": \"site\",\n  \"devDependencies\": {\n    \"vite\": \"^7.0.0\"\n  }\n}\n";
    let edited = dev_dependency(package, "@penwise/vite-plugin", "file:/repo/adapters/vite")
        .expect("edited");
    assert!(
        edited.contains("\"devDependencies\": {\n    \"@penwise/vite-plugin\": \"file:/repo/adapters/vite\",\n    \"vite\": \"^7.0.0\""),
        "{edited}"
    );
    assert!(serde_json::from_str::<serde_json::Value>(&edited).is_ok());
}

#[test]
fn the_dev_dependency_map_is_created_when_missing() {
    let package =
        "{\n  \"name\": \"site\",\n  \"dependencies\": {\n    \"react\": \"^19.0.0\"\n  }\n}\n";
    let edited =
        dev_dependency(package, "@penwise/next", "file:/repo/adapters/next").expect("edited");
    assert!(
        edited.contains(
            "\"devDependencies\": {\n    \"@penwise/next\": \"file:/repo/adapters/next\"\n  },"
        ),
        "{edited}"
    );
    assert!(serde_json::from_str::<serde_json::Value>(&edited).is_ok());
}

#[test]
fn an_empty_package_json_gains_the_map_without_a_trailing_comma() {
    let edited =
        dev_dependency("{}", "@penwise/vite-plugin", "file:/repo/adapters/vite").expect("edited");
    assert!(serde_json::from_str::<serde_json::Value>(&edited).is_ok());
    assert!(edited.contains("\"@penwise/vite-plugin\""));
}

#[test]
fn a_broken_package_json_is_not_edited() {
    assert_eq!(
        dev_dependency("{ nope", "@penwise/vite-plugin", "file:/x"),
        None
    );
    assert_eq!(
        dev_dependency("[1, 2]", "@penwise/vite-plugin", "file:/x"),
        None
    );
}

/// A temp folder removed when the test ends.
struct Temp(PathBuf);

impl Temp {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("penwise-adapter-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn plan_in(folder: &Path, kind: Kind, config: &str, module: &str) -> Plan {
    Plan {
        kind,
        config: folder.join(config),
        module: PathBuf::from(module),
        package_dir: PathBuf::from("/repo/adapters/vite"),
    }
}

#[test]
fn applying_the_plan_writes_both_files_and_prepare_is_idempotent() {
    let temp = Temp::new("apply");
    let root = &temp.0;
    std::fs::write(
        root.join("vite.config.ts"),
        "export default defineConfig({\n  plugins: [react()],\n});\n",
    )
    .expect("config");
    std::fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"site\",\n  \"devDependencies\": {\n    \"vite\": \"^7.0.0\"\n  }\n}\n",
    )
    .expect("package");
    let plan = Plan {
        kind: Kind::Vite,
        config: root.join("vite.config.ts"),
        module: root.join("adapters/vite/index.js"),
        package_dir: root.join("adapters/vite"),
    };
    let Prepared::Ready { config, package } = plan.prepare().expect("prepared") else {
        panic!("expected a ready plan");
    };
    assert!(config.diff().contains("plugins: [penwise(), react()]"));
    Plan::apply(&config, package.as_ref()).expect("apply");
    let written = std::fs::read_to_string(&plan.config).expect("config");
    assert!(
        written.contains("plugins: [penwise(), react()]"),
        "{written}"
    );
    let package = std::fs::read_to_string(root.join("package.json")).expect("package");
    assert!(
        package.contains("\"@penwise/vite-plugin\": \"file:"),
        "{package}"
    );

    // A second run sees the import and leaves the files alone.
    assert!(matches!(
        plan.prepare().expect("prepared"),
        Prepared::Installed
    ));
}

#[test]
fn an_unrecognised_config_returns_the_manual_instructions() {
    let temp = Temp::new("manual");
    std::fs::write(temp.0.join("vite.config.ts"), "export default build();\n").expect("config");
    let plan = plan_in(
        &temp.0,
        Kind::Vite,
        "vite.config.ts",
        "/repo/adapters/vite/index.js",
    );
    let Prepared::Manual(text) = plan.prepare().expect("prepared") else {
        panic!("expected manual instructions");
    };
    assert!(text.contains("add the adapter by hand"), "{text}");
    assert!(text.contains("@penwise/vite-plugin"), "{text}");
    assert_eq!(
        std::fs::read_to_string(&plan.config).expect("config"),
        "export default build();\n"
    );
}

#[test]
fn the_repo_adapters_are_found_for_vite_and_next_projects() {
    let temp = Temp::new("real-adapters");
    std::fs::write(
        temp.0.join("vite.config.ts"),
        "export default defineConfig({});\n",
    )
    .expect("vite");
    std::fs::write(temp.0.join("next.config.js"), "module.exports = {};\n").expect("next");
    let vite = plan(&temp.0, Some("vite")).expect("the checkout has adapters/vite");
    assert_eq!(vite.kind, Kind::Vite);
    // SvelteKit carries a Vite config too, so it gets the same adapter.
    assert!(plan(&temp.0, Some("sveltekit")).is_some());
    let next = plan(&temp.0, Some("next")).expect("the checkout has adapters/next");
    assert_eq!(next.kind, Kind::Next);
    // Astro has no adapter.
    assert!(plan(&temp.0, Some("astro")).is_none());
    assert!(plan(&temp.0, None).is_none());
    assert!(matches!(
        vite.prepare().expect("prepared"),
        Prepared::Ready { .. }
    ));
}

#[test]
fn the_next_import_form_follows_the_config_type() {
    let temp = Temp::new("next-import");
    let common = Plan {
        kind: Kind::Next,
        config: temp.0.join("next.config.js"),
        module: PathBuf::from("/repo/adapters/next/index.cjs"),
        package_dir: PathBuf::from("/repo/adapters/next"),
    };
    assert!(
        common
            .next_import()
            .contains("require('/repo/adapters/next/index.cjs')")
    );
    let module = Plan {
        config: temp.0.join("next.config.mjs"),
        module: PathBuf::from("/repo/adapters/next/index.mjs"),
        ..common
    };
    assert!(
        module
            .next_import()
            .contains("import { withPenwise } from '/repo/adapters/next/index.mjs'")
    );
}
