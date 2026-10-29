//! `penwise link [dir]`: detect the project, offer the adapter, register it, write `.penwise.json` and the
//! agent's MCP config (design-harness spec §8).

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::adapter::{self, Plan, Prepared};
use crate::client::{self, Client, Register};
use crate::detect::{Detected, detect};
use crate::error::Error;
use crate::mcp_config::Targets;
use crate::project_file::Link;

/// What `link` was asked to do.
#[derive(Debug)]
pub struct Options {
    /// The folder to link (the current folder by default).
    pub dir: PathBuf,
    /// Skip every confirmation.
    pub yes: bool,
    /// Never touch the framework config or `package.json`.
    pub no_adapter: bool,
}

/// Links `options.dir` with the running server.
///
/// # Errors
/// The folder is not detectable, the adapter or link file cannot be written, the server does not answer, or the
/// registration is refused.
pub fn run(options: &Options) -> Result<(), Error> {
    let detected = detect(&options.dir)?;
    let base = client::base_url();
    println!("{} — {}", detected.name, detected.describe());
    println!("  server {base}");
    if !options.yes && !ask("register with Penwise? [Y/n] ", true)? {
        println!("not linked");
        return Ok(());
    }
    let adapter = offer_adapter(&detected, options)?;
    let client = Client::connect(&base)?;
    let root = detected.root.to_string_lossy();
    let static_dir = detected
        .static_folder()
        .map(|folder| folder.to_string_lossy());
    let request = Register {
        name: &detected.name,
        root: root.as_ref(),
        r#static: static_dir.as_deref(),
        origin: detected.origin(),
    };
    let project = client.register(&request)?;
    let file = save(&detected, &project.id, adapter.as_deref())?;
    report(&project, &file);
    offer_mcp(&detected.root, &project.id, options.yes);
    Ok(())
}

/// Shows the adapter diff and applies it after a confirmation; returns the adapter name when it is in place.
fn offer_adapter(detected: &Detected, options: &Options) -> Result<Option<String>, Error> {
    if options.no_adapter {
        return Ok(None);
    }
    let Some(plan) = adapter::plan(&detected.root, detected.framework) else {
        return Ok(None);
    };
    println!(
        "\nadapter: {} — adds data-pw-src=\"file:line:col\" to host elements in dev builds",
        plan.kind.as_str()
    );
    let (config, package) = match plan.prepare()? {
        Prepared::Installed => {
            println!("  already in {}", plan.config.display());
            return Ok(Some(plan.kind.as_str().to_owned()));
        }
        Prepared::Manual(text) => {
            println!("{text}");
            return Ok(None);
        }
        Prepared::Ready { config, package } => (config, package),
    };
    println!("{}", config.diff());
    if let Some(package) = &package {
        println!("{}", package.diff());
    }
    let prompt = format!("apply the adapter to {}? [y/N] ", plan.config.display());
    if !options.yes && !ask(&prompt, false)? {
        println!("  adapter not applied");
        return Ok(None);
    }
    Plan::apply(&config, package.as_ref())?;
    println!("  applied — run `npm install` in the project to link the adapter package");
    Ok(Some(plan.kind.as_str().to_owned()))
}

/// Offers the two agent config files (Claude Code's `.mcp.json`, omp's `.omp/mcp.json`) and writes them.
///
/// A write failure is reported but does not undo the link: the project is registered either way.
fn offer_mcp(root: &Path, project: &str, yes: bool) {
    let targets = Targets::of(root);
    println!(
        "\nMCP config for agents (penwise → {}):",
        targets.claude.display()
    );
    for (client, path) in targets.all() {
        println!("  {client:<12} {}", path.display());
    }
    let confirmed = yes || ask("write the MCP config? [Y/n] ", true).unwrap_or(false);
    if !confirmed {
        println!("  MCP config not written");
        return;
    }
    match crate::mcp_config::write(root, project) {
        Ok(written) => {
            for (client, path) in written {
                println!("  wrote {client}: {}", path.display());
            }
        }
        Err(error) => println!("  could not write the MCP config: {error}"),
    }
}

/// Writes the link record: what was detected (the server echoes it) plus the id it assigned.
fn save(detected: &Detected, id: &str, adapter: Option<&str>) -> Result<PathBuf, Error> {
    let link = Link {
        project: id.to_owned(),
        name: detected.name.clone(),
        r#static: detected
            .static_folder()
            .map(|folder| folder.to_string_lossy().into_owned()),
        origin: detected.origin().map(str::to_owned),
        adapter: adapter.map(str::to_owned),
    };
    link.save(&detected.root)
}

fn report(project: &crate::client::Project, file: &Path) {
    println!("linked \"{}\" as project {}", project.name, project.id);
    println!("  port   {}", project.port);
    println!("  frame  http://127.0.0.1:{}/", project.port);
    println!("  file   {}", file.display());
}

/// One y/N (or Y/n) answer; an empty line, end of input, or an unreadable prompt takes `default_yes`.
fn ask(prompt: &str, default_yes: bool) -> Result<bool, Error> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Ok(default_yes);
    }
    let answer = line.trim().to_ascii_lowercase();
    if answer.is_empty() {
        return Ok(default_yes);
    }
    Ok(answer == "y" || answer == "yes")
}
