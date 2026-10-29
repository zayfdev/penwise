//! Argument parsing: four commands, hand-rolled (there is no `clap` in the tree).

use std::path::PathBuf;

use crate::error::Error;

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// No arguments (the desktop entry) or `open`: the app window, with browser arguments after `--`.
    Open(Vec<String>),
    /// `link [dir] [--yes]`.
    Link {
        dir: PathBuf,
        yes: bool,
    },
    /// `unlink [dir]`.
    Unlink {
        dir: PathBuf,
    },
    /// `status`.
    Status,
    Help,
    Version,
}

pub const USAGE: &str =
    "usage: penwise                      open the Penwise window (starts the server if needed)
       penwise open [browser args…]    the same, with extra arguments for the browser
       penwise link [dir] [--yes]      link a project folder (default: the current folder)
       penwise unlink [dir]            drop the link and its registry entry
       penwise status                  linked projects and whether their dev servers answer";

/// Parses `args` (the program name already removed).
///
/// # Errors
/// An unknown command or option, with the usage text.
pub fn parse(args: &[String]) -> Result<Command, Error> {
    let Some((first, rest)) = args.split_first() else {
        return Ok(Command::Open(Vec::new()));
    };
    match first.as_str() {
        "open" => Ok(Command::Open(rest.to_vec())),
        "link" => parse_link(rest),
        "unlink" => parse_unlink(rest),
        "status" => no_more(rest).map(|()| Command::Status),
        "--help" | "-h" | "help" => Ok(Command::Help),
        "--version" | "-V" => Ok(Command::Version),
        other => Err(unknown(other)),
    }
}

/// `link [dir] [--yes]`, options anywhere.
fn parse_link(rest: &[String]) -> Result<Command, Error> {
    let mut yes = false;
    let mut dir = None;
    for arg in rest {
        match arg.as_str() {
            "--yes" | "-y" => yes = true,
            option if option.starts_with('-') => return Err(unknown(option)),
            folder => {
                if dir.replace(PathBuf::from(folder)).is_some() {
                    return Err(usage("only one folder can be linked"));
                }
            }
        }
    }
    Ok(Command::Link {
        dir: dir.unwrap_or_else(|| PathBuf::from(".")),
        yes,
    })
}

fn parse_unlink(rest: &[String]) -> Result<Command, Error> {
    match rest {
        [] => Ok(Command::Unlink {
            dir: PathBuf::from("."),
        }),
        [dir] if !dir.starts_with('-') => Ok(Command::Unlink {
            dir: PathBuf::from(dir),
        }),
        [option, ..] if option.starts_with('-') => Err(unknown(option)),
        _ => Err(usage("unlink takes at most one folder")),
    }
}

fn no_more(rest: &[String]) -> Result<(), Error> {
    match rest.first() {
        None => Ok(()),
        Some(option) => Err(unknown(option)),
    }
}

fn unknown(argument: &str) -> Error {
    usage(&format!("unknown argument {argument:?}"))
}

fn usage(message: &str) -> Error {
    Error::Usage(format!("{message}\n\n{USAGE}"))
}

#[cfg(test)]
mod tests;
