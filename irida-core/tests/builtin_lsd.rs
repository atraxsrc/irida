mod common;

use common::{TestEnv, apply_only, fixture_text, palette, render_export, rice_text};
use irida_core::apply::{SkipReason, TargetStatus};

#[test]
fn static_files_match_rice() {
    for slug in ["ember", "emberday"] {
        for (template, rel) in [
            ("config.yaml", "lsd/config.yaml"),
            ("install.sh.tmpl", "lsd/install.sh"),
        ] {
            assert_eq!(
                render_export(slug, "lsd", template),
                rice_text(slug, rel),
                "{slug} {rel}"
            );
        }
    }
}

#[test]
fn colors_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "lsd", "colors.yaml.tmpl"),
            fixture_text(&format!("expected/lsd/{slug}-colors.yaml")),
            "{slug}"
        );
    }
}

#[test]
fn colors_are_rgb_arrays() {
    let out = render_export("ember", "lsd", "colors.yaml.tmpl");
    for line in out.lines() {
        if line.starts_with('#') || line.ends_with(':') {
            continue;
        }
        assert!(is_rgb_line(line), "{line}");
    }
    assert!(out.lines().any(|l| l == "user: [232,113,58]"), "{out}");
}

/// Whether `line` matches `^\s*[a-z-]+: \[\d{1,3},\d{1,3},\d{1,3}\]$`.
fn is_rgb_line(line: &str) -> bool {
    let t = line.trim_start();
    let Some((key, value)) = t.split_once(": ") else {
        return false;
    };
    let key_ok = !key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        return false;
    };
    let parts: Vec<&str> = inner.split(',').collect();
    key_ok
        && parts.len() == 3
        && parts
            .iter()
            .all(|p| (1..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

#[test]
fn apply_creates_theme_once() {
    let env = TestEnv::new();
    let lsd = env.dirs.config.join("lsd");
    std::fs::create_dir_all(&lsd).unwrap();
    let report = apply_only(&env, &palette("ember"), "lsd").unwrap();
    assert!(matches!(
        report.get("lsd").unwrap().status,
        TargetStatus::Applied(_)
    ));
    assert_eq!(
        std::fs::read_to_string(lsd.join("config.yaml")).unwrap(),
        "color:\n  theme: custom\n"
    );

    let env = TestEnv::new();
    let conf = env.write(".config/lsd/config.yaml", "color:\n  when: auto\n");
    let report = apply_only(&env, &palette("ember"), "lsd").unwrap();
    let t = report.get("lsd").unwrap();
    assert!(
        !matches!(t.status, TargetStatus::Skipped(SkipReason::NotInstalled)),
        "{:?}",
        t.status
    );
    assert_eq!(
        std::fs::read_to_string(conf).unwrap(),
        "color:\n  when: auto\n"
    );
    assert!(
        t.warnings
            .iter()
            .any(|w| w.contains("add `theme: custom` under `color:`")),
        "{:?}",
        t.warnings
    );
}
