mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::TargetStatus;
use irida_core::template::Renderer;

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "zellij", "irida.kdl.tmpl"),
            fixture_text(&format!("expected/zellij/{slug}.kdl")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "zellij", "install.sh.tmpl"),
            fixture_text(&format!("expected/zellij/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "zellij", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

fn is_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// The theme is the legacy palette: `themes { <name> { key "#rrggbb" ... } }`.
fn check_theme(out: &str, name: &str) {
    let body: Vec<&str> = out
        .lines()
        .filter(|l| !l.trim_start().starts_with("//") && !l.trim().is_empty())
        .collect();
    assert_eq!(body[0], "themes {", "{out}");
    assert_eq!(body[1], format!("    {name} {{"), "{out}");
    assert_eq!(body[body.len() - 2], "    }", "{out}");
    assert_eq!(body[body.len() - 1], "}", "{out}");
    let keys: Vec<&str> = body[2..body.len() - 2]
        .iter()
        .map(|l| {
            let (k, v) = l.trim().split_once(' ').unwrap();
            assert!(is_hex(v.trim_matches('"')) && v.starts_with('"'), "{l}");
            k
        })
        .collect();
    assert_eq!(
        keys,
        [
            "fg", "bg", "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
            "orange"
        ]
    );
}

#[test]
fn theme_is_legacy_palette() {
    check_theme(&render_apply("ember", "zellij", "irida.kdl.tmpl"), "irida");
    check_theme(&render_export("ember", "zellij", "irida.kdl.tmpl"), "ember");
}

#[test]
fn selector_value() {
    let source = common::target_file("zellij", "theme.tmpl");
    let p = palette("ember");
    assert_eq!(
        Renderer::new(&p).render("theme.tmpl", &source).unwrap(),
        "\"irida\""
    );
    assert_eq!(
        Renderer::for_export(&p)
            .render("theme.tmpl", &source)
            .unwrap(),
        "\"ember\""
    );
}

#[test]
fn apply_sets_theme_and_undo_restores() {
    let env = TestEnv::new();
    let text = "// theme \"dracula\"\nmouse_mode true\n";
    let conf = env.write(".config/zellij/config.kdl", text);
    let report = apply_only(&env, &palette("ember"), "zellij").unwrap();
    assert!(
        matches!(
            report.get("zellij").unwrap().status,
            TargetStatus::Applied(_)
        ),
        "{:?}",
        report.get("zellij")
    );
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        format!("{text}theme \"irida\"\n")
    );
    let theme = env.dirs.home.join(".config/zellij/themes/irida.kdl");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "zellij", "irida.kdl.tmpl")
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
    assert!(!theme.exists());
}

#[test]
fn user_theme_dark_light_are_named() {
    let env = TestEnv::new();
    env.write(
        ".config/zellij/config.kdl",
        "theme_dark \"x\"\ntheme_light \"y\"\n",
    );
    let report = apply_only(&env, &palette("ember"), "zellij").unwrap();
    let w = &report.get("zellij").unwrap().warnings;
    assert!(
        w.iter().any(|m| m.contains("theme_dark and theme_light")),
        "{w:?}"
    );
}
