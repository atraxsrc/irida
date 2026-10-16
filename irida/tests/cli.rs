use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const EMBER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../irida-core/tests/fixtures/palettes/ember.toml"
);
const EMBERDAY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../irida-core/tests/fixtures/palettes/emberday.toml"
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

/// A temp home with stored palettes, a btop config, and fake apps on `PATH`.
/// The fake `btop` reports a version below the signal threshold, so nothing
/// is ever signalled.
fn apply_env(tmp: &TempDir) -> Command {
    let home = tmp.path().join("home");
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    for (name, body) in [
        ("btop", "#!/bin/sh\necho 'btop version: 1.3.0'\n"),
        ("cosmic-term", "#!/bin/sh\nexit 0\n"),
    ] {
        let path = bin.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    for (slug, src) in [("ember", EMBER), ("emberday", EMBERDAY)] {
        let dir = home.join(".local/share/irida/palettes").join(slug);
        fs::create_dir_all(&dir).unwrap();
        fs::copy(src, dir.join("palette.toml")).unwrap();
    }
    let btop = home.join(".config/btop");
    fs::create_dir_all(&btop).unwrap();
    fs::write(btop.join("btop.conf"), "color_theme = \"Default\"\n").unwrap();
    let mut cmd = irida(tmp);
    cmd.env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    cmd
}

/// Every file under the config folder except Irida's own, with its bytes.
fn config_listing(tmp: &TempDir) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path != root.join("irida") {
                    walk(&path, root, out);
                }
            } else {
                out.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }
    let root = tmp.path().join("home/.config");
    let mut out = BTreeMap::new();
    walk(&root, &root, &mut out);
    out
}

#[test]
fn apply_then_undo() {
    let tmp = TempDir::new().unwrap();
    let _ = apply_env(&tmp); // sets up the home before the snapshot
    let before = config_listing(&tmp);

    apply_env(&tmp)
        .args(["apply", "ember"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("btop: applied, restart btop to see it"))
        .stdout(predicate::str::contains("cosmic-term: applied"))
        .stdout(predicate::str::contains("Undo with: irida undo"));
    let config = tmp.path().join("home/.config");
    assert!(config.join("btop/themes/irida.theme").is_file());
    let schemes = config.join("cosmic/com.system76.CosmicTerm/v1");
    assert!(schemes.is_dir());
    assert_ne!(config_listing(&tmp), before);

    apply_env(&tmp)
        .arg("undo")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Restored apply 1 (ember)"));
    assert_eq!(config_listing(&tmp), before);
}

#[test]
fn apply_only() {
    let tmp = TempDir::new().unwrap();
    apply_env(&tmp)
        .args(["apply", "ember", "--only", "btop"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("btop: applied"))
        .stdout(predicate::str::contains("cosmic-term").not());
}

#[test]
fn apply_unknown_target() {
    let tmp = TempDir::new().unwrap();
    apply_env(&tmp)
        .args(["apply", "ember", "--only", "nope"])
        .assert()
        .code(2)
        .stderr(predicate::str::starts_with("irida: "))
        .stderr(predicate::str::contains("nope"));
}

#[test]
fn undo_nothing() {
    let tmp = TempDir::new().unwrap();
    irida(&tmp)
        .arg("undo")
        .assert()
        .code(2)
        .stderr(predicate::str::starts_with("irida: "));
}

#[test]
fn apply_reports_even_when_settings_cannot_be_saved() {
    let tmp = TempDir::new().unwrap();
    let _ = apply_env(&tmp);
    let irida_cfg = tmp.path().join("home/.config/irida");
    fs::create_dir_all(&irida_cfg).unwrap();
    fs::set_permissions(&irida_cfg, fs::Permissions::from_mode(0o555)).unwrap();
    let probe = irida_cfg.join("probe");
    let writable = fs::write(&probe, b"").is_ok();

    let out = (!writable).then(|| apply_env(&tmp).args(["apply", "ember"]).output().unwrap());
    fs::set_permissions(&irida_cfg, fs::Permissions::from_mode(0o755)).unwrap();
    let Some(out) = out else {
        return;
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stdout.contains("btop: applied"), "{stdout}");
    assert!(stdout.contains("Undo with: irida undo"), "{stdout}");
    assert!(stderr.contains("irida: warning: could not save settings:"), "{stderr}");
}
