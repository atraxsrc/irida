mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::TargetStatus;

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "alacritty", "irida.toml.tmpl"),
            fixture_text(&format!("expected/alacritty/{slug}.toml")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "alacritty", "install.sh.tmpl"),
            fixture_text(&format!("expected/alacritty/{slug}-install.sh")),
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
        render_export("ember", "alacritty", "install.sh.tmpl"),
    )
    .unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

/// The theme parses as TOML with the five colour tables, every value a
/// quoted `#RRGGBB`.
#[test]
fn theme_is_toml_with_colour_tables() {
    let out = render_export("ember", "alacritty", "irida.toml.tmpl");
    let doc: toml::Table = toml::from_str(&out).unwrap_or_else(|e| panic!("{e}\n{out}"));
    let colors = doc["colors"].as_table().expect("[colors]");
    for table in ["primary", "cursor", "selection", "normal", "bright"] {
        let t = colors[table]
            .as_table()
            .unwrap_or_else(|| panic!("[colors.{table}]"));
        assert!(!t.is_empty(), "{table}");
        for (k, v) in t {
            let s = v
                .as_str()
                .unwrap_or_else(|| panic!("{table}.{k} is not a string"));
            assert!(
                s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit()),
                "{table}.{k} = {s:?}"
            );
        }
    }
    for name in [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ] {
        assert!(colors["normal"].get(name).is_some(), "normal.{name}");
        assert!(colors["bright"].get(name).is_some(), "bright.{name}");
    }
}

#[test]
fn apply_replaces_import_with_note_and_undo_restores() {
    let env = TestEnv::new();
    let text = "[general]\nimport = [\"~/mine.toml\"]\n\n[font]\nsize = 12\n";
    let conf = env.write(".config/alacritty/alacritty.toml", text);
    let report = apply_only(&env, &palette("ember"), "alacritty").unwrap();
    let t = report.get("alacritty").unwrap();
    assert!(
        matches!(t.status, TargetStatus::Applied(_)),
        "{:?}",
        t.status
    );
    assert!(
        t.warnings
            .iter()
            .any(|w| w.contains("replaced your import line; irida undo restores it")),
        "{:?}",
        t.warnings
    );
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[general]\nimport = [\"~/.config/alacritty/themes/irida.toml\"]\n\n[font]\nsize = 12\n"
    );
    let theme = env.dirs.home.join(".config/alacritty/themes/irida.toml");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "alacritty", "irida.toml.tmpl")
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
    assert!(!theme.exists());
}

#[test]
fn apply_creates_general_section_when_missing() {
    let env = TestEnv::new();
    let text = "[font]\nsize = 12\n";
    let conf = env.write(".config/alacritty/alacritty.toml", text);
    let report = apply_only(&env, &palette("ember"), "alacritty").unwrap();
    let t = report.get("alacritty").unwrap();
    assert!(
        !t.warnings.iter().any(|w| w.contains("replaced your")),
        "{:?}",
        t.warnings
    );
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[font]\nsize = 12\n[general]\nimport = [\"~/.config/alacritty/themes/irida.toml\"]\n"
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
}

#[test]
fn multi_line_import_array_is_replaced_whole() {
    let env = TestEnv::new();
    let text = "[general]\nimport = [\n  \"~/a.toml\",\n]\n\n[font]\nsize = 12\n";
    let conf = env.write(".config/alacritty/alacritty.toml", text);
    apply_only(&env, &palette("ember"), "alacritty").unwrap();
    let after = fs::read_to_string(&conf).unwrap();
    assert_eq!(
        after,
        "[general]\nimport = [\"~/.config/alacritty/themes/irida.toml\"]\n\n[font]\nsize = 12\n"
    );
    toml::from_str::<toml::Table>(&after).unwrap_or_else(|e| panic!("{e}\n{after}"));
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
}

#[test]
fn install_script_never_duplicates_general() {
    let script = render_export("ember", "alacritty", "install.sh.tmpl");
    assert!(script.contains("\\[general\\]"), "{script}");
}

#[test]
fn notes_colour_tables_that_override_the_theme() {
    let env = TestEnv::new();
    env.write(
        ".config/alacritty/alacritty.toml",
        "[font]\nsize = 12\n\n[colors.primary]\nbackground = \"#000000\"\n",
    );
    let report = apply_only(&env, &palette("ember"), "alacritty").unwrap();
    let w = &report.get("alacritty").unwrap().warnings;
    assert!(
        w.iter()
            .any(|w| w
                .ends_with("alacritty.toml: your own [colors] settings override the Irida theme")),
        "{w:?}"
    );

    let env = TestEnv::new();
    env.write(".config/alacritty/alacritty.toml", "[font]\nsize = 12\n");
    let report = apply_only(&env, &palette("ember"), "alacritty").unwrap();
    let w = &report.get("alacritty").unwrap().warnings;
    assert!(!w.iter().any(|w| w.contains("override")), "{w:?}");
}

#[test]
fn every_theme_table_is_under_colors() {
    for line in common::target_file("alacritty", "irida.toml.tmpl").lines() {
        if line.starts_with('[') {
            assert!(line.starts_with("[colors."), "{line}");
        }
    }
}
