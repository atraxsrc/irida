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
