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
            render_export(slug, "helix", "irida.toml.tmpl"),
            fixture_text(&format!("expected/helix/{slug}.toml")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "helix", "install.sh.tmpl"),
            fixture_text(&format!("expected/helix/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "helix", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn selector_value() {
    let source = common::target_file("helix", "theme.tmpl");
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

fn is_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// The theme parses as TOML, `[palette]` is its last table and holds only
/// hex colours, and every scope refers to a palette name.
#[test]
fn theme_is_toml_over_a_palette() {
    let out = render_export("ember", "helix", "irida.toml.tmpl");
    let doc: toml::Table = toml::from_str(&out).unwrap_or_else(|e| panic!("{e}\n{out}"));
    let palette = doc["palette"].as_table().expect("[palette]");
    assert!(
        out.trim_end()
            .lines()
            .last()
            .is_some_and(|l| l.contains('='))
    );
    assert_eq!(
        out.lines().filter(|l| l.starts_with('[')).count(),
        1,
        "{out}"
    );
    for (k, v) in palette {
        assert!(v.as_str().is_some_and(is_hex), "palette.{k} = {v}");
    }
    let names: Vec<&str> = palette.keys().map(String::as_str).collect();
    let refers = |v: &toml::Value| -> bool {
        match v {
            toml::Value::String(s) => names.contains(&s.as_str()),
            toml::Value::Table(t) => {
                ["fg", "bg"].iter().all(|f| {
                    t.get(*f)
                        .is_none_or(|c| c.as_str().is_some_and(|s| names.contains(&s)))
                }) && t.get("underline").is_none_or(|u| {
                    u.get("color")
                        .is_some_and(|c| c.as_str().is_some_and(|s| names.contains(&s)))
                })
            }
            _ => false,
        }
    };
    let mut scopes = 0;
    for (k, v) in &doc {
        if k == "palette" {
            continue;
        }
        assert!(refers(v), "{k} = {v}");
        scopes += 1;
    }
    assert!(scopes >= 40, "{scopes} scopes");
    for key in [
        "ui.background",
        "ui.text",
        "comment",
        "keyword",
        "string",
        "diagnostic.error",
    ] {
        assert!(doc.contains_key(key), "{key}");
    }
}

#[test]
fn apply_puts_theme_before_first_section_and_undo_restores() {
    let env = TestEnv::new();
    let text = "[editor]\nline-number = \"relative\"\n";
    let conf = env.write(".config/helix/config.toml", text);
    let report = apply_only(&env, &palette("ember"), "helix").unwrap();
    assert!(
        matches!(
            report.get("helix").unwrap().status,
            TargetStatus::Applied(_)
        ),
        "{:?}",
        report.get("helix")
    );
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "theme = \"irida\"\n[editor]\nline-number = \"relative\"\n"
    );
    let theme = env.dirs.home.join(".config/helix/themes/irida.toml");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "helix", "irida.toml.tmpl")
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
    assert!(!theme.exists());
}

#[test]
fn apply_replaces_existing_top_level_theme() {
    let env = TestEnv::new();
    let text = "theme = \"onedark\"\n\n[editor]\ntheme = \"not-this-one\"\n";
    let conf = env.write(".config/helix/config.toml", text);
    apply_only(&env, &palette("ember"), "helix").unwrap();
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "theme = \"irida\"\n\n[editor]\ntheme = \"not-this-one\"\n"
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
}
