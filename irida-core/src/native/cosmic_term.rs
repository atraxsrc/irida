//! COSMIC Terminal: colour schemes and syntax themes in its cosmic-config key files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{ExportFile, KeyFileWrite, NativePlan};
use crate::derive::derive;
use crate::dirs::Dirs;
use crate::palette::{Mode, Palette};
use crate::target::TargetError;
use crate::term::ColorScheme;

pub const CONFIG_ID: &str = "com.system76.CosmicTerm";
pub const VERSION: u64 = 1;

/// `<config>/cosmic/com.system76.CosmicTerm/v1`, where cosmic-config keeps one file per key.
pub fn config_dir(dirs: &Dirs) -> PathBuf {
    dirs.config
        .join("cosmic")
        .join(CONFIG_ID)
        .join(format!("v{VERSION}"))
}

/// Mirror of cosmic-term's `Profile`, in its field order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    name: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    syntax_theme_dark: String,
    #[serde(default)]
    syntax_theme_light: String,
    #[serde(default)]
    tab_title: String,
    #[serde(default)]
    working_directory: String,
    #[serde(default)]
    drain_on_exit: bool,
}

/// Mirror of cosmic-term's `AppTheme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
enum AppTheme {
    Dark,
    Light,
    System,
}

type Schemes = BTreeMap<u64, ColorScheme>;

/// Read and parse one key file; `None` when it does not exist.
fn read_key<T: DeserializeOwned>(dir: &Path, key: &str) -> Result<Option<T>, TargetError> {
    let path = dir.join(key);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(TargetError::Io(format!("{}: {e}", path.display()))),
    };
    ron::from_str(&text).map(Some).map_err(|e| {
        TargetError::Invalid(format!(
            "COSMIC Terminal config changed shape ({key}: {e}); nothing written"
        ))
    })
}

/// The exact bytes `cosmic_config::ConfigSet::set` writes for `value`.
fn ron_text<T: Serialize>(value: &T) -> String {
    ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new())
        .expect("COSMIC Terminal config values serialise")
}

/// The id a palette's scheme takes: the lowest id with its name, else one past the highest.
fn scheme_id(schemes: &Schemes, name: &str) -> Result<u64, TargetError> {
    if let Some((&id, _)) = schemes.iter().find(|(_, s)| s.name == name) {
        return Ok(id);
    }
    match schemes.keys().next_back() {
        None => Ok(0),
        Some(last) => last
            .checked_add(1)
            .ok_or_else(|| TargetError::Invalid("no free COSMIC Terminal colour scheme id".into())),
    }
}

pub(crate) fn plan(palettes: &[&Palette], dirs: &Dirs) -> Result<NativePlan, TargetError> {
    super::check_palettes(palettes)?;

    let dir = config_dir(dirs);
    let mut dark: Schemes = read_key(&dir, "color_schemes_dark")?.unwrap_or_default();
    let mut light: Schemes = read_key(&dir, "color_schemes_light")?.unwrap_or_default();
    let default_profile: Option<u64> = read_key::<Option<u64>>(&dir, "default_profile")?.flatten();
    let mut profiles: BTreeMap<u64, Profile> = read_key(&dir, "profiles")?.unwrap_or_default();
    let app_theme: Option<AppTheme> = read_key(&dir, "app_theme")?;

    let mut out = NativePlan::default();
    let mut profile = default_profile.and_then(|id| profiles.get_mut(&id));
    let profile_found = profile.is_some();

    for p in palettes {
        let schemes = match p.mode {
            Mode::Dark => &mut dark,
            Mode::Light => &mut light,
        };
        let id = scheme_id(schemes, &p.name)?;
        schemes.insert(id, derive(p).term.to_scheme(&p.name));
        if let Some(profile) = profile.as_deref_mut() {
            match p.mode {
                Mode::Dark => profile.syntax_theme_dark = p.name.clone(),
                Mode::Light => profile.syntax_theme_light = p.name.clone(),
            }
        }
        out.writes.push(KeyFileWrite {
            path: dir.join(format!("color_schemes_{}", p.mode)),
            contents: ron_text(schemes),
        });
        out.writes.push(KeyFileWrite {
            path: dir.join(format!("syntax_theme_{}", p.mode)),
            contents: ron_text(&p.name),
        });
    }

    if profile_found {
        out.writes.push(KeyFileWrite {
            path: dir.join("profiles"),
            contents: ron_text(&profiles),
        });
    }

    let fixed = match app_theme {
        Some(AppTheme::Dark) => Some("Dark"),
        Some(AppTheme::Light) => Some("Light"),
        Some(AppTheme::System) | None => None,
    };
    if let Some(theme) = fixed {
        out.warnings.push(format!(
            "COSMIC Terminal is set to {theme}; it will not follow the desktop mode"
        ));
    }
    Ok(out)
}

/// `cosmic-term/<Stem>-term.ron` and the `syntax_theme_<mode>` that selects it.
pub(crate) fn export(p: &Palette) -> Vec<ExportFile> {
    vec![
        ExportFile {
            path: format!("cosmic-term/{}-term.ron", p.stem()),
            contents: derive(p).term.to_scheme(&p.name).to_ron() + "\n",
        },
        ExportFile {
            path: format!(
                "cosmic/config/{CONFIG_ID}/v{VERSION}/syntax_theme_{}",
                p.mode
            ),
            contents: ron_text(&p.name),
        },
    ]
}
