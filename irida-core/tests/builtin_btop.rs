mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use common::{TestEnv, fixture_text, palette};
use irida_core::apply::{ApplyRequest, Ctx, TargetStatus, apply};
use irida_core::reload::ReloadStatus;
use irida_core::store::Settings;
use irida_core::template::Renderer;

fn template() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/targets/btop/btop.theme.tmpl");
    fs::read_to_string(path).expect("btop template readable")
}

#[test]
fn matches_rice_files() {
    let source = template();
    for (slug, rice) in [("ember", "Ember"), ("emberday", "EmberDay")] {
        let rendered = Renderer::new(&palette(slug))
            .render("btop.theme.tmpl", &source)
            .unwrap();
        assert_eq!(
            rendered,
            fixture_text(&format!("rices/{rice}.theme")),
            "{rice}"
        );
    }
}

#[test]
fn apply_sets_theme_and_key() {
    let env = TestEnv::new();
    let bin = env.tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("btop");
    fs::write(&fake, "#!/bin/sh\necho 'btop version: 1.3.0'\n").unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let conf = env.write(
        ".config/btop/btop.conf",
        "color_theme = \"Default\"\nshown_boxes = \"cpu\"\n",
    );

    let ctx = Ctx {
        dirs: env.dirs.clone(),
        path: vec![bin],
    };
    let p = palette("ember");
    let report = apply(
        &ctx,
        &Settings::default(),
        &ApplyRequest {
            palette: &p,
            pair: None,
            only: Some(vec!["btop".into()]),
        },
    )
    .unwrap();

    let theme = env.dirs.config.join("btop/themes/irida.theme");
    assert_eq!(
        fs::read_to_string(theme).unwrap(),
        fixture_text("rices/Ember.theme")
    );
    assert_eq!(
        fs::read_to_string(conf).unwrap(),
        "color_theme = \"irida\"\nshown_boxes = \"cpu\"\n"
    );
    assert_eq!(
        report.get("btop").unwrap().status,
        TargetStatus::Applied(ReloadStatus::Restart("restart btop to see it".into()))
    );
}
