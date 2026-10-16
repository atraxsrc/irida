//! Wallpaper images: loading pixels for a draft, a name from the file, and
//! the current COSMIC wallpaper.
//!
//! The current wallpaper is read straight from the
//! `com.system76.CosmicBackground` key files (never through `cosmic-config`,
//! which creates folders), the user's first and COSMIC's shipped defaults
//! for any key the user has not set.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use image::ImageReader;
use image::imageops::{self, FilterType};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::dirs::Dirs;
use crate::palette::slug_from_name;

/// The longest side, in pixels, a wallpaper is scaled down to for a draft.
pub const DRAFT_SIZE: u32 = 160;

/// Where COSMIC keeps its shipped defaults.
pub const SYSTEM_CONFIG: &str = "/usr/share/cosmic";

const BACKGROUND_KEYS: &str = "com.system76.CosmicBackground/v1";

const IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WallpaperError {
    /// The file was read but is not an image Irida can decode.
    NotImage(PathBuf),
    /// The file could not be read.
    Io(PathBuf, String),
}

impl fmt::Display for WallpaperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WallpaperError::NotImage(path) => write!(
                f,
                "{} could not be read as an image; try a JPEG, PNG or WebP",
                path.display()
            ),
            WallpaperError::Io(path, message) => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for WallpaperError {}

/// The image at `path` as RGBA8 bytes, scaled down (Triangle filter) so its
/// longest side is 160 px. An image no larger than that is used as is.
pub fn load_pixels(path: &Path) -> Result<Vec<u8>, WallpaperError> {
    let reader = ImageReader::open(path)
        .map_err(|e| WallpaperError::Io(path.to_path_buf(), e.to_string()))?
        .with_guessed_format()
        .map_err(|e| WallpaperError::Io(path.to_path_buf(), e.to_string()))?;
    let img = reader
        .decode()
        .map_err(|e| match e {
            image::ImageError::IoError(e) if e.kind() != io::ErrorKind::UnexpectedEof => {
                WallpaperError::Io(path.to_path_buf(), e.to_string())
            }
            _ => WallpaperError::NotImage(path.to_path_buf()),
        })?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let longest = w.max(h);
    if longest <= DRAFT_SIZE {
        return Ok(img.into_raw());
    }
    let scale = |side: u32| -> u32 {
        let scaled =
            (u64::from(side) * u64::from(DRAFT_SIZE) + u64::from(longest) / 2) / u64::from(longest);
        u32::try_from(scaled).unwrap_or(DRAFT_SIZE).max(1)
    };
    Ok(imageops::resize(&img, scale(w), scale(h), FilterType::Triangle).into_raw())
}

/// A palette name from an image file name: up to four words of its stem's
/// ASCII letters and digits, each capitalised, or `Wallpaper` when that
/// gives no name or a name without letters (no slug).
pub fn name_from_file(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let words: Vec<String> = stem
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(4)
        .map(|w| {
            let lower = w.to_ascii_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    let name = words.join(" ");
    if name.is_empty() || slug_from_name(&name).is_empty() {
        "Wallpaper".to_string()
    } else {
        name
    }
}

/// The local mirror of `cosmic_bg_config::Entry`: only what Irida reads.
#[derive(Deserialize)]
struct Entry {
    #[allow(dead_code)]
    output: String,
    source: Source,
}

#[derive(Deserialize)]
enum Source {
    Path(PathBuf),
    #[allow(dead_code)]
    Color(ron::Value),
}

/// `#true` and `#false` as `true` and `false`, outside string literals.
/// COSMIC's shipped default writes `filter_by_theme: #true`, which ron does
/// not read; a path that contains `#true` is left alone.
fn plain_bools(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        if !in_string && c == '#' {
            let tail = &text[i + 1..];
            let word = ["true", "false"].into_iter().find(|w| {
                tail.starts_with(w)
                    && !tail[w.len()..].starts_with(|n: char| n.is_alphanumeric() || n == '_')
            });
            if let Some(word) = word {
                out.push_str(word);
                i += 1 + word.len();
                continue;
            }
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// A key: `Ok(None)` when neither folder has it, `Err(())` when the one that
/// has it cannot be read or parsed.
fn read_key<T: DeserializeOwned>(user: &Path, system: &Path, key: &str) -> Result<Option<T>, ()> {
    for root in [user, system] {
        let path = root.join(BACKGROUND_KEYS).join(key);
        match fs::read_to_string(&path) {
            Ok(text) => return ron::from_str(&plain_bools(&text)).map(Some).map_err(|_| ()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(()),
        }
    }
    Ok(None)
}

/// The image an entry shows: a file as is, a folder's first image by name.
fn image_of(entry: Entry) -> Option<PathBuf> {
    let Source::Path(path) = entry.source else {
        return None;
    };
    if path.is_file() {
        return Some(path);
    }
    if !path.is_dir() {
        return None;
    }
    let mut images: Vec<PathBuf> = fs::read_dir(&path)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| IMAGE_EXTENSIONS.contains(&x.to_ascii_lowercase().as_str()))
                && p.is_file()
        })
        .collect();
    images.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    images.into_iter().next()
}

/// The current COSMIC wallpaper, read from the background keys in `user`
/// (`~/.config/cosmic`), each missing key from `system`. With one wallpaper
/// per output (`same-on-all` false and `backgrounds` naming outputs), the
/// first listed output that has an entry; otherwise `all`. A colour or
/// gradient source, a missing file, an unreadable key or a folder without an
/// image gives `None`. Nothing is written.
pub fn current_wallpaper_in(user: &Path, system: &Path) -> Option<PathBuf> {
    let same_on_all: Option<bool> = read_key(user, system, "same-on-all").ok()?;
    if same_on_all == Some(false) {
        let outputs: Vec<String> = read_key(user, system, "backgrounds")
            .ok()?
            .unwrap_or_default();
        for output in outputs {
            if let Some(entry) =
                read_key::<Entry>(user, system, &format!("output.{output}")).ok()?
            {
                return image_of(entry);
            }
        }
    }
    image_of(read_key(user, system, "all").ok()??)
}

/// The current COSMIC wallpaper for these folders (`current_wallpaper_in`
/// with `<config>/cosmic` and `/usr/share/cosmic`).
pub fn current_wallpaper(dirs: &Dirs) -> Option<PathBuf> {
    current_wallpaper_in(&dirs.config.join("cosmic"), Path::new(SYSTEM_CONFIG))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Entry, ron::error::SpannedError> {
        ron::from_str(&plain_bools(text))
    }

    #[test]
    fn colour_sources_parse_as_colours() {
        for source in [
            "Color(Single((0.1, 0.1, 0.1)))",
            "Color(Gradient((colors: [(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)], radius: 270.0)))",
        ] {
            let text = format!(
                "(output: \"all\", source: {source}, filter_by_theme: #true, \
                 rotation_frequency: 3600, filter_method: Lanczos, scaling_mode: Zoom, \
                 sampling_method: Alphanumeric)"
            );
            let entry = parse(&text).expect("a colour entry parses");
            assert!(matches!(entry.source, Source::Color(_)), "{source}");
        }
        let entry = parse("(output: \"all\", source: Path(\"/a.png\"))").unwrap();
        assert!(matches!(entry.source, Source::Path(p) if p == Path::new("/a.png")));
    }

    #[test]
    fn plain_bools_outside_strings_only() {
        assert_eq!(
            plain_bools("(a: #true, b: #false, c: \"#true \\\" #false\", d: #trueish)"),
            "(a: true, b: false, c: \"#true \\\" #false\", d: #trueish)"
        );
    }
}
