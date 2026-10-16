//! Native targets: applications whose own config files Irida plans directly.

pub mod cosmic_desktop;
pub mod cosmic_term;

use std::path::PathBuf;

use crate::dirs::Dirs;
use crate::palette::{Mode, Palette};
use crate::target::{NativeKind, TargetError};

/// The full new contents of one config key file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFileWrite {
    pub path: PathBuf,
    pub contents: String,
    /// What undo puts back inside the file; empty for a plain key file, which
    /// undo restores whole.
    pub undo: Vec<NativeUndo>,
}

/// One part of a key file an apply changed. `before` is the RON of that scheme,
/// or of that field's value, before the write; `None` when it was absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeUndo {
    /// One COSMIC Terminal colour scheme, by id, in `color_schemes_<mode>`.
    TermScheme {
        mode: Mode,
        id: u64,
        before: Option<String>,
    },
    /// One syntax theme field of one COSMIC Terminal profile, in `profiles`.
    TermProfileTheme {
        profile: u64,
        field: String,
        before: Option<String>,
    },
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
        NativeKind::CosmicDesktop => cosmic_desktop::export(p),
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
        NativeKind::CosmicDesktop => cosmic_desktop::plan(palettes, dirs),
    }
}
