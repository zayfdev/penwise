//! The dev-server half of detection: which framework a folder is, and which port its `dev` script listens on.
//!
//! Everything here is a hint. A framework config file is the strongest signal, then a dependency in
//! `package.json`, then the command the `dev` script runs (a monorepo can install the dependency at its root).
//! The port comes from the script, then from the framework's config file, then from its default.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// A framework whose default dev port is known.
pub(super) struct Framework {
    pub(super) name: &'static str,
    /// The word its `dev` script starts with (`next dev`).
    command: &'static str,
    pub(super) default_port: u16,
    configs: &'static [&'static str],
    package: &'static str,
}

/// Checked in this order: `SvelteKit` also carries a `vite.config.*`, so it has to come first.
const FRAMEWORKS: &[Framework] = &[
    Framework {
        name: "sveltekit",
        command: "svelte-kit",
        default_port: 5173,
        configs: &["svelte.config.js", "svelte.config.ts"],
        package: "@sveltejs/kit",
    },
    Framework {
        name: "next",
        command: "next",
        default_port: 3000,
        configs: &[
            "next.config.js",
            "next.config.mjs",
            "next.config.ts",
            "next.config.cjs",
        ],
        package: "next",
    },
    Framework {
        name: "astro",
        command: "astro",
        default_port: 4321,
        configs: &[
            "astro.config.js",
            "astro.config.mjs",
            "astro.config.ts",
            "astro.config.mts",
        ],
        package: "astro",
    },
    Framework {
        name: "vite",
        command: "vite",
        default_port: 5173,
        configs: &[
            "vite.config.js",
            "vite.config.ts",
            "vite.config.mjs",
            "vite.config.mts",
            "vite.config.cjs",
            "vite.config.cts",
        ],
        package: "vite",
    },
];

/// The first framework with a config file in `root`, else the first one a dependency names.
pub(super) fn framework_of(root: &Path, package: Option<&Package>) -> Option<&'static Framework> {
    FRAMEWORKS.iter().find(|framework| {
        framework
            .configs
            .iter()
            .any(|config| root.join(config).is_file())
            || package.is_some_and(|package| package.depends_on(framework.package))
    })
}

/// The framework a `dev` script names, for a project whose dependencies are installed elsewhere.
pub(super) fn framework_in_script(script: &str) -> Option<&'static Framework> {
    script.split_whitespace().find_map(|word| {
        let word = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
        FRAMEWORKS
            .iter()
            .find(|framework| framework.command == word)
    })
}

/// The port `dev` asks for: `--port N`, `-p N`, `PORT=N`, or the `=` forms; quotes around a port are allowed.
pub(super) fn port_in_script(script: &str) -> Option<u16> {
    let mut words = script.split_whitespace();
    while let Some(word) = words.next() {
        let inline = ["PORT=", "--port=", "-p="]
            .iter()
            .find_map(|prefix| word.strip_prefix(prefix));
        if let Some(value) = inline {
            if let Some(port) = port_of(value) {
                return Some(port);
            }
        } else if matches!(word, "--port" | "-p")
            && let Some(port) = words.next().and_then(port_of)
        {
            return Some(port);
        }
    }
    None
}

fn port_of(value: &str) -> Option<u16> {
    value
        .trim_matches(|c| c == '"' || c == '\'')
        .parse()
        .ok()
        .filter(|port: &u16| *port > 0)
}

/// The first `port: N` / `port = N` in the framework's config file.
pub(super) fn port_in_config(root: &Path, framework: &Framework) -> Option<u16> {
    let text = framework.configs.iter().find_map(|config| {
        let path = root.join(config);
        path.is_file()
            .then(|| std::fs::read_to_string(path).ok())
            .flatten()
    })?;
    port_in_text(&text)
}

pub(super) fn port_in_text(text: &str) -> Option<u16> {
    let mut rest = text;
    while let Some(at) = rest.find("port") {
        rest = &rest[at + "port".len()..];
        let tail = rest.trim_start();
        let Some(tail) = tail.strip_prefix(':').or_else(|| tail.strip_prefix('=')) else {
            continue;
        };
        let digits: String = tail
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Some(port) = digits.parse().ok().filter(|port: &u16| *port > 0) {
            return Some(port);
        }
    }
    None
}

/// The parts of `package.json` detection reads. Anything else (and a broken file) is ignored.
#[derive(Debug, Default, Deserialize)]
pub(super) struct Package {
    #[serde(default)]
    scripts: Scripts,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(default)]
    dev_dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct Scripts {
    #[serde(default)]
    dev: Option<String>,
}

impl Package {
    pub(super) fn dev_script(&self) -> Option<&str> {
        self.scripts.dev.as_deref()
    }

    fn depends_on(&self, name: &str) -> bool {
        self.dependencies.contains_key(name) || self.dev_dependencies.contains_key(name)
    }
}

pub(super) fn read_package(root: &Path) -> Option<Package> {
    let text = std::fs::read_to_string(root.join("package.json")).ok()?;
    serde_json::from_str(&text).ok()
}
