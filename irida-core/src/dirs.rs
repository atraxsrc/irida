//! XDG folders and path expansion.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Dirs {
    pub home: PathBuf,
    pub config: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DirsError {
    NoHome,
}

impl fmt::Display for DirsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DirsError::NoHome => write!(f, "$HOME is not set to an absolute path"),
        }
    }
}

impl std::error::Error for DirsError {}

fn xdg(name: &str, fallback: PathBuf) -> PathBuf {
    match std::env::var_os(name) {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => fallback,
    }
}

impl Dirs {
    pub fn from_env() -> Result<Dirs, DirsError> {
        let home = match std::env::var_os("HOME") {
            Some(h) if !h.is_empty() => PathBuf::from(h),
            _ => return Err(DirsError::NoHome),
        };
        Ok(Dirs {
            config: xdg("XDG_CONFIG_HOME", home.join(".config")),
            data: xdg("XDG_DATA_HOME", home.join(".local/share")),
            state: xdg("XDG_STATE_HOME", home.join(".local/state")),
            home,
        })
    }

    /// A self-contained set of folders below `root`, for tests.
    pub fn under(root: &Path) -> Dirs {
        let home = root.join("home");
        Dirs {
            config: home.join(".config"),
            data: home.join(".local/share"),
            state: home.join(".local/state"),
            home,
        }
    }

    /// Expand `~/`, `~/.config/` and `$XDG_*_HOME/` prefixes; other paths are unchanged.
    pub fn expand(&self, p: &str) -> PathBuf {
        let vars = [
            ("$XDG_CONFIG_HOME", &self.config),
            ("$XDG_DATA_HOME", &self.data),
            ("$XDG_STATE_HOME", &self.state),
        ];
        for (var, base) in vars {
            if p == var {
                return base.clone();
            }
            if let Some(rest) = p.strip_prefix(var).and_then(|r| r.strip_prefix('/')) {
                return base.join(rest);
            }
        }
        if p == "~/.config" {
            return self.config.clone();
        }
        if let Some(rest) = p.strip_prefix("~/.config/") {
            return self.config.join(rest);
        }
        if p == "~" {
            return self.home.clone();
        }
        if let Some(rest) = p.strip_prefix("~/") {
            return self.home.join(rest);
        }
        PathBuf::from(p)
    }
}
