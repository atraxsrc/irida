mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_export};
use irida_core::apply::TargetStatus;
use irida_core::reload::ReloadStatus;

#[test]
fn block_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "cava", "cava.tmpl"),
            fixture_text(&format!("expected/cava/{slug}.block")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "cava", "install.sh.tmpl"),
            fixture_text(&format!("expected/cava/{slug}-install.sh")),
            "{slug}"
        );
    }
}

fn gradient(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.strip_prefix("gradient_color_"))
        .map(|l| l.split('\'').nth(1).unwrap().to_string())
        .collect()
}

#[test]
fn gradients() {
    for (slug, want) in [
        (
            "ember",
            "#8FB58A #B4A272 #D18C58 #E8713A #E46B44 #E0664C #DC6054 #D85A5A",
        ),
        (
            "emberday",
            "#4C6B45 #706239 #8C552C #A4441A #A24225 #A13F2D #9F3D34 #9E3A3A",
        ),
    ] {
        let out = render_export(slug, "cava", "cava.tmpl");
        assert_eq!(gradient(&out).join(" "), want, "{slug}");
        assert!(out.contains("gradient_count = 8\n"), "{out}");
    }
}

#[test]
fn apply_appends_then_replaces() {
    let env = TestEnv::new();
    let conf = env.write(".config/cava/config", "[general]\n[eq]\n");
    let report = apply_only(&env, &palette("ember"), "cava").unwrap();
    assert_eq!(
        report.get("cava").unwrap().status,
        TargetStatus::Applied(ReloadStatus::NotRunning)
    );
    let body = render_export("ember", "cava", "cava.tmpl");
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        format!("[general]\n[eq]\n# >>> irida >>>\n{body}# <<< irida <<<\n")
    );

    apply_only(&env, &palette("emberday"), "cava").unwrap();
    let text = fs::read_to_string(&conf).unwrap();
    assert_eq!(text.matches("# >>> irida >>>").count(), 1);
    assert_eq!(text.matches("# <<< irida <<<").count(), 1);
    let body = render_export("emberday", "cava", "cava.tmpl");
    assert_eq!(
        text,
        format!("[general]\n[eq]\n# >>> irida >>>\n{body}# <<< irida <<<\n")
    );
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "cava", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}
