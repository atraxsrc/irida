//! COSMIC desktop: the dark or light theme builder, its built theme, and the mode.
//!
//! COSMIC's own config writer is only ever pointed at a temporary folder. The
//! user's builder keys are copied there, read, overlaid with the palette's
//! colours, and written out again with the built `Theme`; the resulting key
//! files are handed back as ordinary writes.

use std::fs;
use std::path::{Path, PathBuf};

use cosmic_config::{Config, CosmicConfigEntry};
use cosmic_theme::palette::{Srgb, Srgba};
use cosmic_theme::{
    AlphaMap, BlurStrength, CosmicPalette, CosmicPaletteInner, DARK_THEME_BUILDER_ID,
    DARK_THEME_ID, LIGHT_THEME_BUILDER_ID, LIGHT_THEME_ID, Spacing, THEME_MODE_ID, Theme,
    ThemeBuilder,
};

use super::{ExportFile, KeyFileWrite, NativePlan};
use crate::color::{Rgb, mix};
use crate::derive::derive;
use crate::dirs::Dirs;
use crate::palette::{Mode, Palette};
use crate::role::Role;
use crate::target::TargetError;

/// The builder keys Irida writes; every other builder key stays the user's.
pub const COLOR_KEYS: [&str; 11] = [
    "palette",
    "neutral_tint",
    "bg_color",
    "primary_container_bg",
    "secondary_container_bg",
    "text_tint",
    "accent",
    "success",
    "warning",
    "destructive",
    "window_hint",
];

fn srgb(c: Rgb) -> Srgb {
    Srgb::<u8>::new(c.r, c.g, c.b).into_format()
}

fn srgba(c: Rgb) -> Srgba {
    Srgba::<u8>::new(c.r, c.g, c.b, 255).into_format()
}

/// `neutral_0..10`: a linear ramp from background (dark) or container (light) to `neutral_10`.
fn neutrals(p: &Palette) -> [Rgb; 11] {
    let d = derive(p);
    let n0 = match p.mode {
        Mode::Dark => p.roles.get(Role::Background),
        Mode::Light => p.roles.get(Role::Container),
    };
    std::array::from_fn(|k| mix(n0, d.neutral_10, k as f64 / 10.0))
}

pub fn cosmic_palette(p: &Palette) -> CosmicPalette {
    let r = |role: Role| srgba(p.roles.get(role));
    let d = derive(p);
    let n = neutrals(p).map(srgba);
    let inner = CosmicPaletteInner {
        name: p.name.replace(' ', "-"),
        bright_red: r(Role::Error),
        bright_green: r(Role::Success),
        bright_orange: r(Role::Accent),
        gray_1: r(Role::Container),
        gray_2: srgba(d.gray_2),
        neutral_0: n[0],
        neutral_1: n[1],
        neutral_2: n[2],
        neutral_3: n[3],
        neutral_4: n[4],
        neutral_5: n[5],
        neutral_6: n[6],
        neutral_7: n[7],
        neutral_8: n[8],
        neutral_9: n[9],
        neutral_10: n[10],
        accent_blue: r(Role::Blue),
        accent_indigo: r(Role::Magenta),
        accent_purple: r(Role::Magenta),
        accent_pink: r(Role::Error),
        accent_red: r(Role::Error),
        accent_orange: r(Role::Accent),
        accent_yellow: r(Role::Warning),
        accent_green: r(Role::Success),
        accent_warm_grey: r(Role::Muted),
        ext_warm_grey: r(Role::Muted),
        ext_orange: r(Role::Accent),
        ext_yellow: r(Role::Warning),
        ext_blue: r(Role::Cyan),
        ext_purple: r(Role::Magenta),
        ext_pink: r(Role::Error),
        ext_indigo: r(Role::Magenta),
    };
    match p.mode {
        Mode::Dark => CosmicPalette::Dark(inner),
        Mode::Light => CosmicPalette::Light(inner),
    }
}

/// Set the 11 colour keys from the palette.
pub fn overlay(b: &mut ThemeBuilder, p: &Palette) {
    let r = |role: Role| p.roles.get(role);
    b.palette = cosmic_palette(p);
    b.neutral_tint = Some(srgb(neutrals(p)[5]));
    b.bg_color = Some(srgba(r(Role::Background)));
    b.primary_container_bg = Some(srgba(r(Role::Container)));
    b.secondary_container_bg = Some(srgba(r(Role::Pane)));
    b.text_tint = Some(srgb(r(Role::Text)));
    b.accent = Some(srgb(r(Role::Accent)));
    b.success = Some(srgb(r(Role::Success)));
    b.warning = Some(srgb(r(Role::Warning)));
    b.destructive = Some(srgb(r(Role::Error)));
    b.window_hint = Some(srgb(r(Role::Accent)));
}

/// The rices' shared house style: square-ish corners, low frosting, tight gaps.
pub fn house_style(b: &mut ThemeBuilder) {
    b.spacing = Spacing::default();
    let radii = &mut b.corner_radii;
    radii.radius_0 = [0.0; 4];
    radii.radius_xs = [2.0; 4];
    radii.radius_s = [2.0; 4];
    radii.radius_m = [2.0; 4];
    radii.radius_l = [2.0; 4];
    radii.radius_xl = [2.0; 4];
    b.frosted = BlurStrength::Low;
    b.gaps = (0, 7);
    b.active_hint = 1;
    b.frosted_windows = true;
    b.frosted_system_interface = true;
    b.frosted_panel = true;
    b.frosted_applets = true;
    b.frosted_maximized_apps = false;
    b.alpha_map = AlphaMap::default().offset(0.06);
}

/// `cosmic/<Stem>.ron`: the house style plus the palette, as Settings imports it.
pub fn export_ron(p: &Palette) -> String {
    let mut b = ThemeBuilder::default();
    house_style(&mut b);
    overlay(&mut b, p);
    ron::ser::to_string_pretty(&b, ron::ser::PrettyConfig::new())
        .expect("a theme builder serialises")
        + "\n"
}

pub(crate) fn export(p: &Palette) -> Vec<ExportFile> {
    vec![ExportFile {
        path: format!("cosmic/{}.ron", p.stem()),
        contents: export_ron(p),
    }]
}

fn cosmic_err(e: cosmic_config::Error) -> TargetError {
    TargetError::Io(format!("COSMIC theme: {e}"))
}

fn io_err(path: &Path, e: std::io::Error) -> TargetError {
    TargetError::Io(format!("COSMIC theme: {}: {e}", path.display()))
}

/// `<root>/cosmic/<id>/v<version>`, where cosmic-config keeps one file per key.
fn key_dir(root: &Path, id: &str, version: u64) -> PathBuf {
    root.join("cosmic").join(id).join(format!("v{version}"))
}

/// Copy the regular files of `from`, when it exists, into `to`. A key that is
/// a symlink to a file (a dotfile manager's link) counts, as it does for COSMIC.
fn copy_keys(from: &Path, to: &Path) -> Result<(), TargetError> {
    let rd = match fs::read_dir(from) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io_err(from, e)),
    };
    fs::create_dir_all(to).map_err(|e| io_err(to, e))?;
    for entry in rd {
        let entry = entry.map_err(|e| io_err(from, e))?;
        if entry.path().is_file() {
            fs::copy(entry.path(), to.join(entry.file_name()))
                .map_err(|e| io_err(&entry.path(), e))?;
        }
    }
    Ok(())
}

/// The key files below `dir`, sorted by key.
fn read_keys(dir: &Path) -> Result<Vec<(String, String)>, TargetError> {
    let mut keys = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| io_err(dir, e))? {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        let text = fs::read_to_string(entry.path()).map_err(|e| io_err(&entry.path(), e))?;
        keys.push((entry.file_name().to_string_lossy().into_owned(), text));
    }
    keys.sort();
    Ok(keys)
}

/// Plan the builder and theme writes of one palette, staged in `stage`.
fn plan_one(
    p: &Palette,
    dirs: &Dirs,
    stage: &Path,
    out: &mut NativePlan,
) -> Result<(), TargetError> {
    let (builder_id, theme_id) = match p.mode {
        Mode::Dark => (DARK_THEME_BUILDER_ID, DARK_THEME_ID),
        Mode::Light => (LIGHT_THEME_BUILDER_ID, LIGHT_THEME_ID),
    };
    let stage_in = stage.join("in");
    let stage_out = stage.join("out");
    let version = ThemeBuilder::VERSION;
    for v in [version, version - 1] {
        copy_keys(
            &key_dir(&dirs.config, builder_id, v),
            &key_dir(&stage_in, builder_id, v),
        )?;
    }

    let base = Config::with_custom_path(builder_id, version, stage_in).map_err(cosmic_err)?;
    let mut builder = match ThemeBuilder::get_entry(&base) {
        Ok(b) => b,
        Err((errors, partial)) => {
            out.warnings.push(format!(
                "COSMIC {} theme: {} unreadable keys, defaults used",
                p.mode,
                errors.len()
            ));
            partial
        }
    };
    overlay(&mut builder, p);
    let theme: Theme = builder.clone().build();

    let builder_out =
        Config::with_custom_path(builder_id, version, stage_out.clone()).map_err(cosmic_err)?;
    builder.write_entry(&builder_out).map_err(cosmic_err)?;
    let theme_out = Config::with_custom_path(theme_id, Theme::VERSION, stage_out.clone())
        .map_err(cosmic_err)?;
    theme.write_entry(&theme_out).map_err(cosmic_err)?;

    let real_builder = key_dir(&dirs.config, builder_id, version);
    for (key, contents) in read_keys(&key_dir(&stage_out, builder_id, version))? {
        if COLOR_KEYS.contains(&key.as_str()) {
            out.writes.push(KeyFileWrite {
                path: real_builder.join(key),
                contents,
            });
        }
    }
    let real_theme = key_dir(&dirs.config, theme_id, Theme::VERSION);
    for (key, contents) in read_keys(&key_dir(&stage_out, theme_id, Theme::VERSION))? {
        out.writes.push(KeyFileWrite {
            path: real_theme.join(key),
            contents,
        });
    }
    Ok(())
}

pub(crate) fn plan(palettes: &[&Palette], dirs: &Dirs) -> Result<NativePlan, TargetError> {
    super::check_palettes(palettes)?;
    let tmp =
        tempfile::TempDir::new().map_err(|e| TargetError::Io(format!("COSMIC theme: {e}")))?;
    let mut out = NativePlan::default();
    for (i, p) in palettes.iter().enumerate() {
        plan_one(p, dirs, &tmp.path().join(i.to_string()), &mut out)?;
    }

    if let [p] = palettes {
        let mode_dir = key_dir(&dirs.config, THEME_MODE_ID, 1);
        out.writes.push(KeyFileWrite {
            path: mode_dir.join("is_dark"),
            contents: (p.mode == Mode::Dark).to_string(),
        });
        let auto = fs::read_to_string(mode_dir.join("auto_switch"))
            .ok()
            .and_then(|t| ron::from_str::<bool>(&t).ok())
            .unwrap_or(false);
        if auto {
            out.warnings.push(format!(
                "COSMIC switches between dark and light automatically; it may switch away from {}",
                p.name
            ));
        }
    }
    Ok(out)
}
