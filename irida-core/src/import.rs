//! Read an existing `cosmic-<slug>-theme` rice repo into a palette.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use cosmic_theme::ThemeBuilder;
use cosmic_theme::palette::{Srgb, Srgba};
use regex::{Captures, NoExpand, Regex};

use crate::color::Rgb;
use crate::derive::derive;
use crate::export_docs::{capitalised, name_of, opacity, readme_names};
use crate::palette::{Mode, Palette, Roles, validate_name, validate_slug};
use crate::role::Role;
use crate::slot::{Ansi, PinValue, Pins, Slot, Tone};
use crate::term::ColorScheme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    pub path: PathBuf,
    pub message: String,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for ImportError {}

fn err(path: &Path, message: impl Into<String>) -> ImportError {
    ImportError {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

/// The slug of a folder named `cosmic-<slug>-theme`, read from its canonical path.
fn rice_slug(dir: &Path) -> Option<String> {
    let canonical = dir.canonicalize().ok()?;
    let name = canonical.file_name()?.to_str()?;
    let slug = name.strip_prefix("cosmic-")?.strip_suffix("-theme")?;
    validate_slug(slug).ok()?;
    Some(slug.to_string())
}

/// Whether `dir` is a folder named `cosmic-<slug>-theme`.
pub fn is_rice_folder(dir: &Path) -> bool {
    dir.is_dir() && rice_slug(dir).is_some()
}

fn read(path: &Path) -> Result<String, ImportError> {
    fs::read_to_string(path).map_err(|e| err(path, e.to_string()))
}

/// The files in `dir` whose names end in `suffix`, sorted.
fn files_ending(dir: &Path, suffix: &str) -> Result<Vec<PathBuf>, ImportError> {
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(err(dir, e.to_string())),
    };
    let mut out: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(suffix))
        })
        .collect();
    out.sort();
    Ok(out)
}

fn names_of(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn from_srgba(c: Srgba) -> Rgb {
    let c: cosmic_theme::palette::Srgba<u8> = c.into_format();
    Rgb {
        r: c.red,
        g: c.green,
        b: c.blue,
    }
}

fn from_srgb(c: Srgb) -> Rgb {
    let c: Srgb<u8> = c.into_format();
    Rgb {
        r: c.red,
        g: c.green,
        b: c.blue,
    }
}

fn hex(s: &str) -> Rgb {
    Rgb::from_hex(s).expect("the patterns only match #RRGGBB")
}

/// A Python string literal in `scripts/contrast.py`, its text captured.
const PY_STR: &str = r#""((?:[^"\\]|\\.)*)""#;

/// A role line of `ROLES`: indent, role, name, colour, reads.
fn role_line() -> Regex {
    Regex::new(&format!(
        r#"(?m)^(\s*)\("(Text|Accent|Success|Warning|Error|Blue|Magenta|Cyan|Muted|Comment)", {PY_STR}, "(#[0-9A-Fa-f]{{6}})", (True|False)\),$"#
    ))
    .expect("pattern compiles")
}

/// A surface line of `SURFACES`: indent, label, name, colours.
fn surface_line() -> Regex {
    Regex::new(&format!(
        r#"(?m)^(\s*)\("(Background|Containers|Terminal)", {PY_STR}, \[([^\]]*)\]\),$"#
    ))
    .expect("pattern compiles")
}

fn constant_line(key: &str) -> Regex {
    Regex::new(&format!(r#"(?m)^{key} = "(#[0-9A-Fa-f]{{6}})"$"#)).expect("pattern compiles")
}

/// A Python string's text: `\xHH` is that character, and every other
/// backslash keeps only the character after it.
fn py_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('x') => {
                    let digits: String = chars.clone().take(2).collect();
                    if digits.len() == 2 && digits.chars().all(|d| d.is_ascii_hexdigit()) {
                        let b = u8::from_str_radix(&digits, 16).expect("two hex digits");
                        out.push(char::from(b));
                        chars.nth(1);
                    } else {
                        out.push('x');
                    }
                }
                next => out.extend(next),
            },
            c => out.push(c),
        }
    }
    out
}

/// `s` as the inside of a Python string: backslash and quote escaped, and
/// control characters written `\xHH`, so the line stays valid Python.
fn py_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

/// [`py_escape`] with `/` written `\x2f`, so the ` / ` between the container
/// and pane names of `Containers` is the only `/` in the line.
fn py_escape_half(s: &str) -> String {
    py_escape(s).replace('/', r"\x2f")
}

/// Path 1: the roles and names in `scripts/contrast.py`, when it has exactly
/// one of each. `None` when the file is absent or does not have that shape.
/// A name that is the capitalised role name is the default, so it is left
/// out, as in the README.
fn roles_from_contrast(text: &str) -> Option<(BTreeMap<Role, Rgb>, BTreeMap<Role, String>)> {
    let one = |re: Regex| -> Option<Rgb> {
        let found: Vec<_> = re.captures_iter(text).collect();
        match found.as_slice() {
            [c] => Some(hex(&c[1])),
            _ => None,
        }
    };
    let mut roles = BTreeMap::new();
    let mut names = BTreeMap::new();
    let mut keep = |role: Role, name: String| {
        if !name.trim().is_empty() && name != capitalised(role) {
            names.insert(role, name);
        }
    };
    roles.insert(Role::Background, one(constant_line("WINDOW"))?);
    roles.insert(Role::Pane, one(constant_line("PANE"))?);
    roles.insert(
        Role::Container,
        one(
            Regex::new(r#"(?m)^\s*\("Containers", "(?:[^"\\]|\\.)*", \["(#[0-9A-Fa-f]{6})""#)
                .expect("pattern compiles"),
        )?,
    );

    let found: Vec<_> = role_line().captures_iter(text).collect();
    if found.len() != 10 {
        return None;
    }
    for c in &found {
        let role: Role = c[2].to_lowercase().parse().ok()?;
        if roles.insert(role, hex(&c[4])).is_some() {
            return None;
        }
        keep(role, py_unescape(&c[3]));
    }

    let found: Vec<_> = surface_line().captures_iter(text).collect();
    if found.len() != 3 {
        return None;
    }
    let mut seen = Vec::new();
    for c in &found {
        let label = c[2].to_string();
        if seen.contains(&label) {
            return None;
        }
        match label.as_str() {
            "Background" => {
                keep(Role::Background, py_unescape(&c[3]));
            }
            "Containers" => {
                // Split before unescaping: a `/` in a name is written `\x2f`.
                let (container, pane) = c[3].split_once(" / ")?;
                keep(Role::Container, py_unescape(container));
                keep(Role::Pane, py_unescape(pane));
            }
            _ => {}
        }
        seen.push(label);
    }
    Some((roles, names))
}

/// `scripts/contrast.py` with its palette lines rebuilt from `p`: `WINDOW`,
/// `PANE`, the three surfaces and the ten roles, each in place, so the
/// script's own code and every other line stay as written. `None` when the
/// file does not have the shape path 1 reads.
pub(crate) fn refresh_contrast(text: &str, p: &Palette) -> Option<String> {
    roles_from_contrast(text)?;
    let hex = |role: Role| p.roles.get(role).to_hex();
    let name = |role: Role| py_escape(&name_of(p, role));
    let text = constant_line("WINDOW").replace(
        text,
        NoExpand(&format!(r#"WINDOW = "{}""#, hex(Role::Background))),
    );
    let text =
        constant_line("PANE").replace(&text, NoExpand(&format!(r#"PANE = "{}""#, hex(Role::Pane))));
    let text = surface_line().replace_all(&text, |c: &Captures| {
        let (label, colors) = match &c[2] {
            "Background" => (name(Role::Background), vec![hex(Role::Background)]),
            "Containers" => (
                format!(
                    "{} / {}",
                    py_escape_half(&name_of(p, Role::Container)),
                    py_escape_half(&name_of(p, Role::Pane))
                ),
                vec![hex(Role::Container), hex(Role::Pane)],
            ),
            _ => (
                format!("{}, {}", name(Role::Pane), opacity(p)),
                vec![hex(Role::Pane)],
            ),
        };
        let colors = colors
            .iter()
            .map(|h| format!("\"{h}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!(r#"{}("{}", "{label}", [{colors}]),"#, &c[1], &c[2])
    });
    let text = role_line().replace_all(&text, |c: &Captures| {
        let role: Role = c[2]
            .to_lowercase()
            .parse()
            .expect("the pattern names roles");
        format!(
            r#"{}("{}", "{}", "{}", {}),"#,
            &c[1],
            &c[2],
            name(role),
            hex(role),
            &c[5]
        )
    });
    Some(text.into_owned())
}

/// The `cmt` colour of the repo's Neovim colorscheme.
fn comment_from_nvim(dir: &Path, slug: &str) -> Result<Rgb, ImportError> {
    let colors = dir.join("nvim/colors");
    let named = colors.join(format!("{slug}.lua"));
    let file = if named.is_file() {
        named
    } else {
        match files_ending(&colors, ".lua")?.as_slice() {
            [only] => only.clone(),
            _ => {
                return Err(err(
                    &colors,
                    format!("no {slug}.lua to read the comment colour (cmt) from"),
                ));
            }
        }
    };
    let text = read(&file)?;
    let re = Regex::new(r#"\bcmt\s*=\s*"(#[0-9A-Fa-f]{6})""#).expect("pattern compiles");
    let found: Vec<_> = re.captures_iter(&text).collect();
    match found.as_slice() {
        [c] => Ok(hex(&c[1])),
        _ => Err(err(
            &file,
            format!(
                "expected one cmt colour, found {}; cannot read the comment colour",
                found.len()
            ),
        )),
    }
}

/// Path 2: the desktop theme, the terminal scheme and the Neovim comment colour.
fn roles_from_files(
    dir: &Path,
    slug: &str,
    ron_path: &Path,
    builder: &ThemeBuilder,
    scheme_path: &Path,
    scheme: &ColorScheme,
) -> Result<BTreeMap<Role, Rgb>, ImportError> {
    let need_a = |key: &str, c: Option<Srgba>| {
        c.map(from_srgba)
            .ok_or_else(|| err(ron_path, format!("{key} is not set")))
    };
    let need_b = |key: &str, c: Option<Srgb>| {
        c.map(from_srgb)
            .ok_or_else(|| err(ron_path, format!("{key} is not set")))
    };
    let slot = |s: Slot| {
        scheme
            .slot(s)
            .ok_or_else(|| err(scheme_path, format!("{s} is not set")))
    };
    let background = need_a("bg_color", builder.bg_color)?;
    let mut roles = BTreeMap::new();
    roles.insert(Role::Background, background);
    roles.insert(
        Role::Container,
        need_a("primary_container_bg", builder.primary_container_bg)?,
    );
    roles.insert(Role::Accent, need_b("accent", builder.accent)?);
    roles.insert(Role::Success, need_b("success", builder.success)?);
    roles.insert(Role::Warning, need_b("warning", builder.warning)?);
    roles.insert(Role::Error, need_b("destructive", builder.destructive)?);
    roles.insert(
        Role::Pane,
        scheme.slot(Slot::Background).unwrap_or(background),
    );
    roles.insert(Role::Text, slot(Slot::Foreground)?);
    roles.insert(Role::Muted, slot(Slot::DimForeground)?);
    roles.insert(Role::Blue, slot(Slot::Ansi(Tone::Normal, Ansi::Blue))?);
    roles.insert(
        Role::Magenta,
        slot(Slot::Ansi(Tone::Normal, Ansi::Magenta))?,
    );
    roles.insert(Role::Cyan, slot(Slot::Ansi(Tone::Normal, Ansi::Cyan))?);
    roles.insert(Role::Comment, comment_from_nvim(dir, slug)?);
    Ok(roles)
}

/// The terminal scheme slots `roles_from_files` reads roles from.
const ROLE_SOURCES: [Slot; 6] = [
    Slot::Background,
    Slot::Foreground,
    Slot::DimForeground,
    Slot::Ansi(Tone::Normal, Ansi::Blue),
    Slot::Ansi(Tone::Normal, Ansi::Magenta),
    Slot::Ansi(Tone::Normal, Ansi::Cyan),
];

/// One note per slot in `used` whose scheme colour is not opaque.
fn alpha_notes(scheme: &ColorScheme, used: &BTreeSet<Slot>) -> Vec<String> {
    used.iter()
        .filter_map(|&slot| {
            let c = scheme.raw(slot)?;
            (c.a < 255).then(|| {
                format!(
                    "{slot}: alpha {} in the terminal scheme was dropped; Irida colours are opaque",
                    c.a
                )
            })
        })
        .collect()
}

/// Pin every slot the scheme sets to something other than its derived value,
/// in stages so a pinned normal colour is not pinned again in its brights and dims.
fn pin_scheme(p: &mut Palette, scheme: &ColorScheme) {
    let stages: [Vec<Slot>; 3] = [
        Ansi::ALL.map(|a| Slot::Ansi(Tone::Normal, a)).to_vec(),
        Ansi::ALL.map(|a| Slot::Ansi(Tone::Bright, a)).to_vec(),
        Ansi::ALL
            .map(|a| Slot::Ansi(Tone::Dim, a))
            .into_iter()
            .chain([
                Slot::Foreground,
                Slot::Cursor,
                Slot::BrightForeground,
                Slot::DimForeground,
            ])
            .collect(),
    ];
    for stage in stages {
        let derived = derive(p);
        for slot in stage {
            if let Some(c) = scheme.slot(slot)
                && derived.slot(slot) != Some(c)
            {
                p.pins
                    .set(slot, PinValue::Color(c))
                    .expect("colour pins are always allowed");
            }
        }
    }
    let derived = derive(p);
    match scheme.slot(Slot::Background) {
        None => p
            .pins
            .set(Slot::Background, PinValue::None)
            .expect("background can be none"),
        Some(c) if derived.slot(Slot::Background) != Some(c) => {
            p.pins
                .set(Slot::Background, PinValue::Color(c))
                .expect("colour pins are always allowed");
        }
        Some(_) => {}
    }
}

/// The one `cosmic/*.ron`, parsed.
fn read_builder(dir: &Path) -> Result<(PathBuf, String, ThemeBuilder), ImportError> {
    let cosmic = dir.join("cosmic");
    let rons = files_ending(&cosmic, ".ron")?;
    let [path] = rons.as_slice() else {
        return Err(err(
            &cosmic,
            format!(
                "expected exactly one cosmic/*.ron, found {}{}",
                rons.len(),
                if rons.is_empty() {
                    String::new()
                } else {
                    format!(": {}", names_of(&rons))
                }
            ),
        ));
    };
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    let builder: ThemeBuilder =
        ron::from_str(&read(path)?).map_err(|e| err(path, format!("not a COSMIC theme: {e}")))?;
    Ok((path.clone(), stem, builder))
}

/// The terminal scheme `syntax_theme_<mode>` names, and its file.
fn read_scheme(
    dir: &Path,
    stem: &str,
    mode: Mode,
) -> Result<(String, PathBuf, ColorScheme), ImportError> {
    let key = dir.join(format!(
        "cosmic/config/com.system76.CosmicTerm/v1/syntax_theme_{mode}"
    ));
    let selected: String =
        ron::from_str(&read(&key)?).map_err(|e| err(&key, format!("not a scheme name: {e}")))?;

    let term = dir.join("cosmic-term");
    let preferred = term.join(format!("{stem}-term.ron"));
    let mut candidates = Vec::new();
    if preferred.is_file() {
        candidates.push(preferred.clone());
    }
    candidates.extend(
        files_ending(&term, ".ron")?
            .into_iter()
            .filter(|p| *p != preferred),
    );
    let mut read_files = Vec::new();
    for path in candidates {
        let scheme = ColorScheme::from_ron(&read(&path)?)
            .map_err(|e| err(&path, format!("not a COSMIC Terminal scheme: {e}")))?;
        if scheme.name == selected {
            return Ok((selected, path, scheme));
        }
        read_files.push(path);
    }
    Err(err(
        &term,
        format!(
            "no scheme named {selected:?} (selected by syntax_theme_{mode}); read: {}",
            if read_files.is_empty() {
                "no files".to_string()
            } else {
                names_of(&read_files)
            }
        ),
    ))
}

/// Read a rice repo folder into a palette. Slots that differ from their
/// derived values become pins, so the terminal scheme round-trips.
pub fn import_rice(dir: &Path) -> Result<Palette, ImportError> {
    import_rice_with_notes(dir).map(|(p, _)| p)
}

/// [`import_rice`], plus a note for every terminal scheme colour whose alpha
/// was dropped on the way into a role or a pin.
pub fn import_rice_with_notes(dir: &Path) -> Result<(Palette, Vec<String>), ImportError> {
    let slug = rice_slug(dir).ok_or_else(|| {
        err(
            dir,
            "not a rice repo (the folder must be named cosmic-<slug>-theme)",
        )
    })?;
    let dir = &dir.canonicalize().map_err(|e| err(dir, e.to_string()))?;

    let (ron_path, stem, builder) = read_builder(dir)?;
    let mode = if builder.palette.is_dark() {
        Mode::Dark
    } else {
        Mode::Light
    };

    let (name, scheme_path, scheme) = read_scheme(dir, &stem, mode)?;
    if name.replace(' ', "") != stem {
        return Err(err(
            dir,
            format!(
                "syntax_theme_{mode} selects \"{name}\" but cosmic/{stem}.ron names {stem}; \
                 select the {stem} scheme or rename the files"
            ),
        ));
    }
    validate_name(&name).map_err(|e| err(&scheme_path, e.to_string()))?;

    let contrast = dir.join("scripts/contrast.py");
    let from_contrast = if contrast.is_file() {
        roles_from_contrast(&read(&contrast)?)
    } else {
        None
    };
    let mut used = BTreeSet::new();
    let (roles, names) = match from_contrast {
        Some(found) => found,
        None => {
            let roles = roles_from_files(dir, &slug, &ron_path, &builder, &scheme_path, &scheme)?;
            used.extend(ROLE_SOURCES);
            // Display names are optional: an unreadable README gives none.
            let names = fs::read_to_string(dir.join("README.md"))
                .map(|t| readme_names(&t))
                .unwrap_or_default();
            (roles, names)
        }
    };
    let roles = Roles::new(Role::ALL.map(|r| roles[&r]));

    let mut p = Palette {
        name,
        slug,
        mode,
        pair: None,
        notes: None,
        roles,
        names,
        pins: Pins::default(),
    };
    pin_scheme(&mut p, &scheme);
    used.extend(p.pins.iter().map(|(slot, _)| slot));
    let notes = alpha_notes(&scheme, &used);

    let inner = builder.palette.as_ref();
    let derived = derive(&p);
    let neutral_10 = from_srgba(inner.neutral_10);
    if derived.neutral_10 != neutral_10 {
        p.pins
            .set(Slot::CosmicNeutral10, PinValue::Color(neutral_10))
            .expect("colour pins are always allowed");
    }
    let gray_2 = from_srgba(inner.gray_2);
    if derived.gray_2 != gray_2 {
        p.pins
            .set(Slot::CosmicGray2, PinValue::Color(gray_2))
            .expect("colour pins are always allowed");
    }
    Ok((p, notes))
}
