//! `penwise`: link a project folder, open the desktop window, report the registry (design-harness spec §8).
//!
//! The commands are plain loopback JSON against the running server: `GET /pair/local` for the token, then the
//! project registry ([`client`], contract C2). Detection ([`detect`]) reads the folder; the link record lives in
//! `.penwise.json` next to the source ([`project_file`]).
#![allow(
    clippy::print_stdout,
    reason = "the CLI's output is the point; errors go to stderr in the binary"
)]

pub mod adapter;
pub mod args;
pub mod client;
pub mod detect;
pub mod diff;
pub mod error;
pub mod hook;
pub mod inbox;
pub mod launcher;
pub mod link;
pub mod mcp_config;
pub mod offer;
pub mod project_file;
pub mod skill;
pub mod status;
pub mod unlink;

use args::{Command, USAGE};
use error::Error;

/// Runs one command line (`args` excludes the program name).
///
/// # Errors
/// Any command failure, with a message a person can act on.
pub fn run(args: &[String]) -> Result<(), Error> {
    match args::parse(args)? {
        Command::Open(extra) => launcher::open(&extra),
        Command::Link {
            dir,
            yes,
            no_adapter,
        } => link::run(&link::Options {
            dir,
            yes,
            no_adapter,
        }),
        Command::Unlink { dir } => unlink::run(&dir),
        Command::Status => status::run(),
        Command::Inbox => {
            inbox::run();
            Ok(())
        }
        Command::Help => {
            println!("{USAGE}");
            Ok(())
        }
        Command::Version => {
            println!("penwise {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
