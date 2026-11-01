mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::TargetStatus;

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "wezterm", "irida.toml.tmpl"),
            fixture_text(&format!("expected/wezterm/{slug}.toml")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "wezterm", "install.sh.tmpl"),
            fixture_text(&format!("expected/wezterm/{slug}-install.sh")),
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
        render_export("ember", "wezterm", "install.sh.tmpl"),
    )
    .unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

/// The scheme is TOML with 8 ansi and 8 bright colours and is named after
/// the file: the slug on export, `irida` on apply.
#[test]
fn scheme_is_toml_and_named() {
    for (slug, out, name) in [
        (
            "ember",
            render_export("ember", "wezterm", "irida.toml.tmpl"),
            "ember",
        ),
        (
            "emberday",
            render_apply("emberday", "wezterm", "irida.toml.tmpl"),
            "irida",
        ),
    ] {
        let v: toml::Value = toml::from_str(&out).unwrap_or_else(|e| panic!("{slug}: {e}"));
        let colors = v
            .get("colors")
            .and_then(|c| c.as_table())
            .expect("[colors]");
        for key in ["ansi", "brights"] {
            let list = colors.get(key).and_then(|a| a.as_array()).expect(key);
            assert_eq!(list.len(), 8, "{slug} {key}");
            for c in list {
                let s = c.as_str().unwrap();
                assert!(s.len() == 7 && s.starts_with('#'), "{slug} {key}: {s}");
            }
        }
        assert!(colors.contains_key("tab_bar"), "{slug}");
        assert_eq!(
            v.get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str()),
            Some(name),
            "{slug}"
        );
        assert!(
            out.contains(&format!("config.color_scheme = \"{name}\"")),
            "{slug}"
        );
    }
}

/// Apply writes colors/irida.toml, leaves wezterm.lua alone and says what to
/// add to it; undo removes the scheme.
#[test]
fn apply_writes_scheme_and_leaves_wezterm_lua_alone() {
    let env = TestEnv::new();
    let lua = env.write(
        ".config/wezterm/wezterm.lua",
        "local config = wezterm.config_builder()\nreturn config\n",
    );
    let report = apply_only(&env, &palette("ember"), "wezterm").unwrap();
    let t = report.get("wezterm").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    assert!(
        t.warnings
            .iter()
            .any(|w| w.contains("config.color_scheme = \"irida\"")),
        "{:?}",
        t.warnings
    );
    let scheme = env.dirs.home.join(".config/wezterm/colors/irida.toml");
    assert_eq!(
        fs::read_to_string(&scheme).unwrap(),
        render_apply("ember", "wezterm", "irida.toml.tmpl")
    );
    assert_eq!(
        fs::read_to_string(&lua).unwrap(),
        "local config = wezterm.config_builder()\nreturn config\n"
    );
    undo(&env);
    assert!(!scheme.exists());
}
