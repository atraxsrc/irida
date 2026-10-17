//! Try it live: a session of applies to the targets that reload live, held in
//! one apply record from the start, then kept or reverted. The marker
//! `$XDG_STATE_HOME/irida/live.toml` names the open session, so a session cut
//! short by a crash is found on the next start. The app holds a lock on
//! `$XDG_STATE_HOME/irida/app.lock` while it runs, so a second window knows
//! the marker may be the first window's open session.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::PathBuf;

use serde::Serialize;

use crate::apply::{
    ApplyError, ApplyReport, ApplyRequest, Ctx, KEEP_RECORDS, UndoError, UndoReport, apply_into,
    reload_target,
};
use crate::dirs::Dirs;
use crate::record::{self, RecordWriter, RestoreMode};
use crate::store::Settings;
use crate::target::{is_detected, load_targets};
use crate::write::atomic_write;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Marker {
    /// The palette slug the session started with; empty when the marker
    /// does not say.
    palette: String,
    /// The session's record, once its first apply has created it.
    #[serde(skip_serializing_if = "Option::is_none")]
    record: Option<u32>,
}

/// Take the app's lock, held while the returned file is open (the kernel
/// lets it go on exit or crash). `None` when another Irida window holds it.
pub fn lock_app(dirs: &Dirs) -> io::Result<Option<File>> {
    let path = dirs.state.join("irida/app.lock");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

fn marker_path(dirs: &Dirs) -> PathBuf {
    dirs.state.join("irida/live.toml")
}

/// The marker; `None` when there is none, an error when it cannot be read or
/// is not TOML. Read leniently, since losing the record it names loses the
/// Revert: other keys are ignored, and a record that is not a number names
/// none.
fn read_marker(dirs: &Dirs) -> io::Result<Option<Marker>> {
    let text = match fs::read_to_string(marker_path(dirs)) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let table: toml::Table =
        toml::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let record = table
        .get("record")
        .and_then(toml::Value::as_integer)
        .and_then(|n| u32::try_from(n).ok());
    let palette = table
        .get("palette")
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(Some(Marker { palette, record }))
}

fn write_marker(dirs: &Dirs, marker: &Marker) -> io::Result<()> {
    let path = marker_path(dirs);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(marker).map_err(io::Error::other)?;
    atomic_write(&path, text.as_bytes())
}

fn remove_marker(dirs: &Dirs) -> io::Result<()> {
    match fs::remove_file(marker_path(dirs)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// The record the marker names, if any. Reads only; a marker that cannot be
/// read names none.
pub(crate) fn marker_record(dirs: &Dirs) -> Option<u32> {
    read_marker(dirs).ok().flatten()?.record
}

/// Whether a live session holds files in its record, so a normal apply or
/// undo must wait until it is kept or reverted. Reads only.
pub fn session_open(dirs: &Dirs) -> bool {
    marker_record(dirs).is_some_and(|id| record::exists(dirs, id))
}

/// An open try-it-live session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSession {
    palette: String,
    record: Option<u32>,
}

impl LiveSession {
    /// Open a session: writes only the marker. The record is created by the
    /// first apply that succeeds. Refused with `AlreadyExists` while another
    /// session is open (`pending` gives it).
    pub fn start(ctx: &Ctx, palette_slug: &str) -> io::Result<LiveSession> {
        if pending(&ctx.dirs).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a live session is already open; keep or revert it first",
            ));
        }
        let session = LiveSession {
            palette: palette_slug.to_string(),
            record: None,
        };
        session.save_marker(&ctx.dirs, None)?;
        Ok(session)
    }

    /// The palette slug the session started with.
    pub fn palette(&self) -> &str {
        &self.palette
    }

    /// The session's record, once an apply has succeeded.
    pub fn record_id(&self) -> Option<u32> {
        self.record
    }

    /// This session as its marker names it now: an apply that stopped part
    /// way (a panic) may have made the record this copy does not know. A
    /// record stopped before its first write is not taken, so the next apply
    /// starts it again.
    pub fn refreshed(mut self, dirs: &Dirs) -> LiveSession {
        if self.record.is_none() {
            self.record = marker_record(dirs).filter(|&id| record::exists(dirs, id));
        }
        self
    }

    fn save_marker(&self, dirs: &Dirs, record: Option<u32>) -> io::Result<()> {
        write_marker(
            dirs,
            &Marker {
                palette: self.palette.clone(),
                record,
            },
        )
    }

    /// Apply to the targets that reload live only, adding to the session's record.
    pub fn apply(
        &mut self,
        ctx: &Ctx,
        settings: &Settings,
        req: &ApplyRequest,
    ) -> Result<ApplyReport, ApplyError> {
        let req = ApplyRequest {
            live_only: true,
            ..req.clone()
        };
        match self.record {
            Some(id) => {
                let mut rec = RecordWriter::reopen(&ctx.dirs, id).map_err(ApplyError::Record)?;
                apply_into(ctx, settings, &req, &mut rec, true)
            }
            None => self.first_apply(ctx, settings, &req),
        }
    }

    /// The first apply creates the record. The marker names it before any
    /// file is written, so a crash during this apply still offers Revert; a
    /// failed apply deletes the record and takes its name off the marker,
    /// unless rolling back failed, when the record is kept for Revert.
    fn first_apply(
        &mut self,
        ctx: &Ctx,
        settings: &Settings,
        req: &ApplyRequest,
    ) -> Result<ApplyReport, ApplyError> {
        drop_unfinished(&ctx.dirs);
        let mut rec = RecordWriter::create(&ctx.dirs, &self.palette).map_err(ApplyError::Record)?;
        let id = rec.id();
        if let Err(e) = self.save_marker(&ctx.dirs, Some(id)) {
            let _ = rec.abort();
            return Err(ApplyError::Record(e));
        }
        match apply_into(ctx, settings, req, &mut rec, true) {
            Ok(report) => {
                self.record = Some(id);
                Ok(report)
            }
            Err(
                e @ ApplyError::Write {
                    rolled_back: false, ..
                },
            ) => {
                self.record = Some(id);
                Err(e)
            }
            Err(e) => {
                let _ = rec.abort();
                let _ = self.save_marker(&ctx.dirs, None);
                Err(e)
            }
        }
    }

    /// End the session keeping its changes: the record becomes a normal apply
    /// record that `irida undo` undoes in one step.
    pub fn keep(self, ctx: &Ctx) -> io::Result<()> {
        if self.record.is_none() {
            drop_unfinished(&ctx.dirs);
        }
        remove_marker(&ctx.dirs)?;
        if self.record.is_some() {
            // As after an apply, a failed prune is not the caller's problem.
            let _ = record::prune(&ctx.dirs, KEEP_RECORDS);
        }
        Ok(())
    }

    /// End the session putting back every file as it was before the session,
    /// byte for byte, then reload every detected live target. `None` when the
    /// session wrote nothing. When a file cannot be put back, the record and
    /// the marker stay, so Revert is offered again.
    pub fn revert(self, ctx: &Ctx) -> Result<Option<UndoReport>, UndoError> {
        let Some(id) = self.record else {
            drop_unfinished(&ctx.dirs);
            remove_marker(&ctx.dirs).map_err(UndoError::Io)?;
            return Ok(None);
        };
        let rec = match record::load(&ctx.dirs, id) {
            Ok(rec) => rec,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // No record.toml: it is saved before the first write, so
                // nothing was written.
                let _ = record::delete(&ctx.dirs, id);
                remove_marker(&ctx.dirs).map_err(UndoError::Io)?;
                return Ok(None);
            }
            Err(e) => return Err(UndoError::Io(e)),
        };
        let failures = record::restore(&ctx.dirs, &rec, RestoreMode::Rollback);
        if failures.is_empty() {
            record::delete(&ctx.dirs, id).map_err(UndoError::Io)?;
            remove_marker(&ctx.dirs).map_err(UndoError::Io)?;
        }
        reload_live(ctx);
        Ok(Some(UndoReport {
            record_id: id,
            palette: rec.palette,
            failures,
        }))
    }
}

/// Delete the record the marker names when it has no record.toml: a first
/// apply that stopped before its first write (a panic), so nothing of it is
/// on disk to put back, and it must not count as a closed record.
fn drop_unfinished(dirs: &Dirs) {
    if let Some(id) = marker_record(dirs).filter(|&id| !record::exists(dirs, id)) {
        let _ = record::delete(dirs, id);
    }
}

/// Reload every target that reloads live and is detected.
fn reload_live(ctx: &Ctx) {
    for lt in load_targets(&ctx.dirs) {
        if let Ok(target) = &lt.result
            && target.def.live
            && is_detected(&target.def, &ctx.dirs, &ctx.path)
        {
            reload_target(target, ctx);
        }
    }
}

/// The session a previous run left open, to offer Revert. A marker that
/// names no record, names one that is missing, or is not TOML is deleted
/// and gives `None`: nothing of that session can be found on disk.
pub fn pending(dirs: &Dirs) -> Option<LiveSession> {
    let marker = match read_marker(dirs) {
        Ok(Some(marker)) => marker,
        Ok(None) => return None,
        Err(_) => {
            let _ = remove_marker(dirs);
            return None;
        }
    };
    match marker.record {
        Some(id) if record::exists(dirs, id) => {
            let palette = match marker.palette {
                p if p.is_empty() => record::load(dirs, id)
                    .map(|r| r.palette)
                    .unwrap_or_default(),
                p => p,
            };
            Some(LiveSession {
                palette,
                record: Some(id),
            })
        }
        Some(id) => {
            // A folder without record.toml: the first apply stopped before
            // its first write.
            let _ = record::delete(dirs, id);
            let _ = remove_marker(dirs);
            None
        }
        None => {
            let _ = remove_marker(dirs);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn app_lock_is_held_by_one_window() {
        let tmp = TempDir::new().unwrap();
        let dirs = Dirs::under(tmp.path());
        let first = lock_app(&dirs).unwrap();
        assert!(first.is_some());
        assert!(lock_app(&dirs).unwrap().is_none(), "a second window");
        drop(first);
        assert!(
            lock_app(&dirs).unwrap().is_some(),
            "free once the first exits"
        );
    }
}
