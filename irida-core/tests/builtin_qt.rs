//! qt5ct and qt6ct: the same scheme file, one target per tool; the rice
//! folder `qt/` comes from qt6ct alone.

mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::TargetStatus;
use irida_core::template::Renderer;

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        let want = fixture_text(&format!("expected/qt6ct/{slug}.conf"));
        assert_eq!(
            render_export(slug, "qt6ct", "irida.conf.tmpl"),
            want,
            "{slug}"
        );
        assert_eq!(
            render_export(slug, "qt5ct", "irida.conf.tmpl"),
            want,
            "{slug} qt5ct"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "qt6ct", "install.sh.tmpl"),
            fixture_text(&format!("expected/qt6ct/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "qt6ct", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn key_values() {
    let p = palette("ember");
    for tool in ["qt5ct", "qt6ct"] {
        let path = common::target_file(tool, "path.tmpl");
        assert_eq!(
            Renderer::new(&p).render("path.tmpl", &path).unwrap(),
            format!("~/.config/{tool}/colors/irida.conf")
        );
        assert_eq!(
            Renderer::for_export(&p).render("path.tmpl", &path).unwrap(),
            format!("~/.config/{tool}/colors/ember.conf")
        );
        let custom = common::target_file(tool, "custom.tmpl");
        assert_eq!(
            Renderer::new(&p).render("custom.tmpl", &custom).unwrap(),
            "true"
        );
    }
}

/// Three lists of 21 lowercase `#rrggbb` colours under [ColorScheme].
#[test]
fn scheme_has_21_roles_per_state() {
    for slug in ["ember", "emberday"] {
        let out = render_export(slug, "qt6ct", "irida.conf.tmpl");
        let body = out.split("[ColorScheme]\n").nth(1).expect("[ColorScheme]");
        let mut keys = Vec::new();
        for line in body.lines() {
            let (key, list) = line.split_once('=').unwrap_or_else(|| panic!("{line:?}"));
            keys.push(key);
            let colours: Vec<&str> = list.split(", ").collect();
            assert_eq!(colours.len(), 21, "{slug} {key}");
            for c in colours {
                assert!(
                    c.len() == 7
                        && c.starts_with('#')
                        && c[1..]
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                    "{slug} {key}: {c}"
                );
            }
        }
        assert_eq!(
            keys,
            ["active_colors", "disabled_colors", "inactive_colors"]
        );
    }
}

/// Both keys land under [Appearance], the user's path line is replaced with a
/// note, [Fonts] is untouched, and undo restores the file byte for byte.
#[test]
fn apply_selects_the_scheme_under_appearance() {
    let env = TestEnv::new();
    let text = "[Appearance]\ncolor_scheme_path=/usr/share/qt6ct/colors/airy.conf\ncustom_palette=false\nstyle=Fusion\n\n[Fonts]\nfixed=\"Maple Mono,10\"\n";
    let conf = env.write(".config/qt6ct/qt6ct.conf", text);
    let report = apply_only(&env, &palette("ember"), "qt6ct").unwrap();
    let t = report.get("qt6ct").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[Appearance]\ncolor_scheme_path=~/.config/qt6ct/colors/irida.conf\ncustom_palette=true\nstyle=Fusion\n\n[Fonts]\nfixed=\"Maple Mono,10\"\n"
    );
    assert!(
        t.warnings
            .iter()
            .any(|w| w.contains("replaced your color_scheme_path line")),
        "{:?}",
        t.warnings
    );
    assert!(
        t.warnings
            .iter()
            .any(|w| w.contains("QT_QPA_PLATFORMTHEME=qt6ct")),
        "{:?}",
        t.warnings
    );
    let scheme = env.dirs.home.join(".config/qt6ct/colors/irida.conf");
    assert_eq!(
        fs::read_to_string(&scheme).unwrap(),
        render_apply("ember", "qt6ct", "irida.conf.tmpl")
    );
    assert!(!env.dirs.home.join(".config/qt5ct").exists());
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
    assert!(!scheme.exists());
}

/// A qt5ct.conf without [Appearance] gets the section added at the end.
#[test]
fn apply_adds_appearance_when_missing() {
    let env = TestEnv::new();
    let conf = env.write(
        ".config/qt5ct/qt5ct.conf",
        "[Fonts]\nfixed=\"Maple Mono,10\"\n",
    );
    apply_only(&env, &palette("emberday"), "qt5ct").unwrap();
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[Fonts]\nfixed=\"Maple Mono,10\"\n[Appearance]\ncolor_scheme_path=~/.config/qt5ct/colors/irida.conf\ncustom_palette=true\n"
    );
    undo(&env);
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[Fonts]\nfixed=\"Maple Mono,10\"\n"
    );
}

/// The exported install script sets up every tool whose folder exists and
/// edits each conf in place, once.
#[test]
fn install_script_sets_both_keys() {
    let env = TestEnv::new();
    let rice = env.tmp.path().join("rice/qt");
    fs::create_dir_all(&rice).unwrap();
    fs::write(
        rice.join("install.sh"),
        render_export("ember", "qt6ct", "install.sh.tmpl"),
    )
    .unwrap();
    fs::write(rice.join("ember.conf"), "x\n").unwrap();
    let qt6 = env.write(
        ".config/qt6ct/qt6ct.conf",
        "[Appearance]\nstyle=Fusion\ncustom_palette=false\n\n[Fonts]\nfixed=\"Maple Mono,10\"\n",
    );
    let qt5 = env.write(
        ".config/qt5ct/qt5ct.conf",
        "[Fonts]\nfixed=\"Maple Mono,10\"\n",
    );
    let run = || {
        let out = Command::new("/bin/bash")
            .arg(rice.join("install.sh"))
            .env_clear()
            .env("HOME", &env.dirs.home)
            .env("TMPDIR", env.tmp.path())
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let stdout = run();
    assert!(stdout.contains("backup: qt6ct.conf.bak"), "{stdout}");
    assert_eq!(
        fs::read_to_string(&qt6).unwrap(),
        "[Appearance]\nstyle=Fusion\ncustom_palette=true\ncolor_scheme_path=~/.config/qt6ct/colors/ember.conf\n\n[Fonts]\nfixed=\"Maple Mono,10\"\n"
    );
    assert_eq!(
        fs::read_to_string(&qt5).unwrap(),
        "[Fonts]\nfixed=\"Maple Mono,10\"\n[Appearance]\ncolor_scheme_path=~/.config/qt5ct/colors/ember.conf\ncustom_palette=true\n"
    );
    for tool in ["qt5ct", "qt6ct"] {
        assert_eq!(
            fs::read_to_string(
                env.dirs
                    .home
                    .join(format!(".config/{tool}/colors/ember.conf"))
            )
            .unwrap(),
            "x\n"
        );
    }
    let stdout = run();
    assert!(stdout.contains("qt6ct.conf already selects it"), "{stdout}");
    assert!(!stdout.contains("backup"), "{stdout}");
}
