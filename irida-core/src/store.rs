//! Palette storage, versions and settings under the XDG folders.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dirs::Dirs;
use crate::import::{import_rice, is_rice_folder};
use crate::palette::{Palette, slug_from_name, validate_name, validate_slug};
use crate::write::atomic_write;

#[derive(Debug)]
pub enum StoreError {
    NotFound(String),
    BadSlug(String),
    /// A palette with this slug already exists.
    Exists(String),
    /// The palette breaks a rule its file needs; nothing was written.
    Invalid(String),
    Unreadable {
        path: PathBuf,
        message: String,
    },
    Import {
        path: PathBuf,
        message: String,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NotFound(what) => write!(f, "not found: {what}"),
            StoreError::BadSlug(slug) => write!(f, "invalid palette slug: {slug:?}"),
            StoreError::Exists(slug) => write!(f, "a palette called {slug} already exists"),
            StoreError::Invalid(message) => write!(f, "{message}"),
            StoreError::Unreadable { path, message } | StoreError::Import { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            StoreError::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for StoreError {}

#[derive(Debug)]
pub struct StoreEntry {
    pub slug: String,
    pub result: Result<Palette, StoreError>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub disabled: BTreeSet<String>,
    pub last_palette: Option<String>,
}

/// One saved version of a palette, and its snapshot name when pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    pub n: u32,
    pub snapshot: Option<String>,
}

/// What a palette's `pair` leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairLookup {
    /// The palette names no pair.
    None,
    /// The pair exists with the opposite mode.
    Found(Palette),
    /// The pair cannot be loaded; `reason` is the load error.
    Missing { slug: String, reason: String },
    /// The pair exists but has the same mode.
    SameMode(String),
}

/// Snapshot names are at most this many characters.
pub const SNAPSHOT_NAME_MAX: usize = 40;

/// `versions/snapshots.toml`: version number (as a string key) to name.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Snapshots {
    snapshots: BTreeMap<String, String>,
}

const STARTERS: [&str; 4] = [
    include_str!("../starters/ember.toml"),
    include_str!("../starters/emberday.toml"),
    include_str!("../starters/noir.toml"),
    include_str!("../starters/dossier.toml"),
];

/// The starter palettes (Ember, Ember Day, Noir, Dossier), in that order.
pub fn starters() -> Vec<Palette> {
    STARTERS
        .iter()
        .map(|text| Palette::from_toml(text).expect("starter palettes parse"))
        .collect()
}

/// Whether `arg` names a saved palette by slug rather than a file or folder,
/// the same test `Store::resolve` makes.
pub fn is_saved_slug(arg: &str) -> bool {
    let path = Path::new(arg);
    !path.is_file() && !path.is_dir()
}

pub struct Store {
    dirs: Dirs,
}

fn io_err(path: &Path, source: io::Error) -> StoreError {
    StoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn check_slug(slug: &str) -> Result<(), StoreError> {
    validate_slug(slug).map_err(|_| StoreError::BadSlug(slug.to_string()))
}

/// The checks every write of a palette makes first: the slug (it names the
/// folder), then the rest of the file's rules.
fn check_palette(p: &Palette) -> Result<(), StoreError> {
    check_slug(&p.slug)?;
    p.validate().map_err(|e| StoreError::Invalid(e.to_string()))
}

fn read_palette(path: &Path) -> Result<Palette, StoreError> {
    let text = fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    Palette::from_toml(&text).map_err(|e| StoreError::Unreadable {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// A slug that may name a folder in the store; anything else is not found,
/// so no path is ever built from it.
fn known_slug(slug: &str) -> Result<(), StoreError> {
    validate_slug(slug).map_err(|_| StoreError::NotFound(slug.to_string()))
}

fn version_not_found(slug: &str, n: u32) -> StoreError {
    StoreError::NotFound(format!("{slug} version {n}"))
}

fn write_file(path: &Path, contents: &str) -> Result<(), StoreError> {
    atomic_write(path, contents.as_bytes()).map_err(|e| io_err(path, e))
}

impl Store {
    pub fn new(dirs: Dirs) -> Store {
        Store { dirs }
    }

    fn palettes_dir(&self) -> PathBuf {
        self.dirs.data.join("irida/palettes")
    }

    fn palette_dir(&self, slug: &str) -> PathBuf {
        self.palettes_dir().join(slug)
    }

    fn settings_path(&self) -> PathBuf {
        self.dirs.config.join("irida/settings.toml")
    }

    pub fn list(&self) -> Vec<StoreEntry> {
        let Ok(rd) = fs::read_dir(self.palettes_dir()) else {
            return Vec::new();
        };
        let mut entries: Vec<StoreEntry> = rd
            .filter_map(Result::ok)
            .filter_map(|e| {
                let file = e.path().join("palette.toml");
                let slug = e.file_name().into_string().ok()?;
                file.is_file().then(|| StoreEntry {
                    slug,
                    result: read_palette(&file),
                })
            })
            .collect();
        entries.sort_by(|a, b| a.slug.cmp(&b.slug));
        entries
    }

    pub fn load(&self, slug: &str) -> Result<Palette, StoreError> {
        if validate_slug(slug).is_err() {
            return Err(StoreError::NotFound(slug.to_string()));
        }
        let path = self.palette_dir(slug).join("palette.toml");
        if !path.is_file() {
            return Err(StoreError::NotFound(slug.to_string()));
        }
        read_palette(&path)
    }

    pub fn save(&self, p: &Palette) -> Result<(), StoreError> {
        check_palette(p)?;
        write_file(
            &self.palette_dir(&p.slug).join("palette.toml"),
            &p.to_toml(),
        )
    }

    fn version_numbers(&self, slug: &str) -> Result<Vec<u32>, StoreError> {
        let dir = self.palette_dir(slug).join("versions");
        let rd = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_err(&dir, e)),
        };
        Ok(rd
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                name.strip_suffix(".toml")?.parse::<u32>().ok()
            })
            .collect())
    }

    pub fn save_version(&self, p: &Palette) -> Result<u32, StoreError> {
        check_palette(p)?;
        let n = self
            .version_numbers(&p.slug)?
            .into_iter()
            .max()
            .unwrap_or(0)
            + 1;
        let text = p.to_toml();
        let dir = self.palette_dir(&p.slug);
        write_file(&dir.join("versions").join(format!("{n}.toml")), &text)?;
        write_file(&dir.join("palette.toml"), &text)?;
        Ok(n)
    }

    pub fn latest_version(&self, slug: &str) -> Result<Option<(u32, Palette)>, StoreError> {
        if validate_slug(slug).is_err() {
            return Ok(None);
        }
        let Some(n) = self.version_numbers(slug)?.into_iter().max() else {
            return Ok(None);
        };
        let path = self
            .palette_dir(slug)
            .join("versions")
            .join(format!("{n}.toml"));
        Ok(Some((n, read_palette(&path)?)))
    }

    pub fn resolve(&self, arg: &str) -> Result<Palette, StoreError> {
        let path = Path::new(arg);
        if path.is_file() {
            return read_palette(path);
        }
        if path.is_dir() {
            let file = path.join("palette.toml");
            if file.is_file() {
                return read_palette(&file);
            }
            if is_rice_folder(path) {
                return import_rice(path).map_err(|e| StoreError::Import {
                    path: e.path,
                    message: e.message,
                });
            }
            return Err(StoreError::NotFound(format!(
                "{arg}: no palette.toml, and not a rice repo (the folder must be named cosmic-<slug>-theme)"
            )));
        }
        self.load(arg)
    }

    /// True when `settings.toml` exists. Not found is false; any other IO error is returned.
    pub fn settings_present(&self) -> Result<bool, StoreError> {
        match fs::metadata(self.settings_path()) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(io_err(&self.settings_path(), e)),
        }
    }

    pub fn settings(&self) -> Result<Settings, StoreError> {
        let path = self.settings_path();
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(e) => return Err(io_err(&path, e)),
        };
        toml::from_str(&text).map_err(|e| StoreError::Unreadable {
            path,
            message: e.to_string(),
        })
    }

    pub fn save_settings(&self, s: &Settings) -> Result<(), StoreError> {
        let text = toml::to_string(s).expect("settings serialise");
        write_file(&self.settings_path(), &text)
    }
}

impl Store {
    fn versions_dir(&self, slug: &str) -> PathBuf {
        self.palette_dir(slug).join("versions")
    }

    fn version_path(&self, slug: &str, n: u32) -> PathBuf {
        self.versions_dir(slug).join(format!("{n}.toml"))
    }

    fn snapshots_path(&self, slug: &str) -> PathBuf {
        self.versions_dir(slug).join("snapshots.toml")
    }

    /// Whether a palette folder exists for `slug`, readable or not.
    pub fn exists(&self, slug: &str) -> bool {
        validate_slug(slug).is_ok() && self.palette_dir(slug).exists()
    }

    /// Remove the palette folder (versions included). A partner whose `pair`
    /// points back has it cleared. An unreadable palette can be deleted too.
    pub fn delete(&self, slug: &str) -> Result<(), StoreError> {
        known_slug(slug)?;
        let dir = self.palette_dir(slug);
        if !dir.exists() {
            return Err(StoreError::NotFound(slug.to_string()));
        }
        let partner = self.load(slug).ok().and_then(|p| p.pair);
        fs::remove_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        if let Some(partner) = partner {
            self.clear_pair_pointing_to(&partner, slug)?;
        }
        Ok(())
    }

    /// Clear `pair` on `slug` when it points to `to`. A missing or
    /// unreadable palette is left alone.
    fn clear_pair_pointing_to(&self, slug: &str, to: &str) -> Result<(), StoreError> {
        if let Some(p) = self.partner_pointing_to(slug, to) {
            self.save(&p)?;
        }
        Ok(())
    }

    /// `slug` loaded with its `pair` cleared, when that pair points to `to`.
    fn partner_pointing_to(&self, slug: &str, to: &str) -> Option<Palette> {
        let mut p = self.load(slug).ok()?;
        (p.pair.as_deref() == Some(to)).then(|| {
            p.pair = None;
            p
        })
    }

    /// Copy a palette under a new name, with the slug that name gives and no
    /// pair, saved as version 1.
    pub fn duplicate(&self, slug: &str, new_name: &str) -> Result<Palette, StoreError> {
        validate_name(new_name).map_err(|e| StoreError::Invalid(e.to_string()))?;
        let new_slug = slug_from_name(new_name);
        if new_slug.is_empty() {
            return Err(StoreError::Invalid(format!(
                "the name {new_name:?} has no letters to make a slug from: add a letter"
            )));
        }
        if self.exists(&new_slug) {
            return Err(StoreError::Exists(new_slug));
        }
        let mut copy = self.load(slug)?;
        copy.name = new_name.to_string();
        copy.slug = new_slug;
        copy.pair = None;
        self.save_version(&copy)?;
        Ok(copy)
    }

    fn read_snapshots(&self, slug: &str) -> Result<Snapshots, StoreError> {
        let path = self.snapshots_path(slug);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Snapshots::default()),
            Err(e) => return Err(io_err(&path, e)),
        };
        toml::from_str(&text).map_err(|e| StoreError::Unreadable {
            path,
            message: e.to_string(),
        })
    }

    fn write_snapshots(&self, slug: &str, s: &Snapshots) -> Result<(), StoreError> {
        let path = self.snapshots_path(slug);
        if s.snapshots.is_empty() {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(io_err(&path, e)),
                _ => Ok(()),
            };
        }
        write_file(&path, &toml::to_string(s).expect("snapshots serialise"))
    }

    fn check_version(&self, slug: &str, n: u32) -> Result<(), StoreError> {
        known_slug(slug)?;
        if self.version_path(slug, n).is_file() {
            Ok(())
        } else {
            Err(version_not_found(slug, n))
        }
    }

    /// The saved versions, newest first, with their snapshot names. A
    /// palette with no saved version gives an empty list.
    pub fn versions(&self, slug: &str) -> Result<Vec<VersionInfo>, StoreError> {
        known_slug(slug)?;
        let mut numbers = self.version_numbers(slug)?;
        numbers.sort_unstable_by(|a, b| b.cmp(a));
        let mut names = self.read_snapshots(slug)?.snapshots;
        Ok(numbers
            .into_iter()
            .map(|n| VersionInfo {
                n,
                snapshot: names.remove(&n.to_string()),
            })
            .collect())
    }

    pub fn load_version(&self, slug: &str, n: u32) -> Result<Palette, StoreError> {
        self.check_version(slug, n)?;
        read_palette(&self.version_path(slug, n))
    }

    /// Name version `n` as a snapshot (renaming it when it has a name). The
    /// name is trimmed, and must be non-empty and at most 40 characters.
    pub fn pin_snapshot(&self, slug: &str, n: u32, name: &str) -> Result<(), StoreError> {
        self.check_version(slug, n)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid(
                "empty snapshot name: give it a name".to_string(),
            ));
        }
        if name.chars().count() > SNAPSHOT_NAME_MAX {
            return Err(StoreError::Invalid(format!(
                "snapshot name {name:?} is longer than {SNAPSHOT_NAME_MAX} characters: shorten it"
            )));
        }
        let mut s = self.read_snapshots(slug)?;
        s.snapshots.insert(n.to_string(), name.to_string());
        self.write_snapshots(slug, &s)
    }

    /// Remove version `n`'s snapshot name; a version without one is left as is.
    pub fn unpin_snapshot(&self, slug: &str, n: u32) -> Result<(), StoreError> {
        self.check_version(slug, n)?;
        let mut s = self.read_snapshots(slug)?;
        if s.snapshots.remove(&n.to_string()).is_some() {
            self.write_snapshots(slug, &s)?;
        }
        Ok(())
    }

    /// Where `p`'s `pair` leads.
    pub fn pair_of(&self, p: &Palette) -> PairLookup {
        let Some(slug) = p.pair.as_deref() else {
            return PairLookup::None;
        };
        match self.load(slug) {
            Ok(pair) if pair.mode != p.mode => PairLookup::Found(pair),
            Ok(_) => PairLookup::SameMode(slug.to_string()),
            Err(e) => PairLookup::Missing {
                slug: slug.to_string(),
                reason: e.to_string(),
            },
        }
    }

    /// Pair `a` and `b`, which must both load and have opposite modes. An old
    /// partner of either that points back has its `pair` cleared first.
    pub fn link_pair(&self, a: &str, b: &str) -> Result<(), StoreError> {
        if a == b {
            return Err(StoreError::Invalid(format!(
                "cannot pair {a} with itself: choose a palette of the other mode"
            )));
        }
        let load = |slug: &str| {
            self.load(slug)
                .map_err(|e| StoreError::Invalid(format!("cannot pair with {slug}: {e}")))
        };
        let (mut pa, mut pb) = (load(a)?, load(b)?);
        if pa.mode == pb.mode {
            return Err(StoreError::Invalid(format!(
                "{a} and {b} are both {}: a pair is one dark and one light palette",
                pa.mode
            )));
        }
        let mut writes = Vec::new();
        for (p, other) in [(&pa, b), (&pb, a)] {
            if let Some(old) = p.pair.as_deref()
                && old != other
                && let Some(cleared) = self.partner_pointing_to(old, &p.slug)
            {
                writes.push(cleared);
            }
        }
        pa.pair = Some(b.to_string());
        pb.pair = Some(a.to_string());
        writes.push(pa);
        writes.push(pb);
        for p in &writes {
            check_palette(p)?;
        }
        writes.iter().try_for_each(|p| self.save(p))
    }

    /// Clear `pair` on `slug`, and on its partner when it points back.
    pub fn unlink_pair(&self, slug: &str) -> Result<(), StoreError> {
        let mut p = self.load(slug)?;
        let Some(partner) = p.pair.take() else {
            return Ok(());
        };
        self.save(&p)?;
        self.clear_pair_pointing_to(&partner, slug)
    }

    /// Save Ember, Ember Day, Noir and Dossier (each as version 1) when the
    /// palettes folder is missing or holds no folder at all; returns how many
    /// were saved (4 or 0).
    pub fn seed_starters(&self) -> Result<usize, StoreError> {
        let dir = self.palettes_dir();
        match fs::read_dir(&dir) {
            Ok(mut rd) => {
                if rd.any(|e| e.is_ok_and(|e| e.path().is_dir())) {
                    return Ok(0);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&dir, e)),
        }
        let starters = starters();
        for p in &starters {
            self.save_version(p)?;
        }
        Ok(starters.len())
    }

    /// The text of a palette file as stored, for showing one that cannot be
    /// read as a palette.
    pub fn raw_text(&self, slug: &str) -> Result<String, StoreError> {
        known_slug(slug)?;
        let path = self.palette_dir(slug).join("palette.toml");
        match fs::read_to_string(&path) {
            Ok(t) => Ok(t),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(StoreError::NotFound(slug.to_string()))
            }
            Err(e) => Err(io_err(&path, e)),
        }
    }
}
