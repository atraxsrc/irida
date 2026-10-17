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
            render_export(slug, "yazi", "flavor.toml.tmpl"),
            fixture_text(&format!("expected/yazi/{slug}-flavor.toml")),
            "{slug}"
        );
        assert_eq!(
            render_export(slug, "yazi", "tmtheme.xml.tmpl"),
            fixture_text(&format!("expected/yazi/{slug}-tmtheme.xml")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "yazi", "install.sh.tmpl"),
            fixture_text(&format!("expected/yazi/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "yazi", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn flavor_parses() {
    let flavor = render_apply("ember", "yazi", "flavor.toml.tmpl");
    let doc: toml::Table = toml::from_str(&flavor).unwrap_or_else(|e| panic!("{e}\n{flavor}"));
    for table in ["mgr", "tabs", "mode", "status", "filetype"] {
        assert!(doc.contains_key(table), "[{table}]");
    }
    let xml = render_apply("ember", "yazi", "tmtheme.xml.tmpl");
    assert!(xml.starts_with("<?xml"), "{xml}");
    assert_eq!(xml.matches("<plist").count(), 1);
    assert_eq!(xml.matches("</plist>").count(), 1);
}

#[test]
fn selector_value() {
    let source = common::target_file("yazi", "selector.tmpl");
    let p = palette("ember");
    assert_eq!(
        Renderer::new(&p).render("selector.tmpl", &source).unwrap(),
        "\"irida\""
    );
    assert_eq!(
        Renderer::for_export(&p)
            .render("selector.tmpl", &source)
            .unwrap(),
        "\"ember\""
    );
}

#[test]
fn apply_sets_dark_and_light_and_undo_restores() {
    let env = TestEnv::new();
    let text = "[flavor]\ndark = \"catppuccin\"\n";
    let conf = env.write(".config/yazi/theme.toml", text);
    let report = apply_only(&env, &palette("ember"), "yazi").unwrap();
    let t = report.get("yazi").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[flavor]\ndark = \"irida\"\nlight = \"irida\"\n"
    );
    assert!(
        t.warnings
            .iter()
            .any(|w| w.contains("replaced your dark line")),
        "{:?}",
        t.warnings
    );
    let flavor = env.dirs.home.join(".config/yazi/flavors/irida.yazi");
    let mut files: Vec<String> = fs::read_dir(&flavor)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files, ["flavor.toml", "tmtheme.xml"]);
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
    assert!(!flavor.join("flavor.toml").exists());
    assert!(!flavor.join("tmtheme.xml").exists());
}

#[test]
fn apply_without_theme_toml_creates_flavor_section() {
    let env = TestEnv::new();
    fs::create_dir_all(env.dirs.home.join(".config/yazi")).unwrap();
    apply_only(&env, &palette("ember"), "yazi").unwrap();
    let conf = env.dirs.home.join(".config/yazi/theme.toml");
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[flavor]\ndark = \"irida\"\nlight = \"irida\"\n"
    );
    undo(&env);
    assert!(!conf.exists());
}

/// The flavor uses `[mgr]` (25.5.28), `[cmp]` (25.2.26) and `url` filetype
/// rules (25.12.29), so 25.12.29 is the version every user-facing text names.
#[test]
fn stated_floor_matches_flavor() {
    let floor = "25.12.29";
    let readme = fs::read_to_string(common::fixture("../../../README.md")).unwrap();
    let row = readme.lines().find(|l| l.starts_with("| yazi |")).unwrap();
    assert!(row.contains(floor), "{row}");
    assert!(common::target_file("yazi", "target.toml").contains(floor));
    assert!(render_export("ember", "yazi", "install.sh.tmpl").contains(floor));
    for old in [
        row.to_string(),
        render_export("ember", "yazi", "install.sh.tmpl"),
    ] {
        assert!(!old.contains("0.4 or newer"), "{old}");
    }
}
