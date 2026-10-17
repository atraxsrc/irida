mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::TargetStatus;

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "eza", "theme.yml.tmpl"),
            fixture_text(&format!("expected/eza/{slug}-theme.yml")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "eza", "install.sh.tmpl"),
            fixture_text(&format!("expected/eza/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "eza", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

/// Every non-comment line is `key:`, `key: {...}` with quoted `#RRGGBB`
/// colours, or `key: true`; indentation is two spaces, never tabs.
#[test]
fn theme_is_yaml_shaped() {
    let out = render_export("ember", "eza", "theme.yml.tmpl");
    let mut colours = 0;
    for line in out.lines() {
        if line.starts_with('#') {
            continue;
        }
        assert!(!line.contains('\t'), "{line:?}");
        let indent = line.len() - line.trim_start().len();
        assert!(indent % 2 == 0, "{line:?}");
        let body = line.trim_start();
        let (key, value) = body.split_once(':').unwrap_or_else(|| panic!("{line:?}"));
        assert!(
            key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{line:?}"
        );
        let value = value.trim();
        if value.is_empty() || value == "true" {
            continue;
        }
        assert!(value.starts_with('{') && value.ends_with('}'), "{line:?}");
        for field in value[1..value.len() - 1].split(',') {
            let (name, v) = field
                .trim()
                .split_once(": ")
                .unwrap_or_else(|| panic!("{line:?}"));
            match name {
                "foreground" | "background" => {
                    let hex = v.strip_prefix("\"#").and_then(|h| h.strip_suffix('"'));
                    assert!(
                        hex.is_some_and(
                            |h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit())
                        ),
                        "{line:?}"
                    );
                    colours += 1;
                }
                "is_bold" | "is_underline" => assert_eq!(v, "true", "{line:?}"),
                other => panic!("unknown field {other} in {line:?}"),
            }
        }
    }
    assert!(colours >= 50, "{out}");
}

#[test]
fn apply_replaces_existing_theme_with_warning() {
    let env = TestEnv::new();
    let theme = env.write(".config/eza/theme.yml", "colourful: false\n");
    let report = apply_only(&env, &palette("ember"), "eza").unwrap();
    let t = report.get("eza").unwrap();
    assert!(
        matches!(t.status, TargetStatus::Applied(_)),
        "{:?}",
        t.status
    );
    assert!(
        t.warnings
            .iter()
            .any(|w| w.contains("replaced your file; irida undo restores it")),
        "{:?}",
        t.warnings
    );
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "eza", "theme.yml.tmpl")
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&theme).unwrap(), "colourful: false\n");
}
