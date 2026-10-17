mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use common::{TestEnv, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::{ApplyReport, ApplyRequest, Ctx, TargetStatus, apply};
use irida_core::store::Settings;

/// Apply tmux with a fake `tmux` on the path that logs its arguments to
/// `tmux.log`; `info` succeeds only when `server` is true.
fn apply_with_tmux(env: &TestEnv, server: bool) -> ApplyReport {
    let bin = env.tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let tmux = bin.join("tmux");
    let log = env.tmp.path().join("tmux.log");
    let info = if server { 0 } else { 1 };
    fs::write(
        &tmux,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\n[ \"$1\" = info ] && exit {info}\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&tmux, fs::Permissions::from_mode(0o755)).unwrap();
    let ctx = Ctx {
        dirs: env.dirs.clone(),
        path: vec![bin],
    };
    apply(
        &ctx,
        &Settings::default(),
        &ApplyRequest {
            palette: &palette("ember"),
            pair: None,
            only: Some(vec!["tmux".to_string()]),
            live_only: false,
        },
    )
    .unwrap()
}

fn tmux_log(env: &TestEnv) -> String {
    fs::read_to_string(env.tmp.path().join("tmux.log")).unwrap_or_default()
}

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "tmux", "block.tmpl"),
            fixture_text(&format!("expected/tmux/{slug}.conf")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "tmux", "install.sh.tmpl"),
            fixture_text(&format!("expected/tmux/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "tmux", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

fn is_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

#[test]
fn block_sets_the_styles() {
    let out = render_apply("ember", "tmux", "block.tmpl");
    for option in [
        "status-style",
        "window-status-current-style",
        "pane-border-style",
        "pane-active-border-style",
        "message-style",
        "mode-style",
    ] {
        assert!(
            out.lines()
                .any(|l| l.starts_with(&format!("set -g {option} "))),
            "{option}\n{out}"
        );
    }
    for line in out.lines().filter(|l| l.starts_with("set -g ")) {
        let value = line.splitn(4, ' ').nth(3).unwrap().trim_matches('"');
        for part in value.split(',') {
            let colour = part
                .strip_prefix("bg=")
                .or_else(|| part.strip_prefix("fg="))
                .unwrap_or(part);
            assert!(is_hex(colour) || colour == "bold", "{line}");
        }
    }
}

#[test]
fn apply_appends_block_and_undo_restores() {
    let env = TestEnv::new();
    let text = "set -g mouse on\n";
    let conf = env.write(".config/tmux/tmux.conf", text);
    let report = apply_with_tmux(&env, true);
    assert!(
        matches!(report.get("tmux").unwrap().status, TargetStatus::Applied(_)),
        "{:?}",
        report.get("tmux")
    );
    let block = render_apply("ember", "tmux", "block.tmpl");
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        format!("{text}# >>> irida >>>\n{block}# <<< irida <<<\n")
    );
    assert!(tmux_log(&env).contains("source-file"), "{}", tmux_log(&env));
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
}

#[test]
fn apply_creates_xdg_conf_beside_dot_tmux_conf() {
    let env = TestEnv::new();
    let dot = env.write(".tmux.conf", "set -g mouse on\n");
    apply_with_tmux(&env, false);
    let conf = env.dirs.home.join(".config/tmux/tmux.conf");
    assert!(conf.is_file());
    assert_eq!(fs::read_to_string(&dot).unwrap(), "set -g mouse on\n");
    undo(&env);
    assert!(!conf.exists());
    assert_eq!(fs::read_to_string(&dot).unwrap(), "set -g mouse on\n");
}

#[test]
fn no_server_is_not_a_failure() {
    let env = TestEnv::new();
    let report = apply_with_tmux(&env, false);
    let t = report.get("tmux").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    assert!(
        !t.warnings.iter().any(|w| w.contains("failed")),
        "{:?}",
        t.warnings
    );
    assert!(
        !tmux_log(&env).contains("source-file"),
        "{}",
        tmux_log(&env)
    );
}
