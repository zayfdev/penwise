//! `penwise link [dir]`: detect the project, register it, write `.penwise.json`.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::client::{self, Client, Register};
use crate::detect::{Detected, detect};
use crate::error::Error;
use crate::project_file::Link;

/// What `link` was asked to do.
#[derive(Debug)]
pub struct Options {
    /// The folder to link (the current folder by default).
    pub dir: PathBuf,
    /// Skip the confirmation.
    pub yes: bool,
}

/// Links `options.dir` with the running server.
///
/// # Errors
/// The folder is not detectable, the server does not answer, the registration is refused, or the link file
/// cannot be written.
pub fn run(options: &Options) -> Result<(), Error> {
    let detected = detect(&options.dir)?;
    let base = client::base_url();
    println!("{} — {}", detected.name, detected.describe());
    println!("  server {base}");
    if !options.yes && !confirm()? {
        println!("not linked");
        return Ok(());
    }
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
    let file = save(&detected, &project.id)?;
    report(&project, &file);
    Ok(())
}

/// Writes the link record: what was detected (the server echoes it) plus the id it assigned.
fn save(detected: &Detected, id: &str) -> Result<PathBuf, Error> {
    let link = Link {
        project: id.to_owned(),
        name: detected.name.clone(),
        r#static: detected
            .static_folder()
            .map(|folder| folder.to_string_lossy().into_owned()),
        origin: detected.origin().map(str::to_owned),
    };
    link.save(&detected.root)
}

fn report(project: &crate::client::Project, file: &Path) {
    println!("linked \"{}\" as project {}", project.name, project.id);
    println!("  port   {}", project.port);
    println!("  frame  http://127.0.0.1:{}/", project.port);
    println!("  file   {}", file.display());
}

/// `[Y/n]`: an empty answer (or end of input) means yes.
fn confirm() -> Result<bool, Error> {
    print!("register with Penwise? [Y/n] ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer.is_empty() || answer == "y" || answer == "yes")
}
