//! The effect runner: the app's IO, synchronous and free of libcosmic
//! types. Every IO effect returns its result as `Msg`s.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use irida_core::apply::{
    self, ApplyError, ApplyRequest, Ctx, UndoError, UndoReport, target_status,
};
use irida_core::draft::{clusters, mode_of};
use irida_core::export::{ExportRequest, export};
use irida_core::import::import_rice_with_notes;
use irida_core::live::{self, LiveSession};
use irida_core::palette::Palette;
use irida_core::store::{PairLookup, Settings, Store, StoreEntry, StoreError};
use irida_core::wallpaper::{WallpaperError, current_wallpaper_in, load_pixels};

use super::state::{
    ApplyFailure, Effect, EntrySummary, ExportSummary, Msg, NOT_IMAGE, OpenFailure, UndoDone,
    any_live_target,
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
    let (palette, pair) = match entry.result {
        Ok(p) => match slug_mismatch(&entry.slug, &p) {
            Some(e) => (Err(e), None),
            None => (Ok((p.name, p.mode)), p.pair),
        },
        Err(e) => (Err(e.to_string()), None),
    };
    EntrySummary {
        slug: entry.slug,
        palette,
        pair,
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
        e
        @ (ApplyError::UnknownTarget(_) | ApplyError::Record(_) | ApplyError::LiveSessionOpen) => {
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

/// What an undo or a revert did, one line per file it could not put back.
fn undo_done(r: &UndoReport, again: &str) -> UndoDone {
    UndoDone {
        record_id: r.record_id,
        failures: r
            .failures
            .iter()
            .map(|(path, e)| format!("Could not restore {}: {e}; {again}", path.display()))
            .collect(),
    }
}

fn undo(ctx: &Ctx) -> Result<UndoDone, String> {
    match apply::undo(ctx) {
        Ok(r) => Ok(undo_done(&r, "run Undo last apply again.")),
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

/// Apply `palette` in the live session, with its pair as Apply does; the
/// session comes back with its record.
fn live_apply(
    store: &Store,
    ctx: &Ctx,
    mut session: LiveSession,
    palette: &Palette,
    settings: &Settings,
) -> Msg {
    let pair = match store.pair_of(palette) {
        PairLookup::Found(p) => Some(p),
        _ => None,
    };
    let req = ApplyRequest {
        palette,
        pair: pair.as_ref(),
        only: None,
        live_only: true,
    };
    let result = session.apply(ctx, settings, &req).map_err(apply_failure);
    Msg::LiveApplied { session, result }
}

fn live_revert(ctx: &Ctx, session: LiveSession) -> Result<Option<UndoDone>, String> {
    session
        .revert(ctx)
        .map(|r| r.map(|r| undo_done(&r, "Revert is offered again.")))
        .map_err(|e| e.to_string())
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
        Effect::SaveVersion { mut palette, fresh } => {
            // The pair is the store's: a link or unlink queued before this
            // save has already written it.
            if !fresh && let Ok(stored) = store.load(&palette.slug) {
                palette.pair = stored.pair;
            }
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
        Effect::LoadVersions(slug) => Msg::Versions {
            result: store.versions(&slug).map_err(|e| e.to_string()),
            slug,
        },
        Effect::LoadVersion { slug, n } => Msg::VersionLoaded {
            result: store.load_version(&slug, n).map_err(|e| e.to_string()),
            slug,
            n,
        },
        Effect::LoadPair(palette) => Msg::PairLoaded {
            lookup: Box::new(store.pair_of(&palette)),
            slug: palette.slug,
        },
        Effect::LinkPair { slug, with } => Msg::Linked {
            result: store.link_pair(&slug, &with).map_err(|e| e.to_string()),
            slug,
            with,
        },
        Effect::UnlinkPair(slug) => Msg::Unlinked {
            result: store.unlink_pair(&slug).map_err(|e| e.to_string()),
            slug,
        },
        Effect::PinSnapshot { slug, n, name } => Msg::SnapshotSaved {
            result: store
                .pin_snapshot(&slug, n, &name)
                .map_err(|e| e.to_string()),
            slug,
            n,
            name: Some(name),
        },
        Effect::UnpinSnapshot { slug, n } => Msg::SnapshotSaved {
            result: store.unpin_snapshot(&slug, n).map_err(|e| e.to_string()),
            slug,
            n,
            name: None,
        },
        Effect::StartLive(slug) => {
            Msg::LiveStarted(LiveSession::start(ctx, &slug).map_err(|e| e.to_string()))
        }
        Effect::LiveApply {
            session,
            palette,
            settings,
        } => live_apply(&store, ctx, session, &palette, &settings),
        Effect::KeepLive(session) => Msg::LiveKept(session.keep(ctx).map_err(|e| e.to_string())),
        Effect::RevertLive(session) => Msg::LiveReverted(live_revert(ctx, session)),
        Effect::FindPendingLive => Msg::LivePending(live::pending(&ctx.dirs)),
        Effect::LoadLiveTargets(settings) => {
            Msg::LiveTargets(any_live_target(&target_status(ctx, &settings)))
        }
        Effect::Toast(_)
        | Effect::Copy(_)
        | Effect::ChooseFolder(_)
        | Effect::ChooseImage
        | Effect::StartTimer(..)
        | Effect::Exit => {
            return Vec::new();
        }
    };
    vec![msg]
}

/// `perform`, with a panic turned into the effect's own failure Msg, so
/// what waits for that Msg (a save, a live step) never waits for ever.
pub fn perform_caught(effect: Effect, ctx: &Ctx) -> Vec<Msg> {
    caught(effect, ctx, perform)
}

fn caught(effect: Effect, ctx: &Ctx, run: impl FnOnce(Effect, &Ctx) -> Vec<Msg>) -> Vec<Msg> {
    let copy = effect.clone();
    match std::panic::catch_unwind(AssertUnwindSafe(|| run(effect, ctx))) {
        Ok(msgs) => msgs,
        Err(payload) => {
            let why = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown error".to_string());
            vec![panicked(copy, ctx, format!("Irida hit a bug ({why})"))]
        }
    }
}

/// The Msg an effect gives when it fails with `error`; a read with no
/// failure of its own shows `error` in a toast.
pub(crate) fn panicked(effect: Effect, ctx: &Ctx, error: String) -> Msg {
    let failure = || ApplyFailure::Other(error.clone());
    match effect {
        Effect::SeedStarters => Msg::Seeded(Err(error)),
        Effect::OpenPalette(slug) => Msg::Opened {
            slug,
            result: Err(OpenFailure { error, text: None }),
        },
        Effect::SaveVersion { .. } => Msg::Saved(Err(error)),
        Effect::Create(_) | Effect::Duplicate { .. } => Msg::Created(Err(error)),
        Effect::Delete(slug) => Msg::Deleted {
            slug,
            result: Err(error),
        },
        Effect::SaveSettings(_) => Msg::SettingsSaved(Err(error)),
        Effect::Apply { .. } => Msg::Applied(Err(failure())),
        Effect::Undo => Msg::Undone(Err(error)),
        Effect::Import(_) => Msg::Imported(Err(error)),
        Effect::SaveImport { notes, .. } => Msg::ImportSaved {
            result: Err(error),
            notes,
        },
        Effect::Export { .. } => Msg::Exported(Err(error)),
        Effect::DraftImage(path) => Msg::DraftFailed { path, error },
        Effect::LoadVersions(slug) => Msg::Versions {
            slug,
            result: Err(error),
        },
        Effect::LoadVersion { slug, n } => Msg::VersionLoaded {
            slug,
            n,
            result: Err(error),
        },
        Effect::LinkPair { slug, with } => Msg::Linked {
            slug,
            with,
            result: Err(error),
        },
        Effect::UnlinkPair(slug) => Msg::Unlinked {
            slug,
            result: Err(error),
        },
        Effect::PinSnapshot { slug, n, name } => Msg::SnapshotSaved {
            slug,
            n,
            name: Some(name),
            result: Err(error),
        },
        Effect::UnpinSnapshot { slug, n } => Msg::SnapshotSaved {
            slug,
            n,
            name: None,
            result: Err(error),
        },
        Effect::StartLive(_) => Msg::LiveStarted(Err(error)),
        // The apply may have stopped after its record was made, so the
        // session goes on with the record its marker names.
        Effect::LiveApply { session, .. } => Msg::LiveApplied {
            session: session.refreshed(&ctx.dirs),
            result: Err(failure()),
        },
        Effect::KeepLive(_) => Msg::LiveKept(Err(error)),
        Effect::RevertLive(_) => Msg::LiveReverted(Err(error)),
        Effect::LoadStore
        | Effect::ListEntries
        | Effect::LoadApplySheet { .. }
        | Effect::FindWallpaper(_)
        | Effect::LoadPair(_)
        | Effect::FindPendingLive
        | Effect::LoadLiveTargets(_)
        | Effect::Toast(_)
        | Effect::Copy(_)
        | Effect::ChooseFolder(_)
        | Effect::ChooseImage
        | Effect::StartTimer(..)
        | Effect::Exit => Msg::Failed(error),
    }
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
        let timer = Effect::StartTimer(crate::app::state::LIVE_DELAY, 1);
        assert!(!timer.is_io());
        assert!(perform(timer, &ctx(&tmp)).is_empty());
    }

    #[test]
    fn live_session_round_trip() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let config = ctx.dirs.config.join("cava/config");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "x\n").unwrap();
        assert!(matches!(
            perform(Effect::LoadLiveTargets(Settings::default()), &ctx).as_slice(),
            [Msg::LiveTargets(true)]
        ));
        assert!(matches!(
            perform(Effect::FindPendingLive, &ctx).as_slice(),
            [Msg::LivePending(None)]
        ));
        let msgs = perform(Effect::StartLive("ember".to_string()), &ctx);
        let [Msg::LiveStarted(Ok(session))] = msgs.as_slice() else {
            panic!("{msgs:?}");
        };
        assert_eq!(session.record_id(), None);
        let msgs = perform(
            Effect::LiveApply {
                session: session.clone(),
                palette: ember(),
                settings: Settings::default(),
            },
            &ctx,
        );
        let [
            Msg::LiveApplied {
                session,
                result: Ok(report),
            },
        ] = msgs.as_slice()
        else {
            panic!("{msgs:?}");
        };
        assert!(session.record_id().is_some(), "the record comes back");
        let applied: Vec<&str> = report
            .targets
            .iter()
            .filter(|t| matches!(t.status, apply::TargetStatus::Applied(_)))
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(applied, ["cava"]);
        assert_ne!(fs::read(&config).unwrap(), b"x\n");
        // A second start is refused while this one is open.
        assert!(matches!(
            perform(Effect::StartLive("ember".to_string()), &ctx).as_slice(),
            [Msg::LiveStarted(Err(_))]
        ));
        let msgs = perform(Effect::FindPendingLive, &ctx);
        assert!(
            matches!(msgs.as_slice(), [Msg::LivePending(Some(s))] if s == session),
            "{msgs:?}"
        );
        let msgs = perform(Effect::RevertLive(session.clone()), &ctx);
        assert!(
            matches!(msgs.as_slice(), [Msg::LiveReverted(Ok(Some(done)))] if done.failures.is_empty()),
            "{msgs:?}"
        );
        assert_eq!(fs::read(&config).unwrap(), b"x\n");

        // A session that never applied keeps nothing.
        let msgs = perform(Effect::StartLive("ember".to_string()), &ctx);
        let [Msg::LiveStarted(Ok(session))] = msgs.as_slice() else {
            panic!("{msgs:?}");
        };
        assert!(matches!(
            perform(Effect::KeepLive(session.clone()), &ctx).as_slice(),
            [Msg::LiveKept(Ok(()))]
        ));
        assert!(!ctx.dirs.state.join("irida/live.toml").exists());
        // No live target: none.
        fs::remove_dir_all(config.parent().unwrap()).unwrap();
        assert!(matches!(
            perform(Effect::LoadLiveTargets(Settings::default()), &ctx).as_slice(),
            [Msg::LiveTargets(false)]
        ));
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
    #[test]
    fn a_panic_gives_the_effects_own_failure() {
        let tmp = TempDir::new().unwrap();
        let ctx = ctx(&tmp);
        let msgs = caught(Effect::Undo, &ctx, |_, _| panic!("boom"));
        assert!(
            matches!(msgs.as_slice(), [Msg::Undone(Err(e))] if e.contains("boom")),
            "{msgs:?}"
        );
        let msgs = caught(Effect::ListEntries, &ctx, |_, _| panic!("{}", 7));
        assert!(
            matches!(msgs.as_slice(), [Msg::Failed(e)] if e.contains('7')),
            "{msgs:?}"
        );
        let msgs = caught(Effect::Undo, &ctx, perform);
        assert!(matches!(msgs.as_slice(), [Msg::Undone(Err(_))]), "{msgs:?}");
    }
}
