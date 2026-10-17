mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use common::{TestEnv, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::{ApplyReport, ApplyRequest, Ctx, TargetStatus, apply};
use irida_core::palette::Palette;
use irida_core::reload::ReloadStatus;
use irida_core::store::Settings;
use irida_core::template::Renderer;

/// Apply bat with a fake `bat` on the path that logs its arguments.
fn apply_with_fake_bat(env: &TestEnv, p: &Palette) -> (ApplyReport, std::path::PathBuf) {
    let bin = env.tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let log = env.tmp.path().join("bat.log");
    let bat = bin.join("bat");
    fs::write(
        &bat,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\n", log.display()),
    )
    .unwrap();
    fs::set_permissions(&bat, fs::Permissions::from_mode(0o755)).unwrap();
    let ctx = Ctx {
        dirs: env.dirs.clone(),
        path: vec![bin],
    };
    let report = apply(
        &ctx,
        &Settings::default(),
        &ApplyRequest {
            palette: p,
            pair: None,
            only: Some(vec!["bat".to_string()]),
            live_only: false,
        },
    )
    .unwrap();
    (report, log)
}

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "bat", "irida.tmTheme.tmpl"),
            fixture_text(&format!("expected/bat/{slug}.tmTheme")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "bat", "install.sh.tmpl"),
            fixture_text(&format!("expected/bat/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "bat", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn selector_value() {
    let source = common::target_file("bat", "theme.tmpl");
    let p = palette("ember");
    assert_eq!(
        Renderer::new(&p).render("theme.tmpl", &source).unwrap(),
        "irida"
    );
    assert_eq!(
        Renderer::for_export(&p)
            .render("theme.tmpl", &source)
            .unwrap(),
        "ember"
    );
}

/// Balanced plist tags, every colour string a `#RRGGBB`, and the scopes
/// bat needs present.
#[test]
fn theme_is_a_plist_with_hex_colours() {
    let out = render_export("ember", "bat", "irida.tmTheme.tmpl");
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    for tag in ["plist", "dict", "array", "key", "string"] {
        let open = out.matches(&format!("<{tag}")).count();
        let close = out.matches(&format!("</{tag}>")).count();
        assert_eq!(open, close, "{tag}: {open} open, {close} close");
    }
    let mut colours = 0;
    for line in out.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("<string>#") {
            let hex = v
                .strip_suffix("</string>")
                .unwrap_or_else(|| panic!("{line:?}"));
            assert!(
                hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
                "{line:?}"
            );
            colours += 1;
        }
    }
    assert!(colours >= 30, "{colours} colours");
    for key in [
        "background",
        "foreground",
        "caret",
        "lineHighlight",
        "selection",
    ] {
        assert!(out.contains(&format!("<key>{key}</key>")), "{key}");
    }
    for scope in [
        "comment",
        "string",
        "keyword",
        "entity.name.function",
        "markup.heading",
    ] {
        assert!(
            out.contains(&format!("<string>{scope}</string>")),
            "{scope}"
        );
    }
}

#[test]
fn apply_sets_theme_rebuilds_cache_and_undo_restores() {
    let env = TestEnv::new();
    let text = "--style=plain\n--theme=\"TwoDark\"\n";
    let conf = env.write(".config/bat/config", text);
    let (report, log) = apply_with_fake_bat(&env, &palette("ember"));
    let t = report.get("bat").unwrap();
    assert_eq!(
        t.status,
        TargetStatus::Applied(ReloadStatus::Reloaded),
        "{t:?}"
    );
    assert_eq!(fs::read_to_string(&log).unwrap(), "cache --build\n");
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "--style=plain\n--theme=irida\n"
    );
    let theme = env.dirs.home.join(".config/bat/themes/irida.tmTheme");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "bat", "irida.tmTheme.tmpl")
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
    assert!(!theme.exists());
}

#[test]
fn commented_theme_line_is_not_replaced() {
    let env = TestEnv::new();
    let conf = env.write(".config/bat/config", "#--theme=\"TwoDark\"\n");
    apply_with_fake_bat(&env, &palette("ember"));
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "#--theme=\"TwoDark\"\n--theme=irida\n"
    );
}
