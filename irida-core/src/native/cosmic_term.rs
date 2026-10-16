//! COSMIC Terminal: colour schemes and syntax themes in its cosmic-config key files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{ExportFile, KeyFileWrite, NativePlan, NativeUndo};
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
    let profile_id = default_profile.filter(|id| profiles.contains_key(id));
    let mut profile_undo = Vec::new();

    for p in palettes {
        let schemes = match p.mode {
            Mode::Dark => &mut dark,
            Mode::Light => &mut light,
        };
        let id = scheme_id(schemes, &p.name)?;
        let before = schemes.get(&id).map(ron_text);
        schemes.insert(id, derive(p).term.to_scheme(&p.name));
        if let Some(pid) = profile_id
            && let Some(profile) = profiles.get_mut(&pid)
        {
            let field = theme_field(p.mode);
            let value = profile_theme(profile, field).expect("a syntax theme field");
            profile_undo.push(NativeUndo::TermProfileTheme {
                profile: pid,
                field: field.to_string(),
                before: Some(ron_text(value)),
            });
            *value = p.name.clone();
        }
        out.writes.push(KeyFileWrite {
            path: dir.join(format!("color_schemes_{}", p.mode)),
            contents: ron_text(schemes),
            undo: vec![NativeUndo::TermScheme {
                mode: p.mode,
                id,
                before,
            }],
        });
        out.writes.push(KeyFileWrite {
            path: dir.join(format!("syntax_theme_{}", p.mode)),
            contents: ron_text(&p.name),
            undo: Vec::new(),
        });
    }

    if profile_id.is_some() {
        out.writes.push(KeyFileWrite {
            path: dir.join("profiles"),
            contents: ron_text(&profiles),
            undo: profile_undo,
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

/// The profile field that selects a mode's syntax theme.
fn theme_field(mode: Mode) -> &'static str {
    match mode {
        Mode::Dark => "syntax_theme_dark",
        Mode::Light => "syntax_theme_light",
    }
}

/// A profile's syntax theme field by name; `None` for any other name.
fn profile_theme<'p>(profile: &'p mut Profile, field: &str) -> Option<&'p mut String> {
    match field {
        "syntax_theme_dark" => Some(&mut profile.syntax_theme_dark),
        "syntax_theme_light" => Some(&mut profile.syntax_theme_light),
        _ => None,
    }
}

/// A key file after undoing one of Irida's changes in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Undone {
    /// The new text, and whether its map is now empty.
    Text { text: String, empty: bool },
    /// The current file no longer parses; only the recorded copy can restore it.
    Unparsable,
}

fn bad_record(what: &str, e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("apply record: {what}: {e}"),
    )
}

/// `color_schemes_<mode>` text with the scheme at `id` put back as `before`
/// (its RON), or removed when `before` is `None`. Every other scheme is kept.
pub(crate) fn undo_scheme(current: &str, id: u64, before: Option<&str>) -> std::io::Result<Undone> {
    let Ok(mut schemes) = ron::from_str::<Schemes>(current) else {
        return Ok(Undone::Unparsable);
    };
    match before {
        Some(text) => {
            let scheme: ColorScheme =
                ron::from_str(text).map_err(|e| bad_record("colour scheme", e))?;
            schemes.insert(id, scheme);
        }
        None => {
            schemes.remove(&id);
        }
    }
    Ok(Undone::Text {
        text: ron_text(&schemes),
        empty: schemes.is_empty(),
    })
}

/// `profiles` text with one profile's syntax theme `field` put back as
/// `before` (its RON), or emptied when `None`. A profile that is gone is left
/// gone, and the text is returned unchanged.
pub(crate) fn undo_profile_theme(
    current: &str,
    profile: u64,
    field: &str,
    before: Option<&str>,
) -> std::io::Result<Undone> {
    let Ok(mut profiles) = ron::from_str::<BTreeMap<u64, Profile>>(current) else {
        return Ok(Undone::Unparsable);
    };
    let old = match before {
        Some(text) => ron::from_str::<String>(text).map_err(|e| bad_record(field, e))?,
        None => String::new(),
    };
    let Some(p) = profiles.get_mut(&profile) else {
        return Ok(Undone::Text {
            text: current.to_string(),
            empty: false,
        });
    };
    let value = profile_theme(p, field)
        .ok_or_else(|| bad_record("profile field", format!("unknown field {field}")))?;
    *value = old;
    Ok(Undone::Text {
        text: ron_text(&profiles),
        empty: false,
    })
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
