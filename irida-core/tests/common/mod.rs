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
