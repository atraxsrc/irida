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
            render_export(slug, "kitty", "irida.conf.tmpl"),
            fixture_text(&format!("expected/kitty/{slug}.conf")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "kitty", "install.sh.tmpl"),
            fixture_text(&format!("expected/kitty/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "kitty", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn selector_value() {
    let source = common::target_file("kitty", "include.tmpl");
    let p = palette("ember");
    assert_eq!(
        Renderer::new(&p).render("include.tmpl", &source).unwrap(),
        "irida.conf"
    );
    assert_eq!(
        Renderer::for_export(&p)
            .render("include.tmpl", &source)
            .unwrap(),
        "ember.conf"
    );
}

/// Every non-comment line is `key #RRGGBB`.
#[test]
fn theme_lines_are_key_and_hex() {
    let out = render_export("ember", "kitty", "irida.conf.tmpl");
    let mut n = 0;
    for line in out.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let words: Vec<&str> = line.split(' ').collect();
        assert_eq!(words.len(), 2, "{line:?}");
        let v = words[1];
        assert!(
            v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit()),
            "{line:?}"
        );
        n += 1;
    }
    assert_eq!(n, 31, "{out}");
    for i in 0..16 {
        assert!(
            out.lines().any(|l| l.starts_with(&format!("color{i} #"))),
            "color{i}"
        );
    }
}

#[test]
fn apply_adds_include_after_users_and_undo_removes_it() {
    let env = TestEnv::new();
    let conf = env.write(
        ".config/kitty/kitty.conf",
        "include current-theme.conf\nfont_size 12\n",
    );
    let report = apply_only(&env, &palette("ember"), "kitty").unwrap();
    assert!(
        matches!(
            report.get("kitty").unwrap().status,
            TargetStatus::Applied(_)
        ),
        "{:?}",
        report.get("kitty")
    );
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "include current-theme.conf\nfont_size 12\ninclude irida.conf\n"
    );
    let theme = env.dirs.home.join(".config/kitty/irida.conf");
    assert_eq!(
        fs::read_to_string(&theme).unwrap(),
        render_apply("ember", "kitty", "irida.conf.tmpl")
    );
    apply_only(&env, &palette("emberday"), "kitty").unwrap();
    let text = fs::read_to_string(&conf).unwrap();
    assert_eq!(text.matches("include irida.conf").count(), 1, "{text}");
    undo(&env);
    undo(&env);
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "include current-theme.conf\nfont_size 12\n"
    );
    assert!(!theme.exists());
}
