//! Try it live: a session of applies to the targets that reload live, held in
//! one apply record from the start, then kept or reverted. The marker
//! `$XDG_STATE_HOME/irida/live.toml` names the open session, so a session cut
//! short by a crash is found on the next start.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::apply::{
    ApplyError, ApplyReport, ApplyRequest, Ctx, KEEP_RECORDS, UndoError, UndoReport, apply_into,
    reload_target,
};
use crate::dirs::Dirs;
use crate::record::{self, RecordWriter, RestoreMode};
use crate::store::Settings;
use crate::target::{is_detected, load_targets};
use crate::write::atomic_write;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    /// The palette slug the session started with.
    palette: String,
    /// The session's record, once its first apply has created it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    record: Option<u32>,
}

fn marker_path(dirs: &Dirs) -> PathBuf {
    dirs.state.join("irida/live.toml")
}

/// The marker; `None` when there is none, an error when it cannot be read or parsed.
fn read_marker(dirs: &Dirs) -> io::Result<Option<Marker>> {
    let text = match fs::read_to_string(marker_path(dirs)) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    toml::from_str(&text)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
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
/// names no record, names one that is missing, or cannot be read is deleted
/// and gives `None`: nothing of that session is on disk.
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
        Some(id) if record::exists(dirs, id) => Some(LiveSession {
            palette: marker.palette,
            record: Some(id),
        }),
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
