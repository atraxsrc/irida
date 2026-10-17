//! The IO around irida-core that the popup needs: what to show when it opens,
//! an apply, an undo, and reaching the Irida window.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use irida_core::apply::{self, ApplyRequest, Ctx, UndoError};
use irida_core::dirs::Dirs;
use irida_core::live;
use irida_core::native::cosmic_desktop::current_mode;
use irida_core::palette::Mode;
use irida_core::record;
use irida_core::store::{Settings, Store};
use irida_core::target::is_executable;

use crate::copy::{
    CANNOT_REACH, NOTHING_TO_UNDO, after_apply, applied, apply_failed, cannot_start, undo_failed,
    undone,
};
use crate::menu::{Kind, Menu, menu};

/// The window's DBus name and object, as libcosmic's single instance
/// registers them from the app id.
pub const IRIDA_APP_ID: &str = "io.github.atraxsrc.Irida";
const IRIDA_OBJECT: &str = "/io/github/atraxsrc/Irida";

/// Everything the popup shows, read fresh when it opens (spec 4.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub menu: Menu,
    /// The settings an apply uses, or why they cannot be read.
    pub settings: Result<Settings, String>,
    /// A Try it live session is open in the window.
    pub live: bool,
    pub window_open: bool,
    /// COSMIC's current appearance, for the pair rule.
    pub mode: Option<Mode>,
}

pub fn load(ctx: &Ctx) -> Loaded {
    let store = Store::new(ctx.dirs.clone());
    let latest = record::latest(&ctx.dirs).ok().flatten().map(|r| r.palette);
    Loaded {
        menu: menu(store.list(), latest.as_deref()),
        settings: store.settings().map_err(|e| e.to_string()),
        live: live::session_open(&ctx.dirs),
        window_open: window_open(&ctx.dirs),
        mode: current_mode(&ctx.dirs),
    }
}

/// True when an Irida window holds the app lock. The lock is tried and let
/// go at once; an error taking it counts as no window.
pub fn window_open(dirs: &Dirs) -> bool {
    matches!(live::lock_app(dirs), Ok(None))
}

/// Apply one row (spec 4.3). On success `last_palette` is saved and the
/// status text returned; on failure settings are untouched.
pub fn apply_row(
    ctx: &Ctx,
    settings: &Settings,
    kind: &Kind,
    mode: Option<Mode>,
) -> Result<String, String> {
    let label = kind.label();
    let (palette, pair) = kind.pick(mode);
    let req = ApplyRequest {
        palette,
        pair,
        only: None,
        live_only: false,
    };
    apply::apply(ctx, settings, &req).map_err(|e| apply_failed(&label, &e.to_string()))?;
    let store = Store::new(ctx.dirs.clone());
    store
        .save_settings(&after_apply(settings, &palette.slug))
        .map_err(|e| apply_failed(&label, &e.to_string()))?;
    Ok(applied(&label))
}

/// Undo the last apply, as the window's Undo last apply does (spec 4.5).
pub fn undo(ctx: &Ctx) -> String {
    match apply::undo(ctx) {
        Ok(r) => {
            let name = Store::new(ctx.dirs.clone())
                .load(&r.palette)
                .map(|p| p.name)
                .unwrap_or(r.palette);
            let failures: Vec<String> = r
                .failures
                .iter()
                .map(|(path, e)| format!("Could not restore {}: {e}", path.display()))
                .collect();
            undone(&name, &failures)
        }
        Err(UndoError::NothingToUndo) => NOTHING_TO_UNDO.to_string(),
        Err(e) => undo_failed(&e.to_string()),
    }
}

/// The `irida` binary on `ctx.path`, which the panel's short `$PATH` was
/// widened into by `Ctx::for_app`.
fn irida_binary(ctx: &Ctx) -> Option<PathBuf> {
    ctx.path
        .iter()
        .map(|dir| dir.join("irida"))
        .find(|p| is_executable(p))
}

/// Start the window, with the compositor's activation token when one was
/// given so it opens focused. A running window is activated by `irida`'s own
/// single-instance logic.
pub fn open_irida(ctx: &Ctx, token: Option<String>) -> Result<(), String> {
    let bin = irida_binary(ctx).ok_or_else(|| cannot_start("irida is not on the PATH"))?;
    let mut cmd = Command::new(bin);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(token) = token {
        cmd.env("XDG_ACTIVATION_TOKEN", token);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| cannot_start(&e.to_string()))
}

#[zbus::proxy(
    interface = "org.freedesktop.DbusActivation",
    default_service = "io.github.atraxsrc.Irida",
    default_path = "/io/github/atraxsrc/Irida"
)]
trait IridaWindow {
    fn activate_action(
        &self,
        action_name: &str,
        parameter: Vec<&str>,
        platform_data: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;
}

/// Ask the running window to close, as its close button would (spec 5.2).
pub fn close_irida() -> Result<(), String> {
    let reach = || -> zbus::Result<()> {
        let conn = zbus::blocking::Connection::session()?;
        let proxy = IridaWindowProxyBlocking::builder(&conn)
            .destination(IRIDA_APP_ID)?
            .path(IRIDA_OBJECT)?
            .build()?;
        proxy.activate_action("close", Vec::new(), HashMap::new())
    };
    reach().map_err(|_| CANNOT_REACH.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use irida_core::dirs::Dirs;
    use irida_core::store::{Settings, Store, starters};
    use std::collections::BTreeSet;
    use tempfile::TempDir;

    /// A temp home with the four starters saved and an empty `$PATH`.
    fn env() -> (TempDir, Ctx, Store) {
        let tmp = TempDir::new().unwrap();
        let dirs = Dirs::under(tmp.path());
        let store = Store::new(dirs.clone());
        for p in starters() {
            store.save_version(&p).unwrap();
        }
        (tmp, Ctx { dirs, path: vec![] }, store)
    }

    fn ember(store: &Store) -> Kind {
        Kind::Single(Box::new(store.load("ember").unwrap()))
    }

    /// The COSMIC desktop target is detected by its config folder alone.
    fn detect_cosmic(ctx: &Ctx) {
        std::fs::create_dir_all(ctx.dirs.config.join("cosmic")).unwrap();
    }

    #[test]
    fn window_open_follows_the_lock() {
        let (_tmp, ctx, _) = env();
        assert!(!window_open(&ctx.dirs));
        let lock = irida_core::live::lock_app(&ctx.dirs).unwrap();
        assert!(lock.is_some());
        assert!(window_open(&ctx.dirs));
        drop(lock);
        assert!(!window_open(&ctx.dirs));
    }

    #[test]
    fn load_reads_menu_settings_and_live() {
        let (_tmp, ctx, _) = env();
        let l = load(&ctx);
        assert_eq!(l.menu.pairs.len(), 2);
        assert!(l.menu.dark.is_empty() && l.menu.light.is_empty());
        assert_eq!(l.settings, Ok(Settings::default()));
        assert!(!l.live);
        assert_eq!(l.mode, None);
        assert!(!l.window_open);
    }

    #[test]
    fn load_reports_unreadable_settings() {
        let (_tmp, ctx, store) = env();
        store.save_settings(&Settings::default()).unwrap();
        std::fs::write(
            ctx.dirs.config.join("irida/settings.toml"),
            "not = toml = at all",
        )
        .unwrap();
        let l = load(&ctx);
        assert!(l.settings.unwrap_err().contains("settings.toml"));
    }

    #[test]
    fn apply_row_failure_leaves_settings_alone() {
        let (_tmp, ctx, store) = env();
        let settings = Settings {
            disabled: BTreeSet::from(["btop".to_string()]),
            last_palette: None,
        };
        store.save_settings(&settings).unwrap();
        let r = apply_row(&ctx, &settings, &ember(&store), None);
        assert!(
            r.clone().unwrap_err().starts_with("Could not apply Ember:"),
            "{r:?}"
        );
        assert_eq!(store.settings().unwrap(), settings);
    }

    #[test]
    fn apply_row_success_saves_last_palette() {
        let (_tmp, ctx, store) = env();
        detect_cosmic(&ctx);
        let settings = Settings {
            disabled: BTreeSet::from(["btop".to_string()]),
            last_palette: None,
        };
        store.save_settings(&settings).unwrap();
        assert_eq!(
            apply_row(&ctx, &settings, &ember(&store), None),
            Ok("Applied Ember".to_string())
        );
        let after = store.settings().unwrap();
        assert_eq!(after.last_palette.as_deref(), Some("ember"));
        assert_eq!(after.disabled, settings.disabled);
        let latest = irida_core::record::latest(&ctx.dirs).unwrap().unwrap();
        assert_eq!(latest.palette, "ember");
    }

    #[test]
    fn undo_texts() {
        let (_tmp, ctx, store) = env();
        assert_eq!(undo(&ctx), NOTHING_TO_UNDO);
        detect_cosmic(&ctx);
        apply_row(&ctx, &Settings::default(), &ember(&store), None).unwrap();
        assert_eq!(undo(&ctx), "Undid Ember");
        assert_eq!(undo(&ctx), NOTHING_TO_UNDO);
    }
}
