mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use common::{TestEnv, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::{ApplyReport, ApplyRequest, Ctx, TargetStatus, apply};
use irida_core::live::LiveSession;
use irida_core::store::Settings;
use irida_core::template::Renderer;

/// A context with a fake `starship` binary on the path, so the target is
/// detected.
fn ctx_with_binary(env: &TestEnv) -> Ctx {
    let bin = env.tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let starship = bin.join("starship");
    fs::write(&starship, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&starship, fs::Permissions::from_mode(0o755)).unwrap();
    Ctx {
        dirs: env.dirs.clone(),
        path: vec![bin],
    }
}

fn apply_with_binary(env: &TestEnv) -> ApplyReport {
    let ctx = ctx_with_binary(env);
    apply(
        &ctx,
        &Settings::default(),
        &ApplyRequest {
            palette: &palette("ember"),
            pair: None,
            only: Some(vec!["starship".to_string()]),
            live_only: false,
        },
    )
    .unwrap()
}

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "starship", "block.tmpl"),
            fixture_text(&format!("expected/starship/{slug}.toml")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "starship", "install.sh.tmpl"),
            fixture_text(&format!("expected/starship/{slug}-install.sh")),
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
        render_export("ember", "starship", "install.sh.tmpl"),
    )
    .unwrap();
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

fn check_block(out: &str, name: &str) {
    let doc: toml::Table = toml::from_str(out).unwrap_or_else(|e| panic!("{e}\n{out}"));
    assert_eq!(doc.len(), 1, "{out}");
    let table = doc["palettes"].as_table().unwrap()[name]
        .as_table()
        .unwrap_or_else(|| panic!("palettes.{name}\n{out}"));
    for (k, v) in table {
        assert!(v.as_str().is_some_and(is_hex), "{k} = {v}");
    }
    for key in ["red", "green", "blue", "purple", "bright-red"] {
        assert!(table.contains_key(key), "{key}");
    }
}

#[test]
fn block_is_a_palette_table() {
    check_block(&render_apply("ember", "starship", "block.tmpl"), "irida");
    check_block(&render_export("ember", "starship", "block.tmpl"), "ember");
}

#[test]
fn selector_value() {
    let source = common::target_file("starship", "palette.tmpl");
    let p = palette("ember");
    assert_eq!(
        Renderer::new(&p).render("palette.tmpl", &source).unwrap(),
        "\"irida\""
    );
    assert_eq!(
        Renderer::for_export(&p)
            .render("palette.tmpl", &source)
            .unwrap(),
        "\"ember\""
    );
}

#[test]
fn apply_puts_palette_on_top_and_table_at_end() {
    let env = TestEnv::new();
    let text = "add_newline = false\n\n[character]\nsuccess_symbol = \"[>](bold green)\"\n";
    let conf = env.write(".config/starship.toml", text);
    let report = apply_with_binary(&env);
    let t = report.get("starship").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    let block = render_apply("ember", "starship", "block.tmpl");
    let after = fs::read_to_string(&conf).unwrap();
    assert_eq!(
        after,
        format!(
            "add_newline = false\n\npalette = \"irida\"\n[character]\nsuccess_symbol = \"[>](bold green)\"\n\
             # >>> irida >>>\n{block}# <<< irida <<<\n"
        )
    );
    let doc: toml::Table = toml::from_str(&after).unwrap_or_else(|e| panic!("{e}\n{after}"));
    assert_eq!(doc["palette"].as_str(), Some("irida"));
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
}

#[test]
fn user_palette_is_replaced_and_named() {
    let env = TestEnv::new();
    let text = "palette = \"mine\"\n\n[palettes.mine]\nred = \"#ff0000\"\n";
    let conf = env.write(".config/starship.toml", text);
    let report = apply_with_binary(&env);
    let w = &report.get("starship").unwrap().warnings;
    assert!(
        w.iter().any(|m| m.ends_with(
            "starship.toml: replaced your palette line; colour names from your own \
             palette stop working; irida undo restores it"
        )),
        "{w:?}"
    );
    let after = fs::read_to_string(&conf).unwrap();
    assert!(
        after.starts_with("palette = \"irida\"\n\n[palettes.mine]\nred = \"#ff0000\"\n"),
        "{after}"
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
}

#[test]
fn starship_config_env_is_followed() {
    let mut env = TestEnv::new();
    let elsewhere = env.tmp.path().join("elsewhere/starship.toml");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::write(&elsewhere, "add_newline = false\n").unwrap();
    env.dirs
        .vars
        .insert("STARSHIP_CONFIG".to_string(), elsewhere.clone());
    apply_with_binary(&env);
    assert!(
        fs::read_to_string(&elsewhere)
            .unwrap()
            .contains("palette = \"irida\"")
    );
    assert!(!env.dirs.home.join(".config/starship.toml").exists());
}

/// Starship rereads its config at every prompt, so it is live: a session
/// writes it, Revert puts the file back, and Keep leaves one undo.
#[test]
fn live_session_reverts_and_keeps() {
    let text = "format = \"$all\"\n";
    for keep in [false, true] {
        let env = TestEnv::new();
        let conf = env.write(".config/starship.toml", text);
        let ctx = ctx_with_binary(&env);
        let mut s = LiveSession::start(&ctx, "ember").unwrap();
        for slug in ["ember", "noir"] {
            let req = ApplyRequest {
                palette: &palette(slug),
                pair: None,
                only: None,
                live_only: false,
            };
            let report = s.apply(&ctx, &Settings::default(), &req).unwrap();
            let t = report.get("starship").unwrap();
            assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
        }
        let after = fs::read_to_string(&conf).unwrap();
        assert!(after.contains("palette = \"irida\"\n"), "{after}");
        if keep {
            s.keep(&ctx).unwrap();
            irida_core::apply::undo(&ctx).unwrap();
        } else {
            s.revert(&ctx).unwrap().expect("a report");
        }
        assert_eq!(fs::read_to_string(&conf).unwrap(), text, "keep {keep}");
    }
}
