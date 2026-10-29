//! The adapter offer in `penwise link` (design-harness spec §7.3, §8 step 2).
//!
//! A Vite or Next project gets `data-pw-src="<file>:<line>:<col>"` on host elements from a build-time plugin.
//! This module finds the config file, computes the exact text edit (import plus plugin) and the
//! `devDependencies` entry, and applies both only after the owner confirmed the diff. A config shape it does not
//! recognise is never guessed at: the caller prints manual instructions instead.

use std::path::{Path, PathBuf};

use crate::diff;
use crate::error::Error;

/// The adapters' repository folder: `PENWISE_ADAPTERS`, else `adapters/` near the executable (a source build).
pub const ADAPTERS_ENV: &str = "PENWISE_ADAPTERS";

/// Which framework's adapter to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Vite,
    Next,
}

impl Kind {
    fn of(framework: &str) -> Option<Self> {
        match framework {
            // SvelteKit builds on Vite; the plugin sees the Svelte compiler's output.
            "vite" | "sveltekit" => Some(Self::Vite),
            "next" => Some(Self::Next),
            _ => None,
        }
    }

    /// The name `penwise link` prints and `.penwise.json` records.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vite => "vite",
            Self::Next => "next",
        }
    }

    fn dev_dependency(self) -> &'static str {
        match self {
            Self::Vite => "@penwise/vite-plugin",
            Self::Next => "@penwise/next",
        }
    }

    fn folder(self) -> &'static str {
        self.as_str()
    }
}

/// What to install: the config file, the adapter module to import, and the npm package.
#[derive(Debug, Clone)]
pub struct Plan {
    pub kind: Kind,
    pub config: PathBuf,
    module: PathBuf,
    package_dir: PathBuf,
}

/// One file edit: its old and new text.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

impl FileChange {
    /// The unified diff, for the confirmation prompt.
    #[must_use]
    pub fn diff(&self) -> String {
        diff::unified(&self.path, &self.before, &self.after)
    }

    fn write(&self) -> Result<(), Error> {
        std::fs::write(&self.path, &self.after)?;
        Ok(())
    }
}

/// The result of reading the project's config.
#[derive(Debug, Clone)]
pub enum Prepared {
    /// The edits, ready to apply after confirmation.
    Ready {
        config: FileChange,
        package: Option<FileChange>,
    },
    /// The adapter is already imported by the config.
    Installed,
    /// Unrecognised shape: the caller prints these instructions and changes nothing.
    Manual(String),
}

/// Finds the adapter plan for `framework` in `root`, or `None` when this is not a Vite/Next project, the
/// framework's config file is absent, or the adapters are not installed next to this binary.
#[must_use]
pub fn plan(root: &Path, framework: Option<&str>) -> Option<Plan> {
    let kind = Kind::of(framework?)?;
    let config = config_file(root, kind)?;
    let base = adapters_base()?;
    let package_dir = base.join(kind.folder());
    let module = package_dir.join(module_name(kind, &config)?);
    module.is_file().then_some(Plan {
        kind,
        config,
        module,
        package_dir,
    })
}

impl Plan {
    /// Reads the config and computes both edits, or explains how to add the adapter by hand.
    ///
    /// # Errors
    /// The config or `package.json` cannot be read.
    pub fn prepare(&self) -> Result<Prepared, Error> {
        let before = std::fs::read_to_string(&self.config)?;
        let module = self.module.to_string_lossy();
        if before.contains(module.as_ref()) {
            return Ok(Prepared::Installed);
        }
        let after = match self.kind {
            Kind::Vite => vite_edit(&before, &self.vite_import()),
            Kind::Next => next_edit(&before, &self.next_import()),
        };
        let Some(after) = after else {
            return Ok(Prepared::Manual(self.instructions()));
        };
        let package = self.package_change()?;
        Ok(Prepared::Ready {
            config: FileChange {
                path: self.config.clone(),
                before,
                after,
            },
            package,
        })
    }

    /// Writes the confirmed edits.
    ///
    /// # Errors
    /// A file is not writable.
    pub fn apply(config: &FileChange, package: Option<&FileChange>) -> Result<(), Error> {
        if let Some(package) = package {
            package.write()?;
        }
        config.write()
    }

    fn import_line(&self, statement: &str) -> String {
        let module = self.module.to_string_lossy();
        statement.replace("MODULE", &module)
    }

    fn vite_import(&self) -> String {
        self.import_line("import penwise from 'MODULE';")
    }

    fn next_import(&self) -> String {
        if self
            .config
            .extension()
            .is_some_and(|extension| extension == "js")
        {
            self.import_line("const { withPenwise } = require('MODULE');")
        } else {
            self.import_line("import { withPenwise } from 'MODULE';")
        }
    }

    fn package_change(&self) -> Result<Option<FileChange>, Error> {
        let path = self
            .config
            .parent()
            .unwrap_or(Path::new("."))
            .join("package.json");
        if !path.is_file() {
            return Ok(None);
        }
        let before = std::fs::read_to_string(&path)?;
        let value = format!("file:{}", self.package_dir.display());
        let Some(after) = dev_dependency(&before, self.kind.dev_dependency(), &value) else {
            return Ok(None);
        };
        Ok(Some(FileChange {
            path,
            before,
            after,
        }))
    }

    /// The manual instructions for a config shape the text edit does not recognise.
    #[must_use]
    pub fn instructions(&self) -> String {
        let import = match self.kind {
            Kind::Vite => self.vite_import(),
            Kind::Next => self.next_import(),
        };
        let call = match self.kind {
            Kind::Vite => "plugins: [penwise(), ...]",
            Kind::Next => "wrap the exported config: module.exports = withPenwise(nextConfig)",
        };
        format!(
            "cannot edit {} automatically — add the adapter by hand:\n  {import}\n  {call}\n  \
             package.json devDependencies: \"{}\": \"file:{}\"",
            self.config.display(),
            self.kind.dev_dependency(),
            self.package_dir.display()
        )
    }
}

/// The `vite.config.*` / `next.config.*` file in `root`, in the order the frameworks look for it.
fn config_file(root: &Path, kind: Kind) -> Option<PathBuf> {
    let names: &[&str] = match kind {
        Kind::Vite => &[
            "vite.config.ts",
            "vite.config.mts",
            "vite.config.js",
            "vite.config.mjs",
        ],
        Kind::Next => &["next.config.js", "next.config.mjs", "next.config.ts"],
    };
    names
        .iter()
        .map(|name| root.join(name))
        .find(|path| path.is_file())
}

/// The adapter entry file matching the config: Vite has one ESM entry; Next picks CJS or ESM by config type.
fn module_name(kind: Kind, config: &Path) -> Option<&'static str> {
    match kind {
        Kind::Vite => Some("index.js"),
        Kind::Next if config.extension()? == "js" => Some("index.cjs"),
        Kind::Next => Some("index.mjs"),
    }
}

/// The folder holding the adapter packages, when this machine has them.
#[must_use]
pub fn adapters_base() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(ADAPTERS_ENV) {
        let path = PathBuf::from(path);
        if path.join("vite/index.js").is_file() {
            return Some(path);
        }
    }
    let from_exe = std::env::current_exe().ok().and_then(|exe| {
        exe.ancestors().skip(1).take(4).find_map(|ancestor| {
            let candidate = ancestor.join("adapters");
            candidate
                .join("vite/index.js")
                .is_file()
                .then_some(candidate)
        })
    });
    // The installed binary is a copy, not a symlink: fall back to the checkout it was built from.
    from_exe.or_else(|| {
        let built_from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters");
        built_from
            .join("vite/index.js")
            .is_file()
            .then_some(built_from)
    })
}

mod edit;

pub use self::edit::{dev_dependency, next_edit, vite_edit};

#[cfg(test)]
mod tests;
