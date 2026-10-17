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
