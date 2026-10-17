//! XDG folders and path expansion.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Dirs {
    pub home: PathBuf,
    pub config: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    /// Environment variables holding an absolute path, for `${NAME:-fallback}`
    /// in target paths.
    pub vars: BTreeMap<String, PathBuf>,
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

fn xdg(value: Option<OsString>, fallback: PathBuf) -> PathBuf {
    match value {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => fallback,
    }
}

impl Dirs {
    pub fn from_env() -> Result<Dirs, DirsError> {
        let var = std::env::var_os;
        let mut dirs = Dirs::from_vars(
            var("HOME"),
            var("XDG_CONFIG_HOME"),
            var("XDG_DATA_HOME"),
            var("XDG_STATE_HOME"),
        )?;
        dirs.vars = std::env::vars_os()
            .filter_map(|(k, v)| {
                let k = k.into_string().ok()?;
                Path::new(&v).is_absolute().then(|| (k, PathBuf::from(v)))
            })
            .collect();
        Ok(dirs)
    }

    /// Build the folders from raw variable values. `home` must be absolute;
    /// relative XDG values are ignored.
    pub fn from_vars(
        home: Option<OsString>,
        xdg_config: Option<OsString>,
        xdg_data: Option<OsString>,
        xdg_state: Option<OsString>,
    ) -> Result<Dirs, DirsError> {
        let home = match home {
            Some(h) if Path::new(&h).is_absolute() => PathBuf::from(h),
            _ => return Err(DirsError::NoHome),
        };
        Ok(Dirs {
            config: xdg(xdg_config, home.join(".config")),
            data: xdg(xdg_data, home.join(".local/share")),
            state: xdg(xdg_state, home.join(".local/state")),
            home,
            vars: BTreeMap::new(),
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
            vars: BTreeMap::new(),
        }
    }

    /// The config folder as a config file names it: `~/` and the rest for a
    /// folder in the home folder, else its absolute path.
    pub fn config_display(&self) -> String {
        match self.config.strip_prefix(&self.home) {
            Ok(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
            _ => self.config.display().to_string(),
        }
    }

    /// Expand `~/`, `~/.config/` and `$XDG_*_HOME/` prefixes, and a leading
    /// `${NAME:-fallback}`: the variable's absolute path when it holds one, else
    /// the expanded fallback. Other paths are unchanged.
    pub fn expand(&self, p: &str) -> PathBuf {
        if let Some((name, fallback, rest)) = p
            .strip_prefix("${")
            .and_then(|r| r.split_once('}'))
            .and_then(|(var, rest)| {
                let (name, fallback) = var.split_once(":-")?;
                (rest.is_empty() || rest.starts_with('/')).then_some((name, fallback, rest))
            })
        {
            let base = match self.vars.get(name) {
                Some(v) => v.clone(),
                None => self.expand(fallback),
            };
            return match rest.strip_prefix('/') {
                Some(r) => base.join(r),
                None => base,
            };
        }
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
