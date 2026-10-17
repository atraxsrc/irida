//! The apply transaction: select targets, plan every write, record the
//! before-state, write, roll back on failure, reload; and undo the last apply.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::dirs::{Dirs, DirsError};
use crate::live;
use crate::native::cosmic_term;
use crate::native::{self, NativeUndo};
use crate::palette::Palette;
use crate::record::{self, Entry, Record, RecordWriter, RestoreMode};
use crate::reload::{ReloadStatus, reload};
use crate::store::Settings;
use crate::target::{
    FileEntry, LoadedTarget, Place, Source, Target, WriteKind, is_detected, is_executable,
    load_targets,
};
use crate::template::{Renderer, check_syntax};
use crate::write::{KeyOpts, KeyScope, atomic_write, resolve_write_path, set_block, set_key};

/// Apply records kept after an apply.
pub(crate) const KEEP_RECORDS: usize = 10;

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

    /// `from_env` with its path passed through `app_path`, for the app, which
    /// a dock can start with a short `$PATH`.
    pub fn for_app() -> Result<Ctx, DirsError> {
        let Ctx { dirs, path } = Ctx::from_env()?;
        let path = app_path(&dirs, path);
        Ok(Ctx { dirs, path })
    }
}

/// `path` with the standard binary folders appended where missing:
/// `/usr/local/bin`, `/usr/bin`, `/bin`, `~/.local/bin` and `~/.cargo/bin`.
/// Order is kept and each folder appears once, at its first place.
pub fn app_path(dirs: &Dirs, path: Vec<PathBuf>) -> Vec<PathBuf> {
    let standard = [
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        dirs.home.join(".local/bin"),
        dirs.home.join(".cargo/bin"),
    ];
    let mut out: Vec<PathBuf> = Vec::new();
    for p in path.into_iter().chain(standard) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct ApplyRequest<'a> {
    pub palette: &'a Palette,
    /// The other half of a pair, for targets that hold both modes.
    pub pair: Option<&'a Palette>,
    /// Apply only these targets, even when turned off in settings.
    pub only: Option<Vec<String>>,
    /// Apply only targets that reload live; the others are left out of the report.
    pub live_only: bool,
}

/// What the Apply sheet shows for one target. Nothing is planned or written to find it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetInfo {
    pub name: String,
    /// The name shown to the user; the id when the target sets none.
    pub display: String,
    /// Built into Irida, not a user target folder.
    pub builtin: bool,
    /// Reloads live, so try it live applies it.
    pub live: bool,
    pub detected: bool,
    /// Not turned off in settings.
    pub enabled: bool,
    /// Why the target cannot be applied: its load error, a template syntax
    /// error, or a native target's config-shape error.
    pub broken: Option<String>,
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
    /// The name shown to the user; the id when the target sets none.
    pub display: String,
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
    Render {
        target: String,
        message: String,
    },
    Write {
        path: PathBuf,
        source: io::Error,
        rolled_back: bool,
    },
    NothingToApply(Vec<TargetResult>),
    Record(io::Error),
    /// A live session's record is open; it is kept or reverted in the app.
    LiveSessionOpen,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::UnknownTarget(name) => write!(f, "unknown target: {name}"),
            ApplyError::Render { target, message } => {
                write!(f, "{target}: {message}; nothing was written")
            }
            ApplyError::Write {
                path,
                source,
                rolled_back: true,
            } => write!(
                f,
                "cannot write {}: {source}; every change was rolled back",
                path.display()
            ),
            ApplyError::Write {
                path,
                source,
                rolled_back: false,
            } => write!(
                f,
                "cannot write {}: {source}; rolling back failed, so the apply record was kept \
                 and `irida undo` can retry",
                path.display()
            ),
            ApplyError::NothingToApply(_) => {
                write!(
                    f,
                    "nothing to apply: no installed, enabled target has a file to write"
                )
            }
            ApplyError::Record(e) => {
                write!(f, "cannot save the apply record: {e}; nothing was written")
            }
            ApplyError::LiveSessionOpen => {
                write!(
                    f,
                    "A live session is open; keep or revert it in Irida first"
                )
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
    /// A live session's record is open; it is kept or reverted in the app.
    LiveSessionOpen,
    Io(io::Error),
}

impl fmt::Display for UndoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UndoError::NothingToUndo => write!(f, "nothing to undo"),
            UndoError::LiveSessionOpen => {
                write!(
                    f,
                    "A live session is open; keep or revert it in Irida first"
                )
            }
            UndoError::Io(e) => write!(f, "apply record: {e}"),
        }
    }
}

impl std::error::Error for UndoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UndoError::Io(e) => Some(e),
            UndoError::NothingToUndo | UndoError::LiveSessionOpen => None,
        }
    }
}

/// How a planned write changes its file, with what undo needs to reverse it.
enum Edit {
    File,
    Block {
        comment: String,
        before: Option<String>,
        appended_newline: bool,
    },
    Key {
        key: String,
        separator: String,
        /// The line Irida wrote.
        written: String,
        place: Option<String>,
        section: Option<String>,
        repeatable: bool,
        before: Option<String>,
        appended_newline: bool,
        created_section: bool,
    },
    /// A native key file with the parts undo puts back one by one; never empty.
    Native(Vec<NativeUndo>),
}

/// The full new contents of one file.
struct PlannedWrite {
    path: PathBuf,
    contents: String,
    edit: Edit,
    /// A warning for the result, such as "replaced your import line".
    note: Option<String>,
}

impl PlannedWrite {
    /// The record entries for this write, all sharing the one captured `blob`.
    fn entries(&self, blob: Option<String>) -> Vec<Entry> {
        let path = self.path.clone();
        match &self.edit {
            Edit::File => vec![Entry::File { path, blob }],
            Edit::Block {
                comment,
                before,
                appended_newline,
            } => vec![Entry::Block {
                path,
                blob,
                comment: comment.clone(),
                before: before.clone(),
                appended_newline: *appended_newline,
            }],
            Edit::Key {
                key,
                separator,
                written,
                place,
                section,
                repeatable,
                before,
                appended_newline,
                created_section,
            } => vec![Entry::Key {
                path,
                blob,
                key: key.clone(),
                separator: separator.clone(),
                written: written.clone(),
                place: place.clone(),
                section: section.clone(),
                repeatable: *repeatable,
                before: before.clone(),
                appended_newline: *appended_newline,
                created_section: *created_section,
            }],
            Edit::Native(undo) => undo
                .iter()
                .map(|u| match u {
                    NativeUndo::TermScheme {
                        mode,
                        id,
                        name,
                        before,
                    } => Entry::TermScheme {
                        path: path.clone(),
                        blob: blob.clone(),
                        mode: *mode,
                        id: *id,
                        name: name.clone(),
                        before: before.clone(),
                    },
                    NativeUndo::TermProfileTheme {
                        profile,
                        field,
                        before,
                    } => Entry::TermProfileTheme {
                        path: path.clone(),
                        blob: blob.clone(),
                        profile: *profile,
                        field: field.clone(),
                        before: before.clone(),
                    },
                })
                .collect(),
        }
    }

    /// How many record entries `entries` makes.
    fn entry_count(&self) -> usize {
        match &self.edit {
            Edit::Native(undo) => undo.len(),
            _ => 1,
        }
    }
}

/// One selected target: applied with its writes, or skipped with a reason.
struct TargetPlan<'t> {
    name: String,
    display: String,
    outcome: Result<&'t Target, SkipReason>,
    writes: Vec<PlannedWrite>,
    warnings: Vec<String>,
}

impl TargetPlan<'_> {
    fn skipped(name: &str, display: &str, reason: SkipReason, warnings: Vec<String>) -> Self {
        TargetPlan {
            name: name.to_string(),
            display: display.to_string(),
            outcome: Err(reason),
            writes: Vec::new(),
            warnings,
        }
    }

    fn into_result(self, status: TargetStatus) -> TargetResult {
        TargetResult {
            name: self.name,
            display: self.display,
            status,
            warnings: self.warnings,
        }
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
    let target = lt
        .result
        .as_ref()
        .map_err(|e| SkipReason::Broken(e.to_string()))?;
    if !is_detected(&target.def, &ctx.dirs, &ctx.path) {
        return Err(SkipReason::NotInstalled);
    }
    check_templates(target).map_err(SkipReason::Broken)?;
    Ok(target)
}

/// The first template syntax error of a target's files.
fn check_templates(target: &Target) -> Result<(), String> {
    for f in &target.def.files {
        if let Some(source) = target.templates.get(&f.template) {
            check_syntax(&f.template, source).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Every target, sorted by name, with whether it is installed, turned on and
/// broken. Reads target folders and native config keys; writes nothing.
pub fn target_status(ctx: &Ctx, settings: &Settings) -> Vec<TargetInfo> {
    let user_dir = ctx.dirs.config.join("irida/targets");
    let mut out: Vec<TargetInfo> = load_targets(&ctx.dirs)
        .into_iter()
        .map(|lt| {
            let enabled = !settings.disabled.contains(&lt.name);
            match &lt.result {
                Ok(target) => {
                    let broken = check_templates(target).err().or_else(|| {
                        target
                            .def
                            .native
                            .and_then(|kind| native::check(kind, &ctx.dirs).err())
                            .map(|e| e.to_string())
                    });
                    TargetInfo {
                        display: target.display().to_string(),
                        builtin: target.source == Source::Builtin,
                        live: target.def.live,
                        detected: is_detected(&target.def, &ctx.dirs, &ctx.path),
                        enabled,
                        broken,
                        name: lt.name,
                    }
                }
                // A target that does not load has no detect rules; it is
                // shown as detected so its error is seen, not folded away.
                Err(e) => TargetInfo {
                    display: lt.name.clone(),
                    builtin: !user_dir.join(&lt.name).exists(),
                    live: false,
                    detected: true,
                    enabled,
                    broken: Some(e.to_string()),
                    name: lt.name,
                },
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The current text of `path`; `None` when it does not exist.
fn read_existing(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// The first rendered line missing from `existing`, with the nearest earlier
/// rendered line that is less indented, both trimmed.
fn missing_line<'r>(rendered: &'r str, existing: &str) -> Option<(&'r str, Option<&'r str>)> {
    let have: Vec<&str> = existing.lines().map(str::trim).collect();
    let indent = |l: &str| l.len() - l.trim_start().len();
    let lines: Vec<&str> = rendered.lines().collect();
    let i = lines
        .iter()
        .position(|l| !l.trim().is_empty() && !have.contains(&l.trim()))?;
    let parent = lines[..i]
        .iter()
        .rev()
        .find(|l| !l.trim().is_empty() && indent(l) < indent(lines[i]))
        .map(|l| l.trim());
    Some((lines[i].trim(), parent))
}

/// Plan one `[[file]]`. `Err` is a warning: the file is left alone. `Ok(None)`
/// drops the write with nothing to report.
fn plan_file(
    f: &FileEntry,
    path: PathBuf,
    rendered: String,
    prior_lines: &[&str],
) -> Result<Option<PlannedWrite>, String> {
    let skip = |why: &dyn fmt::Display| format!("{}: {why}; skipped", path.display());
    if f.write == WriteKind::File {
        if f.create_only {
            let resolved = resolve_write_path(&path).map_err(|e| skip(&e))?;
            if resolved.exists() {
                let existing = fs::read_to_string(&resolved).map_err(|e| skip(&e))?;
                return match missing_line(&rendered, &existing) {
                    None => Ok(None),
                    Some((line, parent)) => {
                        let under = parent.map(|p| format!(" under `{p}`")).unwrap_or_default();
                        Err(format!(
                            "{}: already exists, so Irida left it alone; add `{line}`{under}",
                            path.display()
                        ))
                    }
                };
            }
        }
        return Ok(Some(PlannedWrite {
            path,
            contents: rendered,
            edit: Edit::File,
            note: None,
        }));
    }
    let existing = read_existing(&path).map_err(|e| skip(&e))?;
    match f.write {
        WriteKind::Block => {
            let comment = f
                .comment
                .clone()
                .ok_or_else(|| skip(&"block needs comment"))?;
            let edit = set_block(existing.as_deref(), &comment, &rendered)
                .map_err(|_| skip(&"irida markers are broken"))?;
            Ok(Some(PlannedWrite {
                path,
                contents: edit.contents,
                edit: Edit::Block {
                    comment,
                    before: edit.before,
                    appended_newline: edit.appended_newline,
                },
                note: None,
            }))
        }
        _ => {
            let key = f.key.clone().ok_or_else(|| skip(&"key needs key"))?;
            let value = rendered.trim_end_matches(['\n', '\r']);
            let scope = match (&f.place, &f.section) {
                (_, Some(s)) => KeyScope::Section(s.clone()),
                (Some(Place::Top), None) => KeyScope::Top,
                _ => KeyScope::End,
            };
            let opts = KeyOpts {
                separator: f.separator.clone().unwrap_or_else(|| " = ".to_string()),
                scope,
                repeatable: f.repeatable,
            };
            let edit = set_key(existing.as_deref(), &key, value, &opts);
            let written = format!("{key}{}{value}", opts.separator);
            let is_irida = |old: &str| {
                old.trim() == written.trim() || prior_lines.iter().any(|l| l.trim() == old.trim())
            };
            let note = match &edit.before {
                Some(old) if f.note_replaced && !is_irida(old) => Some(format!(
                    "{}: replaced your {key} line; irida undo restores it",
                    path.display()
                )),
                _ => None,
            };
            Ok(Some(PlannedWrite {
                path,
                contents: edit.contents,
                note,
                edit: Edit::Key {
                    written,
                    key,
                    separator: opts.separator,
                    place: matches!(f.place, Some(Place::Top)).then(|| "top".to_string()),
                    section: f.section.clone(),
                    repeatable: opts.repeatable,
                    before: edit.before,
                    appended_newline: edit.appended_newline,
                    created_section: edit.created_section,
                },
            }))
        }
    }
}

/// Whether a `file` write replaces an existing file with different bytes that
/// the previous apply did not write.
/// What the previous apply (and an open live session) wrote: every path, and
/// every key line.
struct Prior {
    paths: Vec<PathBuf>,
    key_lines: Vec<(PathBuf, String)>,
}

impl Prior {
    fn from_entries<'e>(entries: impl Iterator<Item = &'e Entry>) -> Prior {
        let mut prior = Prior {
            paths: Vec::new(),
            key_lines: Vec::new(),
        };
        for e in entries {
            prior.paths.push(e.path().to_path_buf());
            if let Entry::Key { path, written, .. } = e
                && !written.is_empty()
            {
                prior.key_lines.push((path.clone(), written.clone()));
            }
        }
        prior
    }

    /// The key lines Irida wrote to `path` last time.
    fn lines_of(&self, path: &Path) -> Vec<&str> {
        self.key_lines
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, l)| l.as_str())
            .collect()
    }
}

fn replaces_user_file(w: &PlannedWrite, recorded: &[PathBuf]) -> bool {
    if recorded.contains(&w.path) {
        return false;
    }
    match fs::read(&w.path) {
        Ok(bytes) => bytes != w.contents.as_bytes(),
        Err(_) => false,
    }
}

/// Plan every write of one target. A render error stops the whole apply; a
/// native planning error, or no write left, skips only this target.
fn plan_target<'t>(
    target: &'t Target,
    renderer: &Renderer,
    req: &ApplyRequest,
    dirs: &Dirs,
    prior: &Prior,
) -> Result<TargetPlan<'t>, ApplyError> {
    let name = &target.def.name;
    let mut writes = Vec::new();
    let mut warnings: Vec<String> = target.def.note.iter().cloned().collect();
    for f in &target.def.files {
        let render_error = |message: String| ApplyError::Render {
            target: name.clone(),
            message,
        };
        let source = target
            .templates
            .get(&f.template)
            .ok_or_else(|| render_error(format!("missing template {}", f.template)))?;
        let rendered = renderer
            .render(&f.template, source)
            .map_err(|e| render_error(e.to_string()))?;
        let path = dirs.expand(&f.path);
        let prior_lines = prior.lines_of(&path);
        match plan_file(f, path, rendered, &prior_lines) {
            Ok(Some(mut w)) => {
                if f.write == WriteKind::File && replaces_user_file(&w, &prior.paths) {
                    warnings.push(replaced_note(&w.path));
                }
                warnings.extend(w.note.take());
                writes.push(w);
            }
            Ok(None) => {}
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
                    note: None,
                    edit: if w.undo.is_empty() {
                        Edit::File
                    } else {
                        Edit::Native(w.undo)
                    },
                }));
                warnings.extend(plan.warnings);
            }
            Err(e) => {
                return Ok(TargetPlan::skipped(
                    name,
                    target.display(),
                    SkipReason::Broken(e.to_string()),
                    warnings,
                ));
            }
        }
    }
    Ok(TargetPlan {
        name: name.clone(),
        display: target.display().to_string(),
        outcome: Ok(target),
        writes,
        warnings,
    }
    .or_skipped())
}

/// The note for a `file` write over a file the user wrote.
fn replaced_note(path: &Path) -> String {
    format!(
        "{}: replaced your file; irida undo restores it",
        path.display()
    )
}

impl TargetPlan<'_> {
    /// A selected target left with no write is skipped, its first warning the reason.
    fn or_skipped(mut self) -> Self {
        if self.outcome.is_err() || !self.writes.is_empty() {
            return self;
        }
        let reason = if self.warnings.is_empty() {
            "nothing to write".to_string()
        } else {
            self.warnings.remove(0)
        };
        self.outcome = Err(SkipReason::Broken(reason));
        self
    }

    /// Drop writes to a file an earlier write already plans, recording this
    /// target's own writes in `seen`.
    fn guard_paths(mut self, seen: &mut Vec<(PathBuf, String)>) -> Self {
        if self.outcome.is_err() {
            return self;
        }
        let mut kept = Vec::new();
        for w in std::mem::take(&mut self.writes) {
            let key = resolve_write_path(&w.path).unwrap_or_else(|_| w.path.clone());
            match seen.iter().find(|(p, _)| *p == key) {
                Some((_, first)) => {
                    // The dropped write replaces nothing, so its note goes too.
                    let note = replaced_note(&w.path);
                    if let Some(i) = self.warnings.iter().position(|m| *m == note) {
                        self.warnings.remove(i);
                    }
                    self.warnings.push(format!(
                        "{}: already written by {first}; skipped",
                        w.path.display()
                    ));
                }
                None => {
                    seen.push((key, self.name.clone()));
                    kept.push(w);
                }
            }
        }
        self.writes = kept;
        self.or_skipped()
    }
}

/// Capture every file before it is written and save the record; then write,
/// rolling back from the record when a write fails.
fn write_recorded(
    dirs: &Dirs,
    rec: &mut RecordWriter,
    writes: &[&PlannedWrite],
) -> Result<(), ApplyError> {
    for w in writes {
        let blob = rec.capture(&w.path).map_err(ApplyError::Record)?;
        for e in w.entries(blob) {
            rec.push(e);
        }
    }
    rec.save().map_err(ApplyError::Record)?;
    // Entries of the writes done so far; one write can make several entries.
    let mut done = 0;
    for w in writes {
        if let Err(source) = atomic_write(&w.path, w.contents.as_bytes()) {
            let record = rec.record();
            let completed = Record {
                id: record.id,
                palette: record.palette.clone(),
                entries: record.entries[..done].to_vec(),
            };
            let rolled_back = record::restore(dirs, &completed, RestoreMode::Rollback).is_empty();
            return Err(ApplyError::Write {
                path: w.path.clone(),
                source,
                rolled_back,
            });
        }
        done += w.entry_count();
    }
    Ok(())
}

/// The bytes at `path` now, through symlinks; `None` when it does not exist.
fn current_bytes(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(resolve_write_path(path)?) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Add a live session apply's writes to its record. A path is captured only
/// the first time the session writes it; after that only new COSMIC Terminal
/// schemes and profile fields get entries, their `before` read from the
/// session's captured copy, so undo goes back to before the session.
fn add_session_entries(rec: &mut RecordWriter, writes: &[&PlannedWrite]) -> io::Result<()> {
    for w in writes {
        if !rec.has(&w.path) {
            let blob = rec.capture(&w.path)?;
            for e in w.entries(blob) {
                rec.push(e);
            }
            continue;
        }
        let Edit::Native(undo) = &w.edit else {
            continue;
        };
        let (blob, text) = match rec.captured(&w.path)? {
            Some((name, bytes)) => (Some(name), String::from_utf8(bytes).ok()),
            None => (None, None),
        };
        for u in undo {
            match u {
                NativeUndo::TermScheme { mode, id, name, .. } => {
                    let known = rec.entries_mut().iter_mut().find_map(|e| match e {
                        Entry::TermScheme {
                            path,
                            mode: m,
                            id: i,
                            name: n,
                            ..
                        } if *path == w.path && m == mode && i == id => Some(n),
                        _ => None,
                    });
                    match known {
                        // Undo must recognise the scheme this apply wrote there.
                        Some(n) => n.clone_from(name),
                        None => rec.push(Entry::TermScheme {
                            path: w.path.clone(),
                            blob: blob.clone(),
                            mode: *mode,
                            id: *id,
                            name: name.clone(),
                            before: text
                                .as_deref()
                                .and_then(|t| cosmic_term::scheme_before(t, *id)),
                        }),
                    }
                }
                NativeUndo::TermProfileTheme { profile, field, .. } => {
                    let known = rec.record().entries.iter().any(|e| {
                        matches!(e, Entry::TermProfileTheme { path, profile: p, field: f, .. }
                            if *path == w.path && p == profile && f == field)
                    });
                    if !known {
                        rec.push(Entry::TermProfileTheme {
                            path: w.path.clone(),
                            blob: blob.clone(),
                            profile: *profile,
                            field: field.clone(),
                            before: text.as_deref().and_then(|t| {
                                cosmic_term::profile_theme_before(t, *profile, field)
                            }),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// A live session's writes: record what is new, save the record before the
/// first write, then write. A failed write puts back only this call's writes
/// from the bytes read just before it, and sets the record back to how it was
/// before this call; the record itself is never deleted.
fn write_session(rec: &mut RecordWriter, writes: &[&PlannedWrite]) -> Result<(), ApplyError> {
    let before = writes
        .iter()
        .map(|w| current_bytes(&w.path))
        .collect::<io::Result<Vec<_>>>()
        .map_err(ApplyError::Record)?;
    let checkpoint = rec.checkpoint();
    if let Err(e) = add_session_entries(rec, writes).and_then(|()| rec.save()) {
        let _ = rec.rewind(checkpoint);
        return Err(ApplyError::Record(e));
    }
    for (i, w) in writes.iter().enumerate() {
        if let Err(source) = atomic_write(&w.path, w.contents.as_bytes()) {
            let failed = writes[..i]
                .iter()
                .zip(&before)
                .rev()
                .filter(|(w, bytes)| {
                    match bytes {
                        Some(b) => atomic_write(&w.path, b),
                        None => record::remove(&w.path),
                    }
                    .is_err()
                })
                .count();
            let rolled_back = failed == 0;
            if rolled_back {
                let _ = rec.rewind(checkpoint);
            }
            return Err(ApplyError::Write {
                path: w.path.clone(),
                source,
                rolled_back,
            });
        }
    }
    Ok(())
}

/// Apply into a new record, then prune old records. Refused while a live
/// session is open: its record must stay the one undo puts back first.
pub fn apply(
    ctx: &Ctx,
    settings: &Settings,
    req: &ApplyRequest,
) -> Result<ApplyReport, ApplyError> {
    if live::session_open(&ctx.dirs) {
        return Err(ApplyError::LiveSessionOpen);
    }
    let mut rec = RecordWriter::create(&ctx.dirs, &req.palette.slug).map_err(ApplyError::Record)?;
    match apply_into(ctx, settings, req, &mut rec, false) {
        Ok(report) => {
            let _ = record::prune(&ctx.dirs, KEEP_RECORDS);
            Ok(report)
        }
        // Rolling back failed, so the record is kept for `irida undo` to retry.
        Err(
            e @ ApplyError::Write {
                rolled_back: false, ..
            },
        ) => Err(e),
        Err(e) => {
            // The unfinished folder holds private copies; the apply error is
            // returned even when this cleanup fails.
            let _ = rec.abort();
            Err(e)
        }
    }
}

/// Select, plan, record and write one apply into `rec`, then reload. With
/// `session` false, `rec` is fresh and the caller cleans it up on failure;
/// with `session` true, `rec` is a live session's record (see `write_session`).
/// Nothing is pruned here.
pub(crate) fn apply_into(
    ctx: &Ctx,
    settings: &Settings,
    req: &ApplyRequest,
    rec: &mut RecordWriter,
    session: bool,
) -> Result<ApplyReport, ApplyError> {
    let loaded = load_targets(&ctx.dirs);
    if let Some(only) = &req.only
        && let Some(unknown) = only.iter().find(|n| !loaded.iter().any(|t| &t.name == *n))
    {
        return Err(ApplyError::UnknownTarget(unknown.clone()));
    }

    let renderer = Renderer::new(req.palette);
    // Files Irida wrote last time: the newest record's, and an open session's.
    let latest = record::latest(&ctx.dirs)
        .ok()
        .flatten()
        .map(|r| r.entries)
        .unwrap_or_default();
    let prior = Prior::from_entries(latest.iter().chain(&rec.record().entries));
    let mut seen = Vec::new();
    let mut plans = Vec::new();
    for lt in &loaded {
        let named = match &req.only {
            Some(only) if !only.contains(&lt.name) => continue,
            Some(_) => true,
            None => false,
        };
        if req.live_only && !lt.result.as_ref().is_ok_and(|t| t.def.live) {
            continue;
        }
        plans.push(match select(lt, ctx, settings, named) {
            Ok(target) => {
                plan_target(target, &renderer, req, &ctx.dirs, &prior)?.guard_paths(&mut seen)
            }
            Err(reason) => {
                let display = lt.result.as_ref().map_or(lt.name.as_str(), |t| t.display());
                TargetPlan::skipped(&lt.name, display, reason, Vec::new())
            }
        });
    }

    let writes: Vec<&PlannedWrite> = plans
        .iter()
        .filter(|p| p.outcome.is_ok())
        .flat_map(|p| &p.writes)
        .collect();
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

    if session {
        write_session(rec, &writes)?;
    } else {
        write_recorded(&ctx.dirs, rec, &writes)?;
    }

    let targets = plans
        .into_iter()
        .map(|p| {
            let status = match &p.outcome {
                Ok(target) => TargetStatus::Applied(reload_target(target, ctx)),
                Err(reason) => TargetStatus::Skipped(reason.clone()),
            };
            p.into_result(status)
        })
        .collect();
    Ok(ApplyReport {
        record_id: rec.id(),
        targets,
    })
}

/// Reload one applied target. A signal is sent only when the target's binary
/// is on the path: an app found only by its config folder cannot be running.
pub(crate) fn reload_target(target: &Target, ctx: &Ctx) -> ReloadStatus {
    let r = target.def.reload.as_ref();
    // A signal goes only to a target whose binary is on PATH; one found only
    // by its config folder, or with no binary named, cannot be running.
    if r.is_some_and(|r| r.signal.is_some())
        && !target
            .def
            .detect
            .binary
            .as_ref()
            .is_some_and(|bin| ctx.path.iter().any(|p| is_executable(&p.join(bin))))
    {
        return ReloadStatus::NotRunning;
    }
    reload(r, &target.def.name, &ctx.path)
}

/// Undo the newest apply, keeping the user's own edits to block and key files.
/// Refuses while a live session's record is open.
pub fn undo(ctx: &Ctx) -> Result<UndoReport, UndoError> {
    if live::session_open(&ctx.dirs) {
        return Err(UndoError::LiveSessionOpen);
    }
    let record = record::latest(&ctx.dirs)
        .map_err(UndoError::Io)?
        .ok_or(UndoError::NothingToUndo)?;
    let failures = record::restore(&ctx.dirs, &record, RestoreMode::Undo);
    if failures.is_empty() {
        record::delete(&ctx.dirs, record.id).map_err(UndoError::Io)?;
    }
    Ok(UndoReport {
        record_id: record.id,
        palette: record.palette,
        failures,
    })
}
