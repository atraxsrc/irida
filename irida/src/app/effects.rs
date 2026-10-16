//! The effect runner: the app's IO, synchronous and free of libcosmic
//! types. Every IO effect returns its result as `Msg`s.

use irida_core::apply::Ctx;
use irida_core::palette::Palette;
use irida_core::store::{Store, StoreEntry, StoreError};

use super::state::{Effect, EntrySummary, Msg, OpenFailure};

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
        Effect::Toast(_) | Effect::Exit => return Vec::new(),
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
    fn window_effects_are_not_performed() {
        let tmp = TempDir::new().unwrap();
        assert!(perform(Effect::Exit, &ctx(&tmp)).is_empty());
        assert!(!Effect::Exit.is_io());
        assert!(Effect::LoadStore.is_io());
    }
}
