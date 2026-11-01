mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use common::{TestEnv, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::{ApplyRequest, Ctx, TargetStatus, apply};
use irida_core::store::Settings;

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "lazygit", "irida.yml.tmpl"),
            fixture_text(&format!("expected/lazygit/{slug}.yml")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "lazygit", "install.sh.tmpl"),
            fixture_text(&format!("expected/lazygit/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(
        &script,
        render_export("ember", "lazygit", "install.sh.tmpl"),
    )
    .unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

/// Every colour is a quoted `#RRGGBB` list item or map value; indentation is
/// two spaces; the file names the one it is read beside.
#[test]
fn theme_is_yaml_shaped() {
    for (slug, out, file) in [
        (
            "ember",
            render_export("ember", "lazygit", "irida.yml.tmpl"),
            "ember.yml",
        ),
        (
            "emberday",
            render_apply("emberday", "lazygit", "irida.yml.tmpl"),
            "irida.yml",
        ),
    ] {
        let mut colours = 0;
        for line in out.lines().filter(|l| !l.starts_with('#')) {
            assert!(!line.contains('\t'), "{slug}: {line:?}");
            let indent = line.len() - line.trim_start().len();
            assert!(indent % 2 == 0, "{slug}: {line:?}");
            if let Some(i) = line.find("\"#") {
                let hex = &line[i + 1..i + 8];
                assert!(
                    hex.len() == 7 && hex[1..].bytes().all(|b| b.is_ascii_hexdigit()),
                    "{slug}: {line:?}"
                );
                colours += 1;
            }
        }
        assert_eq!(colours, 13, "{slug}: {out}");
        assert!(out.contains(&format!("lazygit/{file}")), "{slug}");
    }
}

/// Apply with a fake lazygit on the path writes irida.yml and says how it is
/// read; undo removes it and leaves config.yml untouched.
#[test]
fn apply_writes_theme_beside_config_and_undo_removes_it() {
    let env = TestEnv::new();
    let bin = env.tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let lazygit = bin.join("lazygit");
    fs::write(&lazygit, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&lazygit, fs::Permissions::from_mode(0o755)).unwrap();
    let conf = env.write(
        ".config/lazygit/config.yml",
        "gui:\n  nerdFontsVersion: \"3\"\n",
    );
    let ctx = Ctx {
        dirs: env.dirs.clone(),
        path: vec![bin],
    };
    let ember = palette("ember");
    let report = apply(
        &ctx,
        &Settings::default(),
        &ApplyRequest {
            palette: &ember,
            pair: None,
            only: Some(vec!["lazygit".to_string()]),
            live_only: false,
        },
    )
    .unwrap();
    let t = report.get("lazygit").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    assert!(
        t.warnings.iter().any(|w| w.contains("LG_CONFIG_FILE")),
        "{:?}",
        t.warnings
    );
    let theme = env.dirs.home.join(".config/lazygit/irida.yml");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "lazygit", "irida.yml.tmpl")
    );
    undo(&env);
    assert!(!theme.exists());
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "gui:\n  nerdFontsVersion: \"3\"\n"
    );
}
