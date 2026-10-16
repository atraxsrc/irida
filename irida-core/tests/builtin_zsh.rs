mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::{TestEnv, palette, render_apply, render_export, rice_text, undo};
use irida_core::apply::{ApplyError, ApplyRequest, Ctx, SkipReason, TargetStatus, apply};
use irida_core::reload::ReloadStatus;
use irida_core::store::Settings;

const HINT: &str = "open a new shell or run exec zsh";

#[test]
fn export_matches_rice() {
    for slug in ["ember", "emberday"] {
        for (template, rel) in [
            ("zsh.tmpl", format!("zsh/{slug}.zsh")),
            ("install.sh.tmpl", "zsh/install.sh".to_string()),
        ] {
            assert_eq!(
                render_export(slug, "zsh", template),
                rice_text(slug, &rel),
                "{slug} {rel}"
            );
        }
    }
}

#[test]
fn apply_block_is_colour_part() {
    for slug in ["ember", "emberday"] {
        let rice = rice_text(slug, &format!("zsh/{slug}.zsh"));
        let part: String = rice.split_inclusive('\n').skip(14).take(32).collect();
        assert_eq!(render_apply(slug, "zsh", "zsh.tmpl"), part, "{slug}");
    }
}

#[test]
fn autosuggest() {
    for (slug, want) in [("ember", "#474C59"), ("emberday", "#A4A6AA")] {
        assert!(
            render_apply(slug, "zsh", "zsh.tmpl")
                .contains(&format!("ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE='fg={want}'")),
            "{slug}"
        );
    }
}

fn ctx(env: &TestEnv, path: Vec<std::path::PathBuf>) -> Ctx {
    Ctx {
        dirs: env.dirs.clone(),
        path,
    }
}

fn request(p: &irida_core::palette::Palette) -> ApplyRequest<'_> {
    ApplyRequest {
        palette: p,
        pair: None,
        only: Some(vec!["zsh".into()]),
        live_only: false,
    }
}

#[test]
fn apply_appends_marked_block() {
    let env = TestEnv::new();
    let zshrc = env.write(".zshrc", "export X=1\n");
    let ember = palette("ember");
    let report = apply(&ctx(&env, vec![]), &Settings::default(), &request(&ember)).unwrap();
    assert_eq!(
        report.get("zsh").unwrap().status,
        TargetStatus::Applied(ReloadStatus::Restart(HINT.into()))
    );
    let body = render_apply("ember", "zsh", "zsh.tmpl");
    assert_eq!(
        fs::read_to_string(&zshrc).unwrap(),
        format!("export X=1\n# >>> irida >>>\n{body}# <<< irida <<<\n")
    );
    undo(&env);
    assert_eq!(fs::read_to_string(&zshrc).unwrap(), "export X=1\n");
}

#[test]
fn not_detected_without_zshrc() {
    let env = TestEnv::new();
    let bin = env.tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let zsh = bin.join("zsh");
    fs::write(&zsh, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&zsh, fs::Permissions::from_mode(0o755)).unwrap();
    let ember = palette("ember");
    match apply(
        &ctx(&env, vec![bin]),
        &Settings::default(),
        &request(&ember),
    ) {
        Err(ApplyError::NothingToApply(results)) => {
            let z = results
                .iter()
                .find(|r| r.name == "zsh")
                .expect("zsh listed");
            assert_eq!(z.status, TargetStatus::Skipped(SkipReason::NotInstalled));
        }
        other => panic!("expected NothingToApply, got {other:?}"),
    }
    assert!(!env.dirs.home.join(".zshrc").exists());
}
