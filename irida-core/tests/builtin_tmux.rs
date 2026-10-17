mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use common::{TestEnv, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::{ApplyReport, ApplyRequest, Ctx, TargetStatus, apply};
use irida_core::live::LiveSession;
use irida_core::store::Settings;

/// Apply tmux with a fake `tmux` on the path that logs its arguments to
/// `tmux.log`; `info` succeeds only when `server` is true.
fn apply_with_tmux(env: &TestEnv, server: bool) -> ApplyReport {
    apply(
        &ctx_with_tmux(env, server),
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

/// A context with the fake `tmux` of `apply_with_tmux` on the path.
fn ctx_with_tmux(env: &TestEnv, server: bool) -> Ctx {
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
    Ctx {
        dirs: env.dirs.clone(),
        path: vec![bin],
    }
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

/// source-file only sets options, so the reload first puts back tmux's
/// default for every option the block sets: an undone or reverted block
/// then leaves no colour behind in a running server.
#[test]
fn reload_resets_every_option_the_block_sets() {
    let block = common::target_file("tmux", "block.tmpl");
    let options: Vec<&str> = block
        .lines()
        .filter_map(|l| l.strip_prefix("set -g "))
        .map(|l| l.split(' ').next().unwrap())
        .collect();
    assert_eq!(options.len(), 12, "{options:?}");
    let env = TestEnv::new();
    apply_with_tmux(&env, true);
    let log = tmux_log(&env);
    let reload = log.lines().find(|l| l.contains("source-file")).unwrap();
    for option in &options {
        let reset = format!("set -gu {option} ;");
        let at = reload
            .find(&reset)
            .unwrap_or_else(|| panic!("{option}: {reload}"));
        assert!(at < reload.find("source-file").unwrap(), "{reload}");
    }
}

/// tmux is live: a session writes the block and Revert puts the file back,
/// then reloads with every Irida option reset.
#[test]
fn live_session_reverts() {
    let env = TestEnv::new();
    let text = "set -g mouse on\n";
    let conf = env.write(".config/tmux/tmux.conf", text);
    let ctx = ctx_with_tmux(&env, true);
    let mut s = LiveSession::start(&ctx, "ember").unwrap();
    let req = ApplyRequest {
        palette: &palette("ember"),
        pair: None,
        only: None,
        live_only: false,
    };
    let report = s.apply(&ctx, &Settings::default(), &req).unwrap();
    let t = report.get("tmux").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    assert!(
        fs::read_to_string(&conf)
            .unwrap()
            .contains("# >>> irida >>>")
    );
    fs::remove_file(env.tmp.path().join("tmux.log")).unwrap();
    s.revert(&ctx).unwrap().expect("a report");
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
    let log = tmux_log(&env);
    assert!(
        log.lines()
            .any(|l| l.contains("set -gu status-style ;") && l.contains("source-file")),
        "{log}"
    );
}
