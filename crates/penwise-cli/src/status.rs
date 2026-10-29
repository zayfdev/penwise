//! `penwise status`: the linked projects and whether their dev servers answer.

use crate::client::{self, Client, Project};
use crate::error::Error;

/// Prints the registry held by the running server.
///
/// # Errors
/// The server does not answer, or its answer is unreadable.
pub fn run() -> Result<(), Error> {
    let base = client::base_url();
    let client = Client::connect(&base)?;
    let projects = client.projects()?;
    if projects.is_empty() {
        println!("no projects linked on {base}");
        return Ok(());
    }
    println!("{} project(s) on {base}", projects.len());
    for project in &projects {
        println!("{}", line(project));
    }
    Ok(())
}

fn line(project: &Project) -> String {
    let mode = match (project.static_dir.as_deref(), project.origin.as_deref()) {
        (Some(folder), _) => folder.to_owned(),
        (None, Some(origin)) => {
            let state = if client::answers(origin) {
                "up"
            } else {
                "down"
            };
            format!("{origin} {state}")
        }
        (None, None) => project.root.clone(),
    };
    format!(
        "{:<20} {:<7} {:<5} {mode}",
        project.id, project.kind, project.port
    )
}
