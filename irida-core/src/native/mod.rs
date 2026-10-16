//! Native targets: applications whose own config files Irida plans directly.

pub mod cosmic_term;

use std::path::PathBuf;

use crate::dirs::Dirs;
use crate::palette::Palette;
use crate::target::{NativeKind, TargetError};

/// The full new contents of one config key file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFileWrite {
    pub path: PathBuf,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NativePlan {
    pub writes: Vec<KeyFileWrite>,
    pub warnings: Vec<String>,
}

/// One file a native target adds to an exported rice repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    /// Relative to the repo.
    pub path: String,
    pub contents: String,
}

/// One palette, or a pair of one dark and one light palette.
pub fn check_palettes(palettes: &[&Palette]) -> Result<(), TargetError> {
    match palettes {
        [_] => Ok(()),
        [a, b] if a.mode != b.mode => Ok(()),
        [_, _] => Err(TargetError::Invalid(
            "a pair needs one dark and one light palette".into(),
        )),
        _ => Err(TargetError::Invalid(format!(
            "expected one palette or a pair, got {}",
            palettes.len()
        ))),
    }
}

/// The files a native target adds to an exported rice repo.
pub fn export(kind: NativeKind, p: &Palette) -> Vec<ExportFile> {
    match kind {
        NativeKind::CosmicTerm => cosmic_term::export(p),
    }
}

/// Plan the writes for one palette, or both halves of a pair.
pub fn plan(
    kind: NativeKind,
    palettes: &[&Palette],
    dirs: &Dirs,
) -> Result<NativePlan, TargetError> {
    match kind {
        NativeKind::CosmicTerm => cosmic_term::plan(palettes, dirs),
    }
}
