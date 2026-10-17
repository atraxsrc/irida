//! The `palette.toml` model.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::color::Rgb;
use crate::role::Role;
use crate::slot::{PinValue, Pins, Slot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dark,
    Light,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum PaletteError {
    Toml(String),
    MissingRole(Role),
    UnknownRole(String),
    BadSlug(String),
    BadName(String),
    BadPair(String),
    EmptyRoleName(Role),
    BadColor { key: String, value: String },
}

impl fmt::Display for PaletteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaletteError::Toml(m) => write!(f, "invalid palette file: {m}"),
            PaletteError::MissingRole(r) => write!(f, "missing role: {r}"),
            PaletteError::UnknownRole(s) => write!(f, "unknown role: {s}"),
            PaletteError::BadSlug(s) => write!(f, "invalid slug {s:?}: use lowercase letters only"),
            PaletteError::BadName(s) => write!(
                f,
                "invalid name {s:?}: use ASCII letters and digits in words separated by single spaces"
            ),
            PaletteError::BadPair(s) => write!(
                f,
                "invalid pair {s:?}: use the slug of another palette, lowercase letters only"
            ),
            PaletteError::EmptyRoleName(r) => write!(
                f,
                "empty display name for names.{r}: give it a name or remove it"
            ),
            PaletteError::BadColor { key, value } => {
                write!(f, "invalid colour for {key}: {value:?}")
            }
        }
    }
}

impl std::error::Error for PaletteError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roles([Rgb; 13]);

impl Roles {
    /// Build from 13 colours in `Role::ALL` order.
    pub fn new(colors: [Rgb; 13]) -> Roles {
        Roles(colors)
    }

    fn index(role: Role) -> usize {
        Role::ALL
            .iter()
            .position(|r| *r == role)
            .expect("role in ALL")
    }

    pub fn get(&self, role: Role) -> Rgb {
        self.0[Self::index(role)]
    }

    pub fn set(&mut self, role: Role, value: Rgb) {
        self.0[Self::index(role)] = value;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub name: String,
    pub slug: String,
    pub mode: Mode,
    pub pair: Option<String>,
    pub notes: Option<String>,
    /// Absolute path of the image this palette was drafted from.
    pub image: Option<String>,
    pub roles: Roles,
    pub names: BTreeMap<Role, String>,
    pub pins: Pins,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    slug: String,
    mode: Mode,
    #[serde(default)]
    pair: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    image: Option<String>,
    roles: BTreeMap<String, String>,
    #[serde(default)]
    names: BTreeMap<String, String>,
    #[serde(default)]
    pins: BTreeMap<String, String>,
}

/// Slugs are one or more lowercase ASCII letters.
pub fn validate_slug(s: &str) -> Result<(), PaletteError> {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase()) {
        Ok(())
    } else {
        Err(PaletteError::BadSlug(s.to_string()))
    }
}

/// Names are ASCII letters and digits in words separated by single spaces.
pub fn validate_name(s: &str) -> Result<(), PaletteError> {
    let ok = !s.is_empty()
        && s.split(' ')
            .all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_alphanumeric()));
    if ok {
        Ok(())
    } else {
        Err(PaletteError::BadName(s.to_string()))
    }
}

/// A slug made from a name: its ASCII letters, lowercased, everything else
/// dropped. `"Ember Day"` gives `"emberday"`; a name without letters gives `""`.
pub fn slug_from_name(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn quote(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

impl Palette {
    pub fn from_toml(s: &str) -> Result<Palette, PaletteError> {
        let raw: Raw = toml::from_str(s).map_err(|e| PaletteError::Toml(e.to_string()))?;
        validate_slug(&raw.slug)?;
        validate_name(&raw.name)?;

        let mut values: BTreeMap<Role, Rgb> = BTreeMap::new();
        for (key, value) in &raw.roles {
            let role: Role = key
                .parse()
                .map_err(|_| PaletteError::UnknownRole(key.clone()))?;
            let rgb = Rgb::from_hex(value).map_err(|_| PaletteError::BadColor {
                key: format!("roles.{key}"),
                value: value.clone(),
            })?;
            values.insert(role, rgb);
        }
        let mut arr = [crate::color::BLACK; 13];
        for (i, role) in Role::ALL.into_iter().enumerate() {
            arr[i] = *values.get(&role).ok_or(PaletteError::MissingRole(role))?;
        }

        let mut names = BTreeMap::new();
        for (key, value) in raw.names {
            let role: Role = key.parse().map_err(|_| PaletteError::UnknownRole(key))?;
            if !value.trim().is_empty() {
                names.insert(role, value);
            }
        }

        let pins = Pins::from_raw(&raw.pins)
            .map_err(|(key, value)| PaletteError::BadColor { key, value })?;

        Ok(Palette {
            name: raw.name,
            slug: raw.slug,
            mode: raw.mode,
            pair: raw.pair,
            notes: raw.notes,
            image: raw.image,
            roles: Roles(arr),
            names,
            pins,
        })
    }

    /// Check everything that must hold for `to_toml` to give a file that
    /// `from_toml` reads back: the name and slug rules, `pair` (when set) a
    /// valid slug other than this palette's, no empty `[names]` value, and
    /// `none` pinned only on `background`.
    pub fn validate(&self) -> Result<(), PaletteError> {
        validate_name(&self.name)?;
        validate_slug(&self.slug)?;
        if let Some(pair) = &self.pair
            && (validate_slug(pair).is_err() || *pair == self.slug)
        {
            return Err(PaletteError::BadPair(pair.clone()));
        }
        if let Some((role, _)) = self.names.iter().find(|(_, n)| n.trim().is_empty()) {
            return Err(PaletteError::EmptyRoleName(*role));
        }
        if let Some((slot, _)) = self
            .pins
            .iter()
            .find(|(s, v)| *v == PinValue::None && *s != Slot::Background)
        {
            return Err(PaletteError::BadColor {
                key: format!("pins.{slot}"),
                value: "none".to_string(),
            });
        }
        Ok(())
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("name = {}\n", quote(&self.name)));
        out.push_str(&format!("slug = {}\n", quote(&self.slug)));
        out.push_str(&format!("mode = \"{}\"\n", self.mode));
        if let Some(pair) = &self.pair {
            out.push_str(&format!("pair = {}\n", quote(pair)));
        }
        if let Some(notes) = &self.notes {
            out.push_str(&format!("notes = {}\n", quote(notes)));
        }
        if let Some(image) = &self.image {
            out.push_str(&format!("image = {}\n", quote(image)));
        }
        out.push_str("\n[roles]\n");
        for role in Role::ALL {
            out.push_str(&format!("{} = \"{}\"\n", role, self.roles.get(role)));
        }
        if !self.names.is_empty() {
            out.push_str("\n[names]\n");
            for role in Role::ALL {
                if let Some(n) = self.names.get(&role) {
                    out.push_str(&format!("{} = {}\n", role, quote(n)));
                }
            }
        }
        if !self.pins.is_empty() {
            out.push_str("\n[pins]\n");
            let mut entries: Vec<(String, String)> = self
                .pins
                .iter()
                .map(|(s, v)| {
                    let v = match v {
                        PinValue::Color(c) => c.to_hex(),
                        PinValue::None => "none".to_string(),
                    };
                    (s.to_string(), v)
                })
                .collect();
            entries.sort();
            for (k, v) in entries {
                out.push_str(&format!("{} = {}\n", quote(&k), quote(&v)));
            }
        }
        out
    }

    /// The name with spaces removed, used for file and theme names.
    pub fn stem(&self) -> String {
        self.name.replace(' ', "")
    }
}
