//! The effect runner: the app's IO, synchronous and free of libcosmic
//! types. Every IO effect returns its result as `Msg`s.

use std::path::{Path, PathBuf};

use irida_core::apply::{self, ApplyError, ApplyRequest, Ctx, UndoError, target_status};
use irida_core::draft::{clusters, mode_of};
use irida_core::export::{ExportRequest, export};
use irida_core::import::import_rice_with_notes;
use irida_core::palette::Palette;
use irida_core::store::{PairLookup, Settings, Store, StoreEntry, StoreError};
use irida_core::wallpaper::{WallpaperError, current_wallpaper_in, load_pixels};

use super::state::{
    ApplyFailure, Effect, EntrySummary, ExportSummary, Msg, NOT_IMAGE, OpenFailure, UndoDone,
};
use crate::output::format_export;

/// The palette's slug field must name its own folder; otherwise saving it
/// would write another palette's folder.
fn slug_mismatch(folder: &str, p: &Palette) -> Option<String> {
    (p.slug != folder).then(|| {
        format!(
            "its slug is {:?} but its folder is {folder:?}: make them match",
            p.slug
        )
    })
}

fn summary(entry: StoreEntry) -> EntrySummary {
    let palette = match entry.result {
        Ok(p) => match slug_mismatch(&entry.slug, &p) {
            Some(e) => Err(e),
            None => Ok((p.name, p.mode)),
        },
        Err(e) => Err(e.to_string()),
    };
    EntrySummary {
        slug: entry.slug,
        palette,
    }
}

fn summaries(store: &Store) -> Vec<EntrySummary> {
    store.list().into_iter().map(summary).collect()
}

fn open(store: &Store, slug: &str) -> Result<Palette, OpenFailure> {
    let error = match store.load(slug) {
        Ok(p) => match slug_mismatch(slug, &p) {
            None => return Ok(p),
            Some(e) => e,
        },
        Err(StoreError::Unreadable { message, .. }) => message,
        Err(e) => {
            return Err(OpenFailure {
                error: e.to_string(),
                text: None,
            });
        }
    };
    match store.raw_text(slug) {
        Ok(text) => Err(OpenFailure {
            error,
            text: Some(text),
        }),
        Err(e) => Err(OpenFailure {
            error: e.to_string(),
            text: None,
        }),
    }
}

/// Save `p` as a new version; with `fresh`, a slug that exists is refused.
fn save(store: &Store, p: &Palette, fresh: bool) -> Result<u32, String> {
    if fresh && store.exists(&p.slug) {
        return Err(StoreError::Exists(p.slug.clone()).to_string());
    }
    store.save_version(p).map_err(|e| e.to_string())
}

fn apply_failure(e: ApplyError) -> ApplyFailure {
    match e {
        ApplyError::Write {
            path,
            source,
            rolled_back,
        } => ApplyFailure::Write {
            path,
            reason: source.to_string(),
            rolled_back,
        },
        ApplyError::Render { target, message } => ApplyFailure::Render { target, message },
        ApplyError::NothingToApply(_) => ApplyFailure::NothingToApply,
        e @ (ApplyError::UnknownTarget(_) | ApplyError::Record(_)) => {
            ApplyFailure::Other(e.to_string())
        }
    }
}

fn apply_palette(ctx: &Ctx, palette: &Palette, pair: Option<&Palette>, settings: &Settings) -> Msg {
    let req = ApplyRequest {
        palette,
        pair,
        only: None,
        live_only: false,
    };
    Msg::Applied(apply::apply(ctx, settings, &req).map_err(apply_failure))
}

fn undo(ctx: &Ctx) -> Result<UndoDone, String> {
    match apply::undo(ctx) {
        Ok(r) => Ok(UndoDone {
            record_id: r.record_id,
            failures: r
                .failures
                .iter()
                .map(|(path, e)| {
                    format!(
                        "Could not restore {}: {e}; run Undo last apply again.",
                        path.display()
                    )
                })
                .collect(),
        }),
        Err(UndoError::NothingToUndo) => Err("Nothing to undo".to_string()),
        Err(e @ UndoError::LiveSessionOpen) => Err(e.to_string()),
        Err(UndoError::Io(e)) => Err(format!("Could not undo: {e}")),
    }
}

/// Save an imported palette. `replace` adds a version to the palette with
/// its slug, which must be readable, keeping the pair and notes a repo
/// cannot carry.
fn save_import(store: &Store, mut p: Palette, replace: bool) -> Result<Palette, String> {
    if replace {
        let existing = store.load(&p.slug).map_err(|e| {
            format!(
                "{} cannot be read, so it is not replaced ({e}); use Keep both",
                p.slug
            )
        })?;
        p.pair = existing.pair;
        p.notes = existing.notes;
    }
    save(store, &p, !replace).map(|_| p)
}

fn export_to(
    store: &Store,
    ctx: &Ctx,
    p: &Palette,
    folder: &Path,
) -> Result<ExportSummary, String> {
    let (pair, note) = match store.pair_of(p) {
        PairLookup::Found(pair) => (Some(pair), None),
        PairLookup::None => (None, None),
        PairLookup::SameMode(slug) => (
            None,
            Some(format!(
                "Its pair {slug} has the same mode, so {} was exported alone.",
                p.name
            )),
        ),
        PairLookup::Missing { slug, reason } => (
            None,
            Some(format!(
                "Its pair {slug} cannot be loaded ({reason}), so {} was exported alone.",
                p.name
            )),
        ),
    };
    let req = ExportRequest {
        palette: p,
        pair: pair.as_ref(),
        folder,
    };
    let report = export(&ctx.dirs, &req).map_err(|e| e.to_string())?;
    let mut text = format_export(&report, p);
    if let Some(note) = note {
        text.push_str(&note);
        text.push('\n');
    }
    Ok(ExportSummary {
        lines: report.lines,
        text,
    })
}

/// Read the image at `path` and find its colours and mode.
fn draft_image(path: PathBuf) -> Msg {
    match load_pixels(&path) {
        Ok(rgba) => {
            let clusters = clusters(&rgba);
            let mode = mode_of(&clusters);
            Msg::DraftReady {
                path,
                clusters,
                mode,
            }
        }
        Err(e) => {
            let error = match e {
                WallpaperError::NotImage(_) => NOT_IMAGE.to_string(),
                WallpaperError::Io(_, reason) => {
                    let name = path.file_name().unwrap_or(path.as_os_str());
                    format!(
                        "Could not read {}: {reason}. Choose another image.",
                        name.to_string_lossy()
                    )
                }
            };
            Msg::DraftFailed { path, error }
        }
    }
}

pub fn perform(effect: Effect, ctx: &Ctx) -> Vec<Msg> {
    let store = Store::new(ctx.dirs.clone());
    let msg = match effect {
        Effect::SeedStarters => Msg::Seeded(store.seed_starters().map_err(|e| e.to_string())),
        Effect::LoadStore => Msg::StoreLoaded {
            entries: summaries(&store),
            settings: store.settings().map_err(|e| e.to_string()),
        },
        Effect::ListEntries => Msg::Entries(summaries(&store)),
        Effect::OpenPalette(slug) => {
            let result = open(&store, &slug);
            Msg::Opened { slug, result }
        }
        Effect::SaveVersion { palette, fresh } => {
            Msg::Saved(save(&store, &palette, fresh).map(|n| (n, palette)))
        }
        Effect::Create(p) => Msg::Created(save(&store, &p, true).map(|_| p)),
        Effect::Duplicate { from, name } => {
            Msg::Created(store.duplicate(&from, &name).map_err(|e| e.to_string()))
        }
        Effect::Delete(slug) => {
            let result = store.delete(&slug).map_err(|e| e.to_string());
            Msg::Deleted { slug, result }
        }
        Effect::SaveSettings(s) => {
            Msg::SettingsSaved(store.save_settings(&s).map_err(|e| e.to_string()))
        }
        Effect::LoadApplySheet { palette, settings } => Msg::ApplySheetLoaded {
            targets: target_status(ctx, &settings),
            pair: Box::new(store.pair_of(&palette)),
            slug: palette.slug,
        },
        Effect::Apply {
            palette,
            pair,
            settings,
        } => apply_palette(ctx, &palette, pair.as_ref(), &settings),
        Effect::Undo => Msg::Undone(undo(ctx)),
        Effect::Import(path) => {
            Msg::Imported(import_rice_with_notes(&path).map_err(|e| e.to_string()))
        }
        Effect::SaveImport {
            palette,
            notes,
            replace,
        } => Msg::ImportSaved {
            result: save_import(&store, palette, replace),
            notes,
        },
        Effect::Export { palette, folder } => {
            Msg::Exported(export_to(&store, ctx, &palette, &folder))
        }
        Effect::FindWallpaper(system) => Msg::WallpaperFound(current_wallpaper_in(
            &ctx.dirs.config.join("cosmic"),
            &system,
        )),
        Effect::DraftImage(path) => draft_image(path),
        Effect::Toast(_)
        | Effect::Copy(_)
        | Effect::ChooseFolder(_)
        | Effect::ChooseImage
        | Effect::Exit => {
            return Vec::new();
        }
    };
    vec![msg]
}

/// Run `effects` as the app does, then every Msg they give, until no IO
/// effect is left; returns the window effects (toasts, exit) in order.
#[cfg(test)]
pub(crate) fn drive_effects(state: &mut super::state::State, effects: Vec<Effect>) -> Vec<Effect> {
    let mut queue = std::collections::VecDeque::new();
    let mut ui = Vec::new();
    let mut run = |effects: Vec<Effect>,
                   state: &super::state::State,
                   queue: &mut std::collections::VecDeque<Msg>| {
        for e in effects {
            if e.is_io() {
                queue.extend(perform(e, &state.ctx));
            } else {
                ui.push(e);
            }
        }
    };
    run(effects, state, &mut queue);
    while let Some(msg) = queue.pop_front() {
        let effects = super::state::update(state, msg);
        run(effects, state, &mut queue);
    }
    ui
}

/// `update` with `msg`, then `drive_effects`.
#[cfg(test)]
pub(crate) fn drive(state: &mut super::state::State, msg: Msg) -> Vec<Effect> {
    let effects = super::state::update(state, msg);
    drive_effects(state, effects)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use irida_core::dirs::Dirs;
    use irida_core::palette::Mode;
    use irida_core::store::{Settings, starters};
    use tempfile::TempDir;

    use super::*;
    use crate::app::state::FolderFor;

    fn ctx(tmp: &TempDir) -> Ctx {
        Ctx {
            dirs: Dirs::under(tmp.path()),
            path: vec![],
        }
    }

    fn ember() -> Palette {
        starters().into_iter().next().unwrap()
    }

    #[test]
    fn load_store_summarises() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let store = Store::new(ctx.dirs.clone());
        store.save_version(&ember()).unwrap();
        let dir = ctx.dirs.data.join("irida/palettes/broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("palette.toml"), "nope").unwrap();
        let msgs = perform(Effect::LoadStore, &ctx);
        let [Msg::StoreLoaded { entries, settings }] = msgs.as_slice() else {
            panic!("{msgs:?}");
        };
        assert_eq!(settings.as_ref().unwrap(), &Settings::default());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].slug, "broken");
        assert!(entries[0].palette.is_err());
        assert_eq!(entries[1].palette, Ok(("Ember".to_string(), Mode::Dark)));
    }

    #[test]
    fn open_unreadable_gives_text() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let dir = ctx.dirs.data.join("irida/palettes/broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("palette.toml"), "nope").unwrap();
        let msgs = perform(Effect::OpenPalette("broken".to_string()), &ctx);
        let [
            Msg::Opened {
                slug,
                result: Err(f),
            },
        ] = msgs.as_slice()
        else {
            panic!("{msgs:?}");
        };
        assert_eq!(slug, "broken");
        assert_eq!(f.text.as_deref(), Some("nope"));

        let msgs = perform(Effect::OpenPalette("gone".to_string()), &ctx);
        let [Msg::Opened { result: Err(f), .. }] = msgs.as_slice() else {
            panic!("{msgs:?}");
        };
        assert_eq!(f.text, None);
    }

    #[test]
    fn fresh_saves_refuse_existing_slugs() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let store = Store::new(ctx.dirs.clone());
        store.save_version(&ember()).unwrap();
        let msgs = perform(Effect::Create(ember()), &ctx);
        assert!(matches!(msgs.as_slice(), [Msg::Created(Err(e))] if e.contains("already exists")));
        let msgs = perform(
            Effect::SaveVersion {
                palette: ember(),
                fresh: true,
            },
            &ctx,
        );
        assert!(matches!(msgs.as_slice(), [Msg::Saved(Err(_))]));
        assert_eq!(store.versions("ember").unwrap().len(), 1);
        let msgs = perform(
            Effect::SaveVersion {
                palette: ember(),
                fresh: false,
            },
            &ctx,
        );
        assert!(matches!(msgs.as_slice(), [Msg::Saved(Ok((2, _)))]));
    }

    #[test]
    fn undo_apply_and_import_failures_are_plain() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let msgs = perform(Effect::Undo, &ctx);
        assert!(
            matches!(msgs.as_slice(), [Msg::Undone(Err(e))] if e == "Nothing to undo"),
            "{msgs:?}"
        );
        let msgs = perform(
            Effect::Apply {
                palette: ember(),
                pair: None,
                settings: Settings::default(),
            },
            &ctx,
        );
        assert!(
            matches!(
                msgs.as_slice(),
                [Msg::Applied(Err(ApplyFailure::NothingToApply))]
            ),
            "{msgs:?}"
        );
        let msgs = perform(Effect::Import(tmp.path().to_path_buf()), &ctx);
        assert!(
            matches!(msgs.as_slice(), [Msg::Imported(Err(_))]),
            "{msgs:?}"
        );
    }

    #[test]
    fn window_effects_are_not_performed() {
        let tmp = TempDir::new().unwrap();
        assert!(perform(Effect::Exit, &ctx(&tmp)).is_empty());
        assert!(!Effect::Exit.is_io());
        assert!(Effect::LoadStore.is_io());
        assert!(!Effect::ChooseFolder(FolderFor::Import).is_io());
        assert!(perform(Effect::ChooseFolder(FolderFor::Export), &ctx(&tmp)).is_empty());
        assert!(!Effect::ChooseImage.is_io());
        assert!(perform(Effect::ChooseImage, &ctx(&tmp)).is_empty());
    }

    #[test]
    fn draft_image_reads_or_says_why_not() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let png = tmp.path().join("flat.png");
        image::RgbaImage::from_pixel(40, 25, image::Rgba([200, 60, 40, 255]))
            .save(&png)
            .unwrap();
        let msgs = perform(Effect::DraftImage(png.clone()), &ctx);
        let [
            Msg::DraftReady {
                path,
                clusters,
                mode,
            },
        ] = msgs.as_slice()
        else {
            panic!("{msgs:?}");
        };
        assert_eq!(path, &png);
        assert_eq!(clusters.len(), 1);
        assert_eq!(*mode, irida_core::draft::mode_of(clusters));

        let text = tmp.path().join("notes.txt");
        fs::write(&text, "words").unwrap();
        let msgs = perform(Effect::DraftImage(text.clone()), &ctx);
        assert!(
            matches!(msgs.as_slice(), [Msg::DraftFailed { path, error }]
                if *path == text && error == NOT_IMAGE),
            "{msgs:?}"
        );
        let gone = tmp.path().join("gone.jpg");
        let msgs = perform(Effect::DraftImage(gone.clone()), &ctx);
        assert!(
            matches!(msgs.as_slice(), [Msg::DraftFailed { path, error }]
                if *path == gone && error.starts_with("Could not read gone.jpg: ")),
            "{msgs:?}"
        );
    }

    #[test]
    fn find_wallpaper_reads_without_writing() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let system = tmp.path().join("system");
        let msgs = perform(Effect::FindWallpaper(system.clone()), &ctx);
        assert!(
            matches!(msgs.as_slice(), [Msg::WallpaperFound(None)]),
            "{msgs:?}"
        );
        assert!(!ctx.dirs.config.exists(), "nothing created");
        assert!(!system.exists(), "nothing created");

        // The system default is read when the user has none.
        let image = tmp.path().join("w.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]))
            .save(&image)
            .unwrap();
        let keys = system.join("com.system76.CosmicBackground/v1");
        fs::create_dir_all(&keys).unwrap();
        fs::write(
            keys.join("all"),
            format!(
                "(output: \"all\", source: Path({:?}), filter_by_theme: #true)",
                image.to_str().unwrap()
            ),
        )
        .unwrap();
        let msgs = perform(Effect::FindWallpaper(system), &ctx);
        assert!(
            matches!(msgs.as_slice(), [Msg::WallpaperFound(Some(p))] if *p == image),
            "{msgs:?}"
        );
    }
}
