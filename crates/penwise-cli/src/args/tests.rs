use std::path::PathBuf;

use super::*;

fn parse_strs(args: &[&str]) -> Command {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    parse(&owned).unwrap()
}

fn parse_error(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    parse(&owned).unwrap_err().to_string()
}

#[test]
fn no_arguments_is_the_desktop_window() {
    assert_eq!(Command::Open(Vec::new()), parse_strs(&[]));
}

#[test]
fn browser_arguments_reach_open() {
    assert_eq!(
        Command::Open(vec!["--incognito".to_owned()]),
        parse_strs(&["open", "--incognito"])
    );
}

#[test]
fn link_defaults_to_the_current_folder() {
    assert_eq!(
        Command::Link {
            dir: PathBuf::from("."),
            yes: false,
            no_adapter: false,
        },
        parse_strs(&["link"])
    );
}

#[test]
fn link_takes_a_folder_and_the_yes_flag_in_either_order() {
    let expected = Command::Link {
        dir: PathBuf::from("/tmp/site"),
        yes: true,
        no_adapter: false,
    };
    assert_eq!(expected, parse_strs(&["link", "--yes", "/tmp/site"]));
    assert_eq!(expected, parse_strs(&["link", "/tmp/site", "-y"]));
    let skipped = Command::Link {
        dir: PathBuf::from("/tmp/site"),
        yes: false,
        no_adapter: true,
    };
    assert_eq!(skipped, parse_strs(&["link", "--no-adapter", "/tmp/site"]));
}

#[test]
fn unlink_takes_one_folder() {
    assert_eq!(
        Command::Unlink {
            dir: PathBuf::from("site")
        },
        parse_strs(&["unlink", "site"])
    );
    assert!(parse_error(&["unlink", "a", "b"]).contains("usage: penwise"));
}

#[test]
fn status_takes_nothing() {
    assert_eq!(Command::Status, parse_strs(&["status"]));
    assert!(parse_error(&["status", "--json"]).contains("usage: penwise"));
}

#[test]
fn unknown_commands_and_options_explain_the_usage() {
    for args in [&["publish"][..], &["link", "--fast"], &["status", "x"]] {
        let text = parse_error(args);
        assert!(text.contains("usage: penwise"), "{text}");
    }
}

#[test]
fn help_and_version() {
    assert_eq!(Command::Help, parse_strs(&["--help"]));
    assert_eq!(Command::Version, parse_strs(&["-V"]));
}
