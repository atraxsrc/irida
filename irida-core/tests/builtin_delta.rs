mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::{ApplyReport, ApplyRequest, Ctx, TargetStatus, apply};
use irida_core::palette::Palette;
use irida_core::reload::ReloadStatus;
use irida_core::store::Settings;

/// Apply delta with a fake `delta` binary on the path, so the target is
/// detected even before `~/.gitconfig` exists.
fn apply_with_binary(env: &TestEnv, p: &Palette) -> ApplyReport {
    let bin = env.tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let delta = bin.join("delta");
    fs::write(&delta, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&delta, fs::Permissions::from_mode(0o755)).unwrap();
    let ctx = Ctx {
        dirs: env.dirs.clone(),
        path: vec![bin],
    };
    apply(
        &ctx,
        &Settings::default(),
        &ApplyRequest {
            palette: p,
            pair: None,
            only: Some(vec!["delta".to_string()]),
            live_only: false,
        },
    )
    .unwrap()
}

#[test]
fn feature_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "delta", "irida.gitconfig.tmpl"),
            fixture_text(&format!("expected/delta/{slug}.gitconfig")),
            "{slug}"
        );
    }
}

#[test]
fn block_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "delta", "gitconfig-block.tmpl"),
            fixture_text(&format!("expected/delta/{slug}.block")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "delta", "install.sh.tmpl"),
            fixture_text(&format!("expected/delta/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "delta", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn feature_sets_mode_and_name() {
    let ember = render_export("ember", "delta", "irida.gitconfig.tmpl");
    assert!(
        ember.contains("[delta \"ember\"]\n    dark = true\n"),
        "{ember}"
    );
    let day = render_export("emberday", "delta", "irida.gitconfig.tmpl");
    assert!(
        day.contains("[delta \"emberday\"]\n    light = true\n"),
        "{day}"
    );
    let local = render_apply("ember", "delta", "irida.gitconfig.tmpl");
    assert!(local.contains("[delta \"irida\"]\n"), "{local}");
}

#[test]
fn hex_in_styles_is_quoted() {
    let out = render_export("ember", "delta", "irida.gitconfig.tmpl");
    for line in out.lines() {
        let mut rest = line;
        while let Some(i) = rest.find('#') {
            let after = &rest[i + 1..];
            let is_hex =
                after.len() >= 6 && after.as_bytes()[..6].iter().all(u8::is_ascii_hexdigit);
            if is_hex {
                let before = rest[..i].ends_with('"');
                let closed = after[6..].starts_with('"');
                assert!(before && closed, "unquoted colour in {line:?}");
            }
            rest = after;
        }
    }
}

#[test]
fn apply_creates_gitconfig_and_undo_removes_it() {
    let env = TestEnv::new();
    let report = apply_with_binary(&env, &palette("ember"));
    let t = report.get("delta").unwrap();
    assert!(
        matches!(t.status, TargetStatus::Applied(ReloadStatus::Restart(_))),
        "{:?}",
        t.status
    );
    let gc = env.dirs.home.join(".gitconfig");
    let body = render_apply("ember", "delta", "gitconfig-block.tmpl");
    assert_eq!(
        fs::read_to_string(&gc).unwrap(),
        format!("# >>> irida >>>\n{body}# <<< irida <<<\n")
    );
    let theme = env.dirs.home.join(".config/delta/irida.gitconfig");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "delta", "irida.gitconfig.tmpl")
    );
    undo(&env);
    assert!(!gc.exists());
    assert!(!theme.exists());
}

#[test]
fn apply_keeps_existing_gitconfig() {
    let env = TestEnv::new();
    let gc = env.write(".gitconfig", "[user]\n    name = x\n");
    apply_only(&env, &palette("ember"), "delta").unwrap();
    let body = render_apply("ember", "delta", "gitconfig-block.tmpl");
    assert_eq!(
        fs::read_to_string(&gc).unwrap(),
        format!("[user]\n    name = x\n# >>> irida >>>\n{body}# <<< irida <<<\n")
    );
    apply_only(&env, &palette("emberday"), "delta").unwrap();
    let text = fs::read_to_string(&gc).unwrap();
    assert_eq!(text.matches("# >>> irida >>>").count(), 1);
    assert!(text.starts_with("[user]\n    name = x\n"), "{text}");
    undo(&env);
    undo(&env);
    assert_eq!(fs::read_to_string(&gc).unwrap(), "[user]\n    name = x\n");
}
