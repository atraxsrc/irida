//! Palette storage, versions and settings under the XDG folders.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dirs::Dirs;
use crate::import::{import_rice, is_rice_folder};
use crate::palette::{Palette, validate_slug};
use crate::write::atomic_write;

#[derive(Debug)]
pub enum StoreError {
    NotFound(String),
    BadSlug(String),
    Unreadable { path: PathBuf, message: String },
    Import { path: PathBuf, message: String },
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NotFound(what) => write!(f, "not found: {what}"),
            StoreError::BadSlug(slug) => write!(f, "invalid palette slug: {slug:?}"),
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

fn read_palette(path: &Path) -> Result<Palette, StoreError> {
    let text = fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    Palette::from_toml(&text).map_err(|e| StoreError::Unreadable {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
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
        check_slug(&p.slug)?;
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
        check_slug(&p.slug)?;
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
