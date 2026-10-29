//! `penwise`: the design harness CLI (link projects, open the desktop window, report the registry).
#![allow(
    clippy::print_stderr,
    reason = "the binary's one diagnostic channel; `run` prints the normal output"
)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match penwise_cli::run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("penwise: {error}");
            ExitCode::FAILURE
        }
    }
}
