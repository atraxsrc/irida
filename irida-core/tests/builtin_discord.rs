mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::{ApplyError, SkipReason, TargetStatus};

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "discord", "irida.theme.css.tmpl"),
            fixture_text(&format!("expected/discord/{slug}.theme.css")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "discord", "install.sh.tmpl"),
            fixture_text(&format!("expected/discord/{slug}-install.sh")),
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
        render_export("ember", "discord", "install.sh.tmpl"),
    )
    .unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

/// The Vencord header, one selector block for the palette's mode, every
/// declaration a custom property with a `#RRGGBB` (optionally `AA`) or
/// `transparent` value, and each property set once.
#[test]
fn theme_is_one_block_of_variables() {
    for (slug, out, name, selector) in [
        (
            "ember",
            render_export("ember", "discord", "irida.theme.css.tmpl"),
            "Ember",
            ".theme-dark:not(.custom-user-profile-theme), .visual-refresh.theme-dark, .visual-refresh .theme-dark {",
        ),
        (
            "emberday",
            render_apply("emberday", "discord", "irida.theme.css.tmpl"),
            "Irida",
            ".theme-light:not(.custom-user-profile-theme), .visual-refresh.theme-light, .visual-refresh .theme-light {",
        ),
    ] {
        assert!(out.starts_with("/**\n * @name "), "{slug}");
        assert!(out.contains(&format!(" * @name {name}\n")), "{slug}: {out}");
        assert_eq!(out.matches(" {\n").count(), 1, "{slug}");
        assert!(out.contains(selector), "{slug}");
        assert!(
            !out.contains(if slug == "ember" {
                "theme-light"
            } else {
                "theme-dark"
            }),
            "{slug}"
        );
        assert!(out.ends_with("}\n"), "{slug}");
        let mut names = Vec::new();
        for line in out.lines().filter(|l| l.trim_start().starts_with("--")) {
            let (prop, value) = line
                .trim()
                .split_once(": ")
                .unwrap_or_else(|| panic!("{line:?}"));
            let value = value
                .strip_suffix(';')
                .unwrap_or_else(|| panic!("{line:?}"));
            assert!(
                value == "transparent"
                    || ((value.len() == 7 || value.len() == 9)
                        && value.starts_with('#')
                        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())),
                "{slug}: {line:?}"
            );
            assert!(!names.contains(&prop), "{slug}: {prop} set twice");
            names.push(prop);
        }
        assert!(names.len() > 150, "{slug}: {}", names.len());
        for must in [
            "--background-base-lowest",
            "--chat-background-default",
            "--text-default",
            "--brand-500",
        ] {
            assert!(names.contains(&must), "{slug}: {must}");
        }
    }
}

/// Only the clients that are there get the file: a Flatpak Discord alone
/// gets its Vencord themes folder and nothing appears under ~/.config.
#[test]
fn apply_writes_only_the_installed_clients() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let Err(ApplyError::NothingToApply(results)) = apply_only(&env, &ember, "discord") else {
        panic!("expected nothing to apply");
    };
    let discord = results
        .iter()
        .find(|r| r.name == "discord")
        .expect("discord");
    assert_eq!(
        discord.status,
        TargetStatus::Skipped(SkipReason::NotInstalled)
    );

    fs::create_dir_all(env.dirs.home.join(".var/app/com.discordapp.Discord/config")).unwrap();
    let report = apply_only(&env, &ember, "discord").unwrap();
    let t = report.get("discord").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    let flatpak = env
        .dirs
        .home
        .join(".var/app/com.discordapp.Discord/config/Vencord/themes/irida.theme.css");
    assert_eq!(
        fs::read_to_string(&flatpak).unwrap(),
        render_apply("ember", "discord", "irida.theme.css.tmpl")
    );
    assert!(!env.dirs.home.join(".config/Vencord").exists());
    assert!(!env.dirs.home.join(".config/vesktop").exists());
    assert!(!env.dirs.home.join(".var/app/dev.vencord.Vesktop").exists());

    fs::create_dir_all(env.dirs.home.join(".config/vesktop")).unwrap();
    apply_only(&env, &palette("emberday"), "discord").unwrap();
    let vesktop = env.dirs.home.join(".config/vesktop/themes/irida.theme.css");
    assert!(vesktop.is_file());
    assert!(
        fs::read_to_string(&flatpak)
            .unwrap()
            .contains("theme-light")
    );
    undo(&env);
    assert!(!vesktop.exists());
    assert!(fs::read_to_string(&flatpak).unwrap().contains("theme-dark"));
    undo(&env);
    assert!(!flatpak.exists());
}

/// The exported install script finds the Flatpak client, prints the path it
/// wrote and backs up a differing copy on the next run.
#[test]
fn install_script_finds_flatpak_clients() {
    let env = TestEnv::new();
    let rice = env.tmp.path().join("rice/discord");
    fs::create_dir_all(&rice).unwrap();
    fs::write(
        rice.join("install.sh"),
        render_export("ember", "discord", "install.sh.tmpl"),
    )
    .unwrap();
    fs::write(rice.join("ember.theme.css"), "x\n").unwrap();
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
    assert!(stdout.contains("not installed, skipped"), "{stdout}");
    fs::create_dir_all(env.dirs.home.join(".var/app/com.discordapp.Discord/config")).unwrap();
    let stdout = run();
    let theme = env
        .dirs
        .home
        .join(".var/app/com.discordapp.Discord/config/Vencord/themes/ember.theme.css");
    assert!(
        stdout.contains(&format!("Vencord in Flatpak Discord: {}", theme.display())),
        "{stdout}"
    );
    assert!(stdout.contains("Load missing themes"), "{stdout}");
    assert_eq!(fs::read_to_string(&theme).unwrap(), "x\n");
    assert!(!env.dirs.home.join(".config/Vencord").exists());
    fs::write(&theme, "old\n").unwrap();
    let stdout = run();
    assert!(stdout.contains("backup: ember.theme.css.bak"), "{stdout}");
    assert_eq!(
        fs::read_to_string(theme.with_extension("css.bak")).unwrap(),
        "old\n"
    );
}
