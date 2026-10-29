//! `penwise unlink [dir]`: remove the registry entry and the link record.

use std::path::Path;

use crate::client::{self, Client};
use crate::error::Error;
use crate::project_file::Link;

/// Unlinks the project in `dir`.
///
/// # Errors
/// The folder is not linked, the server cannot be reached, or the file cannot be removed.
pub fn run(dir: &Path) -> Result<(), Error> {
    let link = Link::load(dir)?;
    let base = client::base_url();
    let client = Client::connect(&base)?;
    let removed = client.delete(&link.project)?;
    Link::remove(dir)?;
    if removed {
        println!(
            "unlinked \"{}\" (project {}) from {base}",
            link.name, link.project
        );
    } else {
        println!(
            "removed {}; project {} was not registered on {base} any more",
            dir.join(crate::project_file::FILE).display(),
            link.project
        );
    }
    Ok(())
}
