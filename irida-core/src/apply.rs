//! The apply transaction: select targets, plan every write, record the
//! before-state, write, roll back on failure, reload; and undo the last apply.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::dirs::{Dirs, DirsError};
use crate::native;
use crate::palette::Palette;
use crate::record::{self, Entry, Record, RecordWriter, RestoreMode};
use crate::reload::{ReloadStatus, reload};
use crate::store::Settings;
use crate::target::{FileEntry, LoadedTarget, Target, WriteKind, is_detected, load_targets};
use crate::template::{Renderer, check_syntax};
use crate::write::{atomic_write, set_block, set_key};

/// Apply records kept after an apply.
const KEEP_RECORDS: usize = 10;

/// Where an apply reads and writes, and the `$PATH` used to detect and reload apps.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub dirs: Dirs,
    pub path: Vec<PathBuf>,
}

impl Ctx {
    pub fn from_env() -> Result<Ctx, DirsError> {
        let dirs = Dirs::from_env()?;
        let path = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        Ok(Ctx { dirs, path })
    }
}

#[derive(Debug, Clone)]
pub struct ApplyRequest<'a> {
    pub palette: &'a Palette,
    /// The other half of a pair, for targets that hold both modes.
    pub pair: Option<&'a Palette>,
    /// Apply only these targets, even when turned off in settings.
    pub only: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    NotInstalled,
    Disabled,
    Broken(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TargetStatus {
    Applied(ReloadStatus),
    Skipped(SkipReason),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetResult {
    pub name: String,
    pub status: TargetStatus,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplyReport {
    pub record_id: u32,
    pub targets: Vec<TargetResult>,
}

impl ApplyReport {
    pub fn get(&self, name: &str) -> Option<&TargetResult> {
        self.targets.iter().find(|t| t.name == name)
    }
}

#[derive(Debug)]
pub enum ApplyError {
    UnknownTarget(String),
    Render { target: String, message: String },
    Write { path: PathBuf, source: io::Error, rolled_back: bool },
    NothingToApply(Vec<TargetResult>),
    Record(io::Error),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::UnknownTarget(name) => write!(f, "unknown target: {name}"),
            ApplyError::Render { target, message } => {
                write!(f, "{target}: {message}; nothing was written")
            }
            ApplyError::Write { path, source, rolled_back: true } => write!(
                f,
                "cannot write {}: {source}; every change was rolled back",
                path.display()
            ),
            ApplyError::Write { path, source, rolled_back: false } => write!(
                f,
                "cannot write {}: {source}; rolling back failed, so the apply record was kept \
                 and `irida undo` can retry",
                path.display()
            ),
            ApplyError::NothingToApply(_) => {
                write!(f, "nothing to apply: no installed, enabled target has a file to write")
            }
            ApplyError::Record(e) => {
                write!(f, "cannot save the apply record: {e}; nothing was written")
            }
        }
    }
}

impl std::error::Error for ApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApplyError::Write { source, .. } | ApplyError::Record(source) => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct UndoReport {
    pub record_id: u32,
    /// The palette slug the undone apply wrote.
    pub palette: String,
    /// Files that could not be restored; the record is kept when there are any.
    pub failures: Vec<(PathBuf, io::Error)>,
}

#[derive(Debug)]
pub enum UndoError {
    NothingToUndo,
    Io(io::Error),
}

impl fmt::Display for UndoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UndoError::NothingToUndo => write!(f, "nothing to undo"),
            UndoError::Io(e) => write!(f, "apply record: {e}"),
        }
    }
}

impl std::error::Error for UndoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UndoError::Io(e) => Some(e),
            UndoError::NothingToUndo => None,
        }
    }
}

/// How a planned write changes its file, with what undo needs to reverse it.
enum Edit {
    File,
    Block { comment: String, before: Option<String>, appended_newline: bool },
    Key { key: String, before: Option<String>, appended_newline: bool },
}

/// The full new contents of one file.
struct PlannedWrite {
    path: PathBuf,
    contents: String,
    edit: Edit,
}

impl PlannedWrite {
    fn entry(&self, blob: Option<String>) -> Entry {
        let path = self.path.clone();
        match &self.edit {
            Edit::File => Entry::File { path, blob },
            Edit::Block { comment, before, appended_newline } => Entry::Block {
                path,
                blob,
                comment: comment.clone(),
                before: before.clone(),
                appended_newline: *appended_newline,
            },
            Edit::Key { key, before, appended_newline } => Entry::Key {
                path,
                blob,
                key: key.clone(),
                before: before.clone(),
                appended_newline: *appended_newline,
            },
        }
    }
}

/// One selected target: applied with its writes, or skipped with a reason.
struct TargetPlan<'t> {
    name: String,
    outcome: Result<&'t Target, SkipReason>,
    writes: Vec<PlannedWrite>,
    warnings: Vec<String>,
}

impl TargetPlan<'_> {
    fn skipped(name: &str, reason: SkipReason, warnings: Vec<String>) -> Self {
        TargetPlan { name: name.to_string(), outcome: Err(reason), writes: Vec::new(), warnings }
    }

    fn into_result(self, status: TargetStatus) -> TargetResult {
        TargetResult { name: self.name, status, warnings: self.warnings }
    }
}

/// Whether a target takes part, in this order: turned off (unless named in
/// `only`), failed to load, not installed, a template that does not compile.
fn select<'t>(
    lt: &'t LoadedTarget,
    ctx: &Ctx,
    settings: &Settings,
    named: bool,
) -> Result<&'t Target, SkipReason> {
    if settings.disabled.contains(&lt.name) && !named {
        return Err(SkipReason::Disabled);
    }
    let target = lt.result.as_ref().map_err(|e| SkipReason::Broken(e.to_string()))?;
    if !is_detected(&target.def, &ctx.dirs, &ctx.path) {
        return Err(SkipReason::NotInstalled);
    }
    for f in &target.def.files {
        if let Some(source) = target.templates.get(&f.template) {
            check_syntax(&f.template, source).map_err(|e| SkipReason::Broken(e.to_string()))?;
        }
    }
    Ok(target)
}

/// The current text of `path`; `None` when it does not exist.
fn read_existing(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Plan one `[[file]]`. `Err` is a warning: the file is left alone.
fn plan_file(f: &FileEntry, path: PathBuf, rendered: String) -> Result<PlannedWrite, String> {
    if f.write == WriteKind::File {
        return Ok(PlannedWrite { path, contents: rendered, edit: Edit::File });
    }
    let skip = |why: &dyn fmt::Display| format!("{}: {why}; skipped", path.display());
    let existing = read_existing(&path).map_err(|e| skip(&e))?;
    match f.write {
        WriteKind::Block => {
            let comment = f.comment.clone().ok_or_else(|| skip(&"block needs comment"))?;
            let edit = set_block(existing.as_deref(), &comment, &rendered)
                .map_err(|_| skip(&"irida markers are broken"))?;
            Ok(PlannedWrite {
                path,
                contents: edit.contents,
                edit: Edit::Block {
                    comment,
                    before: edit.before,
                    appended_newline: edit.appended_newline,
                },
            })
        }
        _ => {
            let key = f.key.clone().ok_or_else(|| skip(&"key needs key"))?;
            let value = rendered.trim_end_matches(['\n', '\r']);
            let edit = set_key(existing.as_deref(), &key, value);
            Ok(PlannedWrite {
                path,
                contents: edit.contents,
                edit: Edit::Key { key, before: edit.before, appended_newline: edit.appended_newline },
            })
        }
    }
}

/// Plan every write of one target. A render error stops the whole apply; a
/// native planning error, or no write left, skips only this target.
fn plan_target<'t>(
    target: &'t Target,
    renderer: &Renderer,
    req: &ApplyRequest,
    dirs: &Dirs,
) -> Result<TargetPlan<'t>, ApplyError> {
    let name = &target.def.name;
    let mut writes = Vec::new();
    let mut warnings = Vec::new();
    for f in &target.def.files {
        let render_error = |message: String| ApplyError::Render { target: name.clone(), message };
        let source = target
            .templates
            .get(&f.template)
            .ok_or_else(|| render_error(format!("missing template {}", f.template)))?;
        let rendered =
            renderer.render(&f.template, source).map_err(|e| render_error(e.to_string()))?;
        match plan_file(f, dirs.expand(&f.path), rendered) {
            Ok(w) => writes.push(w),
            Err(warning) => warnings.push(warning),
        }
    }
    if let Some(kind) = target.def.native {
        let palettes: Vec<&Palette> = std::iter::once(req.palette).chain(req.pair).collect();
        match native::plan(kind, &palettes, dirs) {
            Ok(plan) => {
                writes.extend(plan.writes.into_iter().map(|w| PlannedWrite {
                    path: w.path,
                    contents: w.contents,
                    edit: Edit::File,
                }));
                warnings.extend(plan.warnings);
            }
            Err(e) => {
                return Ok(TargetPlan::skipped(name, SkipReason::Broken(e.to_string()), warnings));
            }
        }
    }
    if writes.is_empty() {
        let reason = if warnings.is_empty() {
            "nothing to write".to_string()
        } else {
            warnings.remove(0)
        };
        return Ok(TargetPlan::skipped(name, SkipReason::Broken(reason), warnings));
    }
    Ok(TargetPlan { name: name.clone(), outcome: Ok(target), writes, warnings })
}

/// The id `RecordWriter::create` just took: the highest numbered record folder.
fn newest_record_id(dirs: &Dirs) -> Option<u32> {
    fs::read_dir(dirs.state.join("irida/applies"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str()?.parse().ok())
        .max()
}

/// Capture every file before it is written. On failure the unfinished record
/// folder, which holds private copies, is deleted.
fn record_writes(dirs: &Dirs, slug: &str, writes: &[&PlannedWrite]) -> Result<Record, ApplyError> {
    let mut writer = RecordWriter::create(dirs, slug).map_err(ApplyError::Record)?;
    let id = newest_record_id(dirs);
    writes
        .iter()
        .try_for_each(|w| {
            let blob = writer.capture(&w.path)?;
            writer.push(w.entry(blob));
            Ok(())
        })
        .and_then(|()| writer.finish())
        .map_err(|e| {
            if let Some(id) = id {
                let _ = record::delete(dirs, id);
            }
            ApplyError::Record(e)
        })
}

pub fn apply(ctx: &Ctx, settings: &Settings, req: &ApplyRequest) -> Result<ApplyReport, ApplyError> {
    let loaded = load_targets(&ctx.dirs);
    if let Some(only) = &req.only
        && let Some(unknown) = only.iter().find(|n| !loaded.iter().any(|t| &t.name == *n))
    {
        return Err(ApplyError::UnknownTarget(unknown.clone()));
    }

    let renderer = Renderer::new(req.palette);
    let mut plans = Vec::new();
    for lt in &loaded {
        let named = match &req.only {
            Some(only) if !only.contains(&lt.name) => continue,
            Some(_) => true,
            None => false,
        };
        plans.push(match select(lt, ctx, settings, named) {
            Ok(target) => plan_target(target, &renderer, req, &ctx.dirs)?,
            Err(reason) => TargetPlan::skipped(&lt.name, reason, Vec::new()),
        });
    }

    let writes: Vec<&PlannedWrite> =
        plans.iter().filter(|p| p.outcome.is_ok()).flat_map(|p| &p.writes).collect();
    if writes.is_empty() {
        let results = plans
            .into_iter()
            .map(|p| {
                // Every planned target has a write, so all of these were skipped.
                let reason = p
                    .outcome
                    .clone()
                    .err()
                    .unwrap_or_else(|| SkipReason::Broken("nothing to write".into()));
                p.into_result(TargetStatus::Skipped(reason))
            })
            .collect();
        return Err(ApplyError::NothingToApply(results));
    }

    let record = record_writes(&ctx.dirs, &req.palette.slug, &writes)?;
    for (i, w) in writes.iter().enumerate() {
        if let Err(source) = atomic_write(&w.path, w.contents.as_bytes()) {
            let completed = Record {
                id: record.id,
                palette: record.palette.clone(),
                entries: record.entries[..i].to_vec(),
            };
            let rolled_back =
                record::restore(&ctx.dirs, &completed, RestoreMode::Rollback).is_empty();
            if rolled_back {
                let _ = record::delete(&ctx.dirs, record.id);
            }
            return Err(ApplyError::Write { path: w.path.clone(), source, rolled_back });
        }
    }

    let targets = plans
        .into_iter()
        .map(|p| {
            let status = match &p.outcome {
                Ok(target) => {
                    TargetStatus::Applied(reload(target.def.reload.as_ref(), &p.name, &ctx.path))
                }
                Err(reason) => TargetStatus::Skipped(reason.clone()),
            };
            p.into_result(status)
        })
        .collect();
    let _ = record::prune(&ctx.dirs, KEEP_RECORDS);
    Ok(ApplyReport { record_id: record.id, targets })
}

/// Undo the newest apply, keeping the user's own edits to block and key files.
pub fn undo(ctx: &Ctx) -> Result<UndoReport, UndoError> {
    let record = record::latest(&ctx.dirs)
        .map_err(UndoError::Io)?
        .ok_or(UndoError::NothingToUndo)?;
    let failures = record::restore(&ctx.dirs, &record, RestoreMode::Undo);
    if failures.is_empty() {
        record::delete(&ctx.dirs, record.id).map_err(UndoError::Io)?;
    }
    Ok(UndoReport { record_id: record.id, palette: record.palette, failures })
}
