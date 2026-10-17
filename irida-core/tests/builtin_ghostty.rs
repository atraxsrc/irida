mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::TargetStatus;
use irida_core::template::Renderer;

#[test]
fn theme_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "ghostty", "ghostty.theme.tmpl"),
            fixture_text(&format!("expected/ghostty/{slug}.theme")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "ghostty", "install.sh.tmpl"),
            fixture_text(&format!("expected/ghostty/{slug}-install.sh")),
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
        render_export("ember", "ghostty", "install.sh.tmpl"),
    )
    .unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn selector_value() {
    let source = common::target_file("ghostty", "theme.tmpl");
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

#[test]
fn theme_has_sixteen_hex_palette_lines() {
    let out = render_export("ember", "ghostty", "ghostty.theme.tmpl");
    let palette_lines: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with("palette = "))
        .collect();
    assert_eq!(palette_lines.len(), 16, "{out}");
    for (i, line) in palette_lines.iter().enumerate() {
        let value = line
            .strip_prefix(&format!("palette = {i}="))
            .unwrap_or_else(|| panic!("{line}"));
        assert!(
            value.len() == 7
                && value.starts_with('#')
                && value[1..].bytes().all(|b| b.is_ascii_hexdigit()),
            "{line}"
        );
    }
    for key in [
        "background",
        "foreground",
        "cursor-color",
        "cursor-text",
        "selection-background",
        "selection-foreground",
    ] {
        assert!(
            out.lines().any(|l| l.starts_with(&format!("{key} = #"))),
            "{key}: {out}"
        );
    }
}

#[test]
fn apply_writes_theme_and_selector() {
    let env = TestEnv::new();
    let conf = env.write(
        ".config/ghostty/config",
        "font-size = 13\ntheme = light:a,dark:b\n",
    );
    let report = apply_only(&env, &palette("ember"), "ghostty").unwrap();
    assert!(
        matches!(
            report.get("ghostty").unwrap().status,
            TargetStatus::Applied(_)
        ),
        "{:?}",
        report.get("ghostty")
    );
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "font-size = 13\ntheme = irida\n"
    );
    let theme = env.dirs.home.join(".config/ghostty/themes/irida");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "ghostty", "ghostty.theme.tmpl")
    );
    undo(&env);
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "font-size = 13\ntheme = light:a,dark:b\n"
    );
    assert!(!theme.exists());
}

#[test]
fn notes_colour_keys_that_override_the_theme() {
    let env = TestEnv::new();
    env.write(
        ".config/ghostty/config",
        "font-size = 13\nbackground = #000000\n# foreground = #ffffff\npalette = 0=#111111\n",
    );
    let report = apply_only(&env, &palette("ember"), "ghostty").unwrap();
    let w = &report.get("ghostty").unwrap().warnings;
    assert!(
        w.iter().any(|w| w.ends_with(
            "ghostty/config: your own background and palette settings override the Irida theme"
        )),
        "{w:?}"
    );

    let env = TestEnv::new();
    env.write(".config/ghostty/config", "font-size = 13\n");
    let report = apply_only(&env, &palette("ember"), "ghostty").unwrap();
    let w = &report.get("ghostty").unwrap().warnings;
    assert!(!w.iter().any(|w| w.contains("override")), "{w:?}");
}

#[test]
fn override_keys_are_the_theme_keys() {
    let env = TestEnv::new();
    let all = irida_core::target::load_targets(&env.dirs);
    let t = all.iter().find(|t| t.name == "ghostty").unwrap();
    let t = t.result.as_ref().unwrap();
    let selector = t.def.files.iter().find(|f| f.key.is_some()).unwrap();
    let mut keys: Vec<String> = Vec::new();
    for line in common::target_file("ghostty", "ghostty.theme.tmpl").lines() {
        if let Some((k, _)) = line.split_once(" = ")
            && !line.starts_with('#')
            && !keys.iter().any(|x| x == k)
        {
            keys.push(k.to_string());
        }
    }
    assert_eq!(selector.note_overrides, keys);
}
