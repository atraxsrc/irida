//! The install scripts Irida writes itself: those with a snapshot under
//! `fixtures/expected/`. The others are copies of the reference rice repos'
//! scripts and stay byte for byte as they are there.

mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, fixture, render_export};

/// Each target with its own install script, with the script rendered for Ember.
fn own_scripts() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(fixture("expected"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("ember-install.sh").is_file())
        .map(|p| {
            let target = p.file_name().unwrap().to_string_lossy().into_owned();
            let script = render_export("ember", &target, "install.sh.tmpl");
            (target, script)
        })
        .collect();
    out.sort();
    assert!(
        out.len() >= 9,
        "{:?}",
        out.iter().map(|(t, _)| t).collect::<Vec<_>>()
    );
    out
}

#[test]
fn no_script_names_bin_cat() {
    for (target, script) in own_scripts() {
        assert!(!script.contains("/bin/cat"), "{target}");
    }
}

#[test]
fn backups_go_through_the_backup_function() {
    for (target, script) in own_scripts() {
        for line in script.lines() {
            assert!(
                !(line.contains("cp ") && line.contains(".bak")),
                "{target}: {line}"
            );
        }
    }
}

#[test]
fn a_backup_never_replaces_an_older_one() {
    let env = TestEnv::new();
    let rice = env.tmp.path().join("rice/eza");
    fs::create_dir_all(&rice).unwrap();
    fs::write(
        rice.join("install.sh"),
        render_export("ember", "eza", "install.sh.tmpl"),
    )
    .unwrap();
    fs::write(
        rice.join("theme.yml"),
        render_export("ember", "eza", "theme.yml.tmpl"),
    )
    .unwrap();
    let theme = env.write(".config/eza/theme.yml", "old\n");
    let dest = theme.parent().unwrap().to_path_buf();
    fs::write(dest.join("theme.yml.bak"), "older\n").unwrap();

    let run = || {
        let out = Command::new("/bin/bash")
            .arg(rice.join("install.sh"))
            .env_clear()
            .env("HOME", &env.dirs.home)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };

    let stdout = run();
    assert!(stdout.contains("backup: theme.yml.bak-1"), "{stdout}");
    assert_eq!(
        fs::read_to_string(dest.join("theme.yml.bak")).unwrap(),
        "older\n"
    );
    assert_eq!(
        fs::read_to_string(dest.join("theme.yml.bak-1")).unwrap(),
        "old\n"
    );

    // Running it again over an unchanged theme backs nothing up.
    let stdout = run();
    assert!(!stdout.contains("backup"), "{stdout}");
    assert!(!dest.join("theme.yml.bak-2").exists());

    fs::write(&theme, "mine\n").unwrap();
    run();
    assert_eq!(
        fs::read_to_string(dest.join("theme.yml.bak-1")).unwrap(),
        "old\n"
    );
    assert_eq!(
        fs::read_to_string(dest.join("theme.yml.bak-2")).unwrap(),
        "mine\n"
    );
}
