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
            render_export(slug, "foot", "irida.ini.tmpl"),
            fixture_text(&format!("expected/foot/{slug}.ini")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "foot", "install.sh.tmpl"),
            fixture_text(&format!("expected/foot/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "foot", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn selector_value() {
    let source = common::target_file("foot", "include.tmpl");
    let p = palette("ember");
    assert_eq!(
        Renderer::new(&p).render("include.tmpl", &source).unwrap(),
        "~/.config/foot/irida.ini"
    );
    assert_eq!(
        Renderer::for_export(&p)
            .render("include.tmpl", &source)
            .unwrap(),
        "~/.config/foot/ember.ini"
    );
}

fn is_bare_hex(s: &str) -> bool {
    s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Both colour sections carry the same keys; every value is bare RRGGBB,
/// two of them for `cursor` and `jump-labels`.
#[test]
fn theme_has_both_sections_with_bare_hex() {
    let out = render_export("ember", "foot", "irida.ini.tmpl");
    let headers: Vec<&str> = out.lines().filter(|l| l.starts_with('[')).collect();
    assert_eq!(headers, ["[colors-dark]", "[colors-light]"], "{out}");
    let mut keys = 0;
    for line in out
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with('[') && !l.is_empty())
    {
        let (key, value) = line.split_once('=').unwrap_or_else(|| panic!("{line:?}"));
        let values: Vec<&str> = value.split(' ').collect();
        match key {
            "cursor" | "jump-labels" => assert_eq!(values.len(), 2, "{line:?}"),
            _ => assert_eq!(values.len(), 1, "{line:?}"),
        }
        assert!(values.iter().all(|v| is_bare_hex(v)), "{line:?}");
        keys += 1;
    }
    assert_eq!(keys, 2 * 23, "{out}");
}

#[test]
fn apply_puts_include_first_and_undo_restores() {
    let env = TestEnv::new();
    let text = "[main]\nfont=monospace:size=11\n";
    let conf = env.write(".config/foot/foot.ini", text);
    let report = apply_only(&env, &palette("ember"), "foot").unwrap();
    assert!(
        matches!(report.get("foot").unwrap().status, TargetStatus::Applied(_)),
        "{:?}",
        report.get("foot")
    );
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "include=~/.config/foot/irida.ini\n[main]\nfont=monospace:size=11\n"
    );
    let theme = env.dirs.home.join(".config/foot/irida.ini");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "foot", "irida.ini.tmpl")
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
    assert!(!theme.exists());
}

#[test]
fn apply_keeps_users_own_include() {
    let env = TestEnv::new();
    let text = "include=~/.config/foot/mine.ini\n[main]\nfont=monospace\n";
    let conf = env.write(".config/foot/foot.ini", text);
    apply_only(&env, &palette("ember"), "foot").unwrap();
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "include=~/.config/foot/mine.ini\ninclude=~/.config/foot/irida.ini\n[main]\nfont=monospace\n"
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
}

#[test]
fn include_follows_xdg_config_home() {
    let mut env = TestEnv::new();
    env.dirs.config = env.dirs.home.join("cfg");
    let conf = env.write("cfg/foot/foot.ini", "[main]\nfont=monospace\n");
    apply_only(&env, &palette("ember"), "foot").unwrap();
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "include=~/cfg/foot/irida.ini\n[main]\nfont=monospace\n"
    );
}
