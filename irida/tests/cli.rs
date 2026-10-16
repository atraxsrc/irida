use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const EMBER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../irida-core/tests/fixtures/palettes/ember.toml"
);
const NOIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../irida-core/tests/fixtures/palettes/noir.toml"
);

fn irida(tmp: &TempDir) -> Command {
    let home = tmp.path().join("home");
    let mut cmd = Command::cargo_bin("irida").unwrap();
    cmd.env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"));
    cmd
}

#[test]
fn check_ember_fails_on_pane() {
    let tmp = TempDir::new().unwrap();
    irida(&tmp)
        .args(["check", EMBER])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Ember (dark)"))
        .stdout(predicate::str::contains(
            "error    #D85A5A   4.7:1   4.4:1  4.5  FAIL on pane",
        ))
        .stdout(predicate::str::contains("1 failure(s)"));
}

#[test]
fn check_noir_passes() {
    let tmp = TempDir::new().unwrap();
    irida(&tmp)
        .args(["check", NOIR])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("all pass"));
}

#[test]
fn check_unknown_palette() {
    let tmp = TempDir::new().unwrap();
    irida(&tmp)
        .args(["check", "nope"])
        .assert()
        .code(2)
        .stderr(predicate::str::starts_with("irida: "));
}

#[test]
fn list_shows_saved() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("home/.local/share/irida/palettes/ember");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(EMBER, dir.join("palette.toml")).unwrap();
    irida(&tmp)
        .arg("list")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("ember  Ember (dark)"));
}

#[test]
fn no_args_prints_help() {
    let tmp = TempDir::new().unwrap();
    irida(&tmp)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage"));
}
