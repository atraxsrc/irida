#![allow(dead_code)]

use std::path::PathBuf;

use irida_core::palette::Palette;

pub fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

pub fn fixture_text(rel: &str) -> String {
    std::fs::read_to_string(fixture(rel)).expect("fixture readable")
}

pub fn palette(slug: &str) -> Palette {
    Palette::from_toml(&fixture_text(&format!("palettes/{slug}.toml"))).expect("fixture parses")
}

pub struct TestEnv {
    pub tmp: tempfile::TempDir,
    pub dirs: irida_core::dirs::Dirs,
}

impl TestEnv {
    pub fn new() -> TestEnv {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let dirs = irida_core::dirs::Dirs::under(tmp.path());
        TestEnv { tmp, dirs }
    }

    /// Write a file below the fake home, creating parent folders.
    pub fn write(&self, rel_to_home: &str, contents: &str) -> PathBuf {
        let path = self.dirs.home.join(rel_to_home);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
        path
    }
}

/// A file of a built-in target folder.
pub fn target_file(target: &str, name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("targets")
        .join(target)
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A file of a rice fixture repo, `rices/cosmic-<slug>-theme/<rel>`.
pub fn rice_text(slug: &str, rel: &str) -> String {
    fixture_text(&format!("rices/cosmic-{slug}-theme/{rel}"))
}

/// Render a built-in target's template for export.
pub fn render_export(slug: &str, target: &str, template: &str) -> String {
    irida_core::template::Renderer::for_export(&palette(slug))
        .render(template, &target_file(target, template))
        .unwrap_or_else(|e| panic!("{e}"))
}

/// Render a built-in target's template for applying.
pub fn render_apply(slug: &str, target: &str, template: &str) -> String {
    irida_core::template::Renderer::new(&palette(slug))
        .render(template, &target_file(target, template))
        .unwrap_or_else(|e| panic!("{e}"))
}

/// Apply only `target`, with nothing on the path.
pub fn apply_only(
    env: &TestEnv,
    p: &Palette,
    target: &str,
) -> Result<irida_core::apply::ApplyReport, irida_core::apply::ApplyError> {
    let ctx = irida_core::apply::Ctx {
        dirs: env.dirs.clone(),
        path: vec![],
    };
    irida_core::apply::apply(
        &ctx,
        &irida_core::store::Settings::default(),
        &irida_core::apply::ApplyRequest {
            palette: p,
            pair: None,
            only: Some(vec![target.to_string()]),
            live_only: false,
        },
    )
}

/// Undo the newest apply in `env`.
pub fn undo(env: &TestEnv) {
    let ctx = irida_core::apply::Ctx {
        dirs: env.dirs.clone(),
        path: vec![],
    };
    let report = irida_core::apply::undo(&ctx).expect("undo");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
}
