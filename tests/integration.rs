//! Basic integration tests for SessionWeave.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn cli_help_works() {
    let mut cmd = Command::cargo_bin("sw").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("SessionWeave"));
}

#[test]
fn config_path_subcommand() {
    let mut cmd = Command::cargo_bin("sw").unwrap();
    cmd.args(["config", "path"]);
    cmd.assert().success();
}

// Parser is currently private to the crate internals.
// We test via CLI behavior instead in other tests.
#[test]
fn placeholder_for_parser_logic() {
    assert!(true);
}
