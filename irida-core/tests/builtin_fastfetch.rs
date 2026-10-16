mod common;

use common::{TestEnv, apply_only, palette, render_export, rice_text};
use irida_core::apply::TargetStatus;

#[test]
fn matches_rice_files() {
    for slug in ["ember", "emberday"] {
        for (template, rel) in [
            ("config.jsonc.tmpl", "fastfetch/config.jsonc"),
            ("cosmic.txt", "fastfetch/cosmic.txt"),
            ("install.sh.tmpl", "fastfetch/install.sh"),
        ] {
            assert_eq!(
                render_export(slug, "fastfetch", template),
                rice_text(slug, rel),
                "{slug} {rel}"
            );
        }
    }
}

#[test]
fn lines_round_half_even() {
    assert!(
        render_export("emberday", "fastfetch", "config.jsonc.tmpl").contains("38;2;168;166;162m")
    );
}

#[test]
fn replaces_existing_with_note() {
    let env = TestEnv::new();
    env.write(".config/fastfetch/config.jsonc", "{ \"mine\": true }\n");
    let report = apply_only(&env, &palette("ember"), "fastfetch").unwrap();
    let t = report.get("fastfetch").unwrap();
    assert!(
        matches!(t.status, TargetStatus::Applied(_)),
        "{:?}",
        t.status
    );
    assert!(
        t.warnings.iter().any(|w| w.contains("replaced your file")),
        "{:?}",
        t.warnings
    );
}
