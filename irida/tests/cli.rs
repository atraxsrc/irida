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
const EMBER_RICE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../irida-core/tests/fixtures/rices/cosmic-ember-theme"
);

/// The binary with its home, XDG folders and temporary folder inside `tmp`.
fn irida(tmp: &TempDir) -> Command {
    let home = tmp.path().join("home");
    let scratch = tmp.path().join("tmp");
    fs::create_dir_all(&scratch).unwrap();
    let mut cmd = Command::cargo_bin("irida").unwrap();
    cmd.env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("TMPDIR", &scratch);
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
fn no_display_message() {
    let tmp = TempDir::new().unwrap();
    irida(&tmp)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .assert()
        .code(2)
        .stdout("")
        .stderr("irida: no display found; run irida --help for the command line\n");
    irida(&tmp)
        .env("WAYLAND_DISPLAY", "")
        .env("DISPLAY", "")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no display found"));
    irida(&tmp)
        .arg("--help")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Usage"))
        .stdout(predicate::str::contains("draft"));
}

const VECTORS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../irida-core/tests/fixtures/draft/vectors.json"
);

/// The reference vector `case` written as a 40 by 25 PNG at `path`.
fn vector_png(case: &str, path: &Path) {
    use base64::Engine;
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(VECTORS).unwrap()).unwrap();
    let rgba = base64::engine::general_purpose::STANDARD
        .decode(v[case]["rgba"].as_str().unwrap())
        .unwrap();
    image::RgbaImage::from_raw(40, 25, rgba)
        .unwrap()
        .save(path)
        .unwrap();
}

/// The stdout of a run that must exit 0.
fn ok_stdout(cmd: &mut Command) -> String {
    String::from_utf8(cmd.assert().code(0).get_output().stdout.clone()).unwrap()
}

fn stored(tmp: &TempDir, slug: &str) -> String {
    fs::read_to_string(
        tmp.path()
            .join("home/.local/share/irida/palettes")
            .join(slug)
            .join("palette.toml"),
    )
    .unwrap()
}

#[test]
fn draft_saves_and_refuses() {
    let tmp = TempDir::new().unwrap();
    let png = tmp.path().join("two-tone.png");
    vector_png("two-tone", &png);
    let out = ok_stdout(irida(&tmp).arg("draft").arg(&png));
    assert!(
        out.starts_with("saved twotone (Two Tone, dark)\nTwo Tone (dark)\n"),
        "{out}"
    );
    assert!(out.ends_with("all pass\n"), "{out}");
    let text = stored(&tmp, "twotone");
    assert!(text.contains("name = \"Two Tone\""), "{text}");
    assert!(text.contains("mode = \"dark\""), "{text}");
    let versions = tmp
        .path()
        .join("home/.local/share/irida/palettes/twotone/versions/1.toml");
    assert_eq!(fs::read_to_string(versions).unwrap(), text);

    irida(&tmp)
        .arg("draft")
        .arg(&png)
        .assert()
        .code(2)
        .stdout("")
        .stderr("irida: a palette called twotone already exists; pass --name\n");
    assert_eq!(stored(&tmp, "twotone"), text, "unchanged");

    let out = ok_stdout(irida(&tmp).arg("draft").arg(&png).args([
        "--name",
        "Night Sky",
        "--mode",
        "light",
    ]));
    assert!(
        out.starts_with("saved nightsky (Night Sky, light)\n"),
        "{out}"
    );
    assert!(stored(&tmp, "nightsky").contains("mode = \"light\""));
    irida(&tmp).args(["check", "nightsky"]).assert().code(0);
}

#[test]
fn draft_every_vector_passes_check() {
    let tmp = TempDir::new().unwrap();
    for case in ["two-tone", "flat", "grey", "noise", "pastel"] {
        let png = tmp.path().join(format!("{case}.png"));
        vector_png(case, &png);
        let name = format!("Vector {}", case.replace('-', " "));
        irida(&tmp)
            .arg("draft")
            .arg(&png)
            .args(["--name", &name])
            .assert()
            .code(0);
        let slug: String = name
            .chars()
            .filter(char::is_ascii_alphabetic)
            .collect::<String>()
            .to_lowercase();
        irida(&tmp)
            .args(["check", &slug])
            .assert()
            .code(0)
            .stdout(predicate::str::contains("all pass"));
    }
}

#[test]
fn draft_refuses_bad_input() {
    let tmp = TempDir::new().unwrap();
    let text = tmp.path().join("notes.png");
    fs::write(&text, "hello").unwrap();
    irida(&tmp)
        .arg("draft")
        .arg(&text)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("could not be read as an image"));
    let png = tmp.path().join("two-tone.png");
    vector_png("two-tone", &png);
    for bad in ["Bad-Name", "2024", ""] {
        irida(&tmp)
            .arg("draft")
            .arg(&png)
            .args(["--name", bad])
            .assert()
            .code(2)
            .stderr(predicate::str::starts_with("irida: "));
    }
    irida(&tmp)
        .arg("draft")
        .arg(&png)
        .args(["--mode", "dim"])
        .assert()
        .code(2);
    assert!(!tmp.path().join("home/.local/share/irida/palettes").exists());
}

/// A temp home with stored palettes, a btop config, and only fake apps on
/// `PATH`. The fake `btop` reports a version below the signal threshold, so
/// nothing is ever signalled, and there is no `cava`.
fn apply_env(tmp: &TempDir) -> Command {
    let home = tmp.path().join("home");
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    for (name, body) in [
        ("btop", "#!/bin/sh\necho 'btop version: 1.3.0'\n"),
        ("cosmic-term", "#!/bin/sh\nexit 0\n"),
        ("cosmic-settings", "#!/bin/sh\nexit 0\n"),
        ("nvim", "#!/bin/sh\nexit 0\n"),
        ("fastfetch", "#!/bin/sh\nexit 0\n"),
        ("lsd", "#!/bin/sh\nexit 0\n"),
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
    cmd.env("PATH", &bin);
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
        .stdout(predicate::str::contains(
            "btop: applied, restart btop to see it",
        ))
        .stdout(predicate::str::contains("COSMIC Terminal: applied"))
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
fn undo_refuses_during_live_session() {
    let tmp = TempDir::new().unwrap();
    apply_env(&tmp)
        .args(["apply", "ember", "--only", "btop"])
        .assert()
        .code(0);
    // A live session whose record is apply 1.
    let marker = tmp.path().join("home/.local/state/irida/live.toml");
    fs::write(&marker, "palette = \"ember\"\nrecord = 1\n").unwrap();
    apply_env(&tmp)
        .arg("undo")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "A live session is open; keep or revert it in Irida first",
        ));
    assert!(
        tmp.path()
            .join("home/.local/state/irida/applies/1")
            .is_dir()
    );

    fs::remove_file(&marker).unwrap();
    apply_env(&tmp).arg("undo").assert().code(0);
}

#[test]
fn apply_refuses_during_live_session() {
    let tmp = TempDir::new().unwrap();
    apply_env(&tmp)
        .args(["apply", "ember", "--only", "btop"])
        .assert()
        .code(0);
    let state = tmp.path().join("home/.local/state/irida");
    let conf = tmp.path().join("home/.config/btop/themes/irida.theme");
    let applied = fs::read(&conf).unwrap();
    // A live session whose record is apply 1.
    let marker = state.join("live.toml");
    fs::write(&marker, "palette = \"ember\"\nrecord = 1\n").unwrap();
    apply_env(&tmp)
        .args(["apply", "emberday", "--only", "btop"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "A live session is open; keep or revert it in Irida first",
        ));
    assert_eq!(fs::read(&conf).unwrap(), applied, "nothing written");
    assert!(!state.join("applies/2").exists(), "no record made");

    fs::remove_file(&marker).unwrap();
    apply_env(&tmp)
        .args(["apply", "emberday", "--only", "btop"])
        .assert()
        .code(0);
    assert!(state.join("applies/2").is_dir());
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
    assert!(
        stderr.contains("irida: warning: could not save settings:"),
        "{stderr}"
    );
}

fn fixture_text(rel: &str) -> String {
    fs::read_to_string(Path::new(EMBER_RICE).join(rel)).unwrap()
}

fn stdout_of(cmd: &mut Command) -> String {
    String::from_utf8(cmd.output().unwrap().stdout).unwrap()
}

#[test]
fn export_ember() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out");
    fs::create_dir_all(&out).unwrap();
    apply_env(&tmp)
        .args(["export", "ember"])
        .arg(&out)
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Exported Ember to"))
        .stdout(predicate::str::contains("  created   cosmic/Ember.ron\n"))
        .stdout(predicate::str::contains("By hand: wallpaper"))
        .stdout(predicate::str::contains("Contrast: 1 failure(s)"))
        .stdout(predicate::str::contains(
            "error    #D85A5A   4.7:1   4.4:1  4.5  FAIL on pane",
        ));
    assert_eq!(
        fs::read_to_string(out.join("cosmic-ember-theme/cosmic/Ember.ron")).unwrap(),
        fixture_text("cosmic/Ember.ron")
    );
}

#[test]
fn check_rice_folder() {
    let tmp = TempDir::new().unwrap();
    irida(&tmp)
        .args(["check", EMBER_RICE])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "error    #D85A5A   4.7:1   4.4:1  4.5  FAIL on pane",
        ));
}

#[test]
fn round_trip() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out");
    apply_env(&tmp)
        .args(["export", "ember"])
        .arg(&out)
        .assert()
        .code(0);
    let exported = stdout_of(
        apply_env(&tmp)
            .arg("check")
            .arg(out.join("cosmic-ember-theme")),
    );
    let saved = stdout_of(apply_env(&tmp).args(["check", "ember"]));
    assert!(saved.contains("FAIL on pane"), "{saved}");
    assert_eq!(exported, saved);
}

#[test]
fn apply_rice_folder() {
    let tmp = TempDir::new().unwrap();
    apply_env(&tmp)
        .args(["apply", EMBER_RICE, "--only", "btop"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("btop: applied"));
    let config = tmp.path().join("home/.config");
    assert!(config.join("btop/themes/irida.theme").is_file());
    let settings = fs::read_to_string(config.join("irida/settings.toml")).unwrap_or_default();
    assert!(!settings.contains("last_palette"), "{settings}");

    apply_env(&tmp)
        .args(["apply", "ember", "--only", "btop"])
        .assert()
        .code(0);
    let settings = fs::read_to_string(config.join("irida/settings.toml")).unwrap();
    assert!(settings.contains("last_palette = \"ember\""), "{settings}");
}

#[test]
fn export_wrong_folder() {
    let tmp = TempDir::new().unwrap();
    let noir = tmp.path().join("cosmic-noir-theme");
    fs::create_dir_all(&noir).unwrap();
    apply_env(&tmp)
        .args(["export", "ember"])
        .arg(&noir)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("irida: "))
        .stderr(predicate::str::contains("cosmic-noir-theme"));
    assert_eq!(fs::read_dir(&noir).unwrap().count(), 0);
}

#[test]
fn export_pair_missing() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = apply_env(&tmp);
    fs::remove_dir_all(tmp.path().join("home/.local/share/irida/palettes/emberday")).unwrap();
    cmd.args(["export", "ember"])
        .arg(tmp.path().join("out"))
        .assert()
        .code(0)
        .stderr(predicate::str::contains("exporting alone"));
}

#[test]
fn apply_all_then_undo() {
    let tmp = TempDir::new().unwrap();
    let _ = apply_env(&tmp);
    let home = tmp.path().join("home");
    let zshrc = home.join(".zshrc");
    fs::write(&zshrc, "export X=1\n").unwrap();
    for dir in ["nvim", "fastfetch", "lsd"] {
        fs::create_dir_all(home.join(".config").join(dir)).unwrap();
    }
    let before = config_listing(&tmp);

    let out = stdout_of(apply_env(&tmp).args(["apply", "ember"]));
    for target in [
        "COSMIC desktop",
        "COSMIC Terminal",
        "btop",
        "Neovim",
        "fastfetch",
        "lsd",
        "zsh",
    ] {
        assert!(
            out.lines()
                .any(|l| l.starts_with(&format!("{target}: applied"))),
            "{target}: {out}"
        );
    }
    assert!(out.contains("not installed: cava"), "{out}");
    assert_ne!(config_listing(&tmp), before);
    assert_ne!(fs::read_to_string(&zshrc).unwrap(), "export X=1\n");

    apply_env(&tmp).arg("undo").assert().code(0);
    assert_eq!(config_listing(&tmp), before);
    assert_eq!(fs::read_to_string(&zshrc).unwrap(), "export X=1\n");
}

/// Copy a folder tree.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn rice_notes_reach_stderr() {
    let tmp = TempDir::new().unwrap();
    let noir = tmp.path().join("cosmic-noir-theme");
    copy_tree(
        &Path::new(EMBER_RICE).with_file_name("cosmic-noir-theme"),
        &noir,
    );
    let scheme = noir.join("cosmic-term/Noir-term.ron");
    let text = fs::read_to_string(&scheme).unwrap();
    assert_eq!(text.matches("foreground: \"#CCD0CF\"").count(), 1);
    fs::write(
        &scheme,
        text.replace("foreground: \"#CCD0CF\"", "foreground: \"#CCD0CF80\""),
    )
    .unwrap();
    let note = "irida: note: foreground: alpha 128 in the terminal scheme was dropped; Irida colours are opaque\n";
    irida(&tmp)
        .arg("check")
        .arg(&noir)
        .assert()
        .code(0)
        .stderr(predicate::str::contains(note));
    irida(&tmp)
        .arg("check")
        .arg(EMBER_RICE)
        .assert()
        .stderr(predicate::str::contains("note:").not());
}

#[test]
fn only_help_names_every_builtin() {
    let tmp = TempDir::new().unwrap();
    let out = irida(&tmp).args(["apply", "--help"]).output().unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    for id in [
        "cosmic-desktop",
        "cosmic-term",
        "cava",
        "btop",
        "nvim",
        "fastfetch",
        "lsd",
        "zsh",
        "ghostty",
        "delta",
    ] {
        assert!(help.contains(id), "{id} missing from --only help:\n{help}");
    }
}

#[test]
fn apply_output_uses_display_names() {
    let tmp = TempDir::new().unwrap();
    apply_env(&tmp)
        .args(["apply", "ember"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("COSMIC Terminal: applied"))
        .stdout(predicate::str::contains("cosmic-term").not());
}
