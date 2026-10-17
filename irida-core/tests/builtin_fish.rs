mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::TargetStatus;

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "fish", "irida.fish.tmpl"),
            fixture_text(&format!("expected/fish/{slug}.fish")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "fish", "install.sh.tmpl"),
            fixture_text(&format!("expected/fish/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "fish", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

fn is_bare_hex(s: &str) -> bool {
    s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[test]
fn values_are_bare_hex() {
    let out = render_export("ember", "fish", "irida.fish.tmpl");
    let mut seen = 0;
    for line in out.lines().filter(|l| l.starts_with("set -g ")) {
        let words: Vec<&str> = line.split_whitespace().collect();
        assert!(
            words[2].starts_with("fish_color_") || words[2].starts_with("fish_pager_color_"),
            "{line}"
        );
        for w in &words[3..] {
            assert!(
                is_bare_hex(w) || *w == "--bold" || *w == "--underline" || *w == "--background",
                "{line}"
            );
        }
        seen += 1;
    }
    assert!(seen >= 20, "{out}");
}

#[test]
fn apply_writes_conf_d() {
    let env = TestEnv::new();
    fs::create_dir_all(env.dirs.home.join(".config/fish")).unwrap();
    let report = apply_only(&env, &palette("ember"), "fish").unwrap();
    assert!(
        matches!(report.get("fish").unwrap().status, TargetStatus::Applied(_)),
        "{:?}",
        report.get("fish")
    );
    let file = env.dirs.home.join(".config/fish/conf.d/irida.fish");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        render_apply("ember", "fish", "irida.fish.tmpl")
    );
    undo(&env);
    assert!(!file.exists());
}

#[test]
fn syntax_ok_when_fish_present() {
    let Ok(out) = Command::new("fish").arg("--version").output() else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let env = TestEnv::new();
    let file = env.tmp.path().join("irida.fish");
    fs::write(&file, render_export("ember", "fish", "irida.fish.tmpl")).unwrap();
    let status = Command::new("fish")
        .arg("--no-execute")
        .arg(&file)
        .status()
        .unwrap();
    assert!(status.success());
}
