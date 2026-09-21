use super::{Cli, reject_session_options};
use clap::Parser;

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).expect("a well-formed command line")
}

#[test]
fn session_options_on_a_command_that_starts_no_session_are_rejected() {
    let error = reject_session_options(&parse(&["nightcrow", "--port", "9000", "status"]))
        .expect_err("a conflict");

    assert!(error.to_string().contains("--port"), "{error}");
    assert!(error.to_string().contains("status"), "{error}");
}

#[test]
fn every_session_option_in_play_is_named_in_one_error() {
    let error = reject_session_options(&parse(&[
        "nightcrow",
        "-d",
        "--bind",
        "127.0.0.1",
        "--exec",
        "claude",
        "stop",
    ]))
    .expect_err("a conflict");
    let text = error.to_string();

    for option in ["--exec", "--bind", "--daemon"] {
        assert!(text.contains(option), "{option} missing from: {text}");
    }
}

#[test]
fn attach_accepts_session_options_because_it_may_start_the_daemon() {
    reject_session_options(&parse(&["nightcrow", "--port", "9000", "attach"]))
        .expect("attach hands the options to the daemon it starts");
    reject_session_options(&parse(&["nightcrow", "-d", "attach"]))
        .expect("backgrounding is what attach already does");
}

#[test]
fn a_bare_subcommand_with_no_session_options_is_accepted() {
    reject_session_options(&parse(&["nightcrow", "status"])).expect("nothing to conflict with");
}

#[test]
fn the_default_command_accepts_every_session_option() {
    reject_session_options(&parse(&["nightcrow", "-d", "--port", "9000"]))
        .expect("this is the invocation the options exist for");
}
