//! Apply records: what every file looked like before an apply, for rollback and undo.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dirs::Dirs;
use crate::write::{atomic_write, resolve_write_path, restore_block, restore_key};

/// One file an apply touched. `blob` names a file inside the record folder holding
/// the bytes before the apply; `None` means the file did not exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Entry {
    File {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
    },
    Block {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
        comment: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
        appended_newline: bool,
    },
    Key {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
        appended_newline: bool,
    },
}

impl Entry {
    pub fn path(&self) -> &Path {
        match self {
            Entry::File { path, .. } | Entry::Block { path, .. } | Entry::Key { path, .. } => path,
        }
    }

    pub fn blob(&self) -> Option<&str> {
        match self {
            Entry::File { blob, .. } | Entry::Block { blob, .. } | Entry::Key { blob, .. } => {
                blob.as_deref()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: u32,
    /// The palette slug.
    pub palette: String,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    /// Put every file back byte for byte.
    Rollback,
    /// Remove only Irida's blocks and keys, keeping later user edits.
    Undo,
}

const RECORD_FILE: &str = "record.toml";

fn applies_dir(dirs: &Dirs) -> PathBuf {
    dirs.state.join("irida/applies")
}

fn record_dir(dirs: &Dirs, id: u32) -> PathBuf {
    applies_dir(dirs).join(id.to_string())
}

/// Record folder numbers, ascending; empty when there is no applies folder.
fn record_ids(dirs: &Dirs) -> io::Result<Vec<u32>> {
    let rd = match fs::read_dir(applies_dir(dirs)) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut ids: Vec<u32> = rd
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok()?.parse().ok())
        .collect();
    ids.sort_unstable();
    Ok(ids)
}

fn is_absent(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory)
}

pub struct RecordWriter {
    dir: PathBuf,
    record: Record,
    blobs: usize,
}

impl RecordWriter {
    /// Start a record in a new folder numbered one above the highest existing one.
    pub fn create(dirs: &Dirs, palette_slug: &str) -> io::Result<RecordWriter> {
        fs::create_dir_all(applies_dir(dirs))?;
        let id = record_ids(dirs)?.last().map_or(1, |n| n + 1);
        let dir = record_dir(dirs, id);
        fs::create_dir(&dir)?;
        Ok(RecordWriter {
            dir,
            record: Record { id, palette: palette_slug.to_string(), entries: Vec::new() },
            blobs: 0,
        })
    }

    /// Copy the current bytes of `path` into the record. `None` when it does not exist.
    pub fn capture(&mut self, path: &Path) -> io::Result<Option<String>> {
        let bytes = match fs::read(resolve_write_path(path)?) {
            Ok(b) => b,
            Err(e) if is_absent(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        let name = format!("files/{}", self.blobs);
        fs::create_dir_all(self.dir.join("files"))?;
        fs::write(self.dir.join(&name), bytes)?;
        self.blobs += 1;
        Ok(Some(name))
    }

    pub fn push(&mut self, e: Entry) {
        self.record.entries.push(e);
    }

    /// Write `record.toml` and return the record.
    pub fn finish(self) -> io::Result<Record> {
        let text = toml::to_string(&self.record).map_err(io::Error::other)?;
        atomic_write(&self.dir.join(RECORD_FILE), text.as_bytes())?;
        Ok(self.record)
    }
}

/// The newest finished record, if any.
pub fn latest(dirs: &Dirs) -> io::Result<Option<Record>> {
    for id in record_ids(dirs)?.into_iter().rev() {
        let path = record_dir(dirs, id).join(RECORD_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let rec = toml::from_str(&text).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", path.display()))
        })?;
        return Ok(Some(rec));
    }
    Ok(None)
}

/// Restore every entry, newest first, and return the ones that failed.
pub fn restore(dirs: &Dirs, rec: &Record, mode: RestoreMode) -> Vec<(PathBuf, io::Error)> {
    let dir = record_dir(dirs, rec.id);
    rec.entries
        .iter()
        .rev()
        .filter_map(|e| {
            let result = match (mode, e) {
                (RestoreMode::Undo, Entry::Block { .. } | Entry::Key { .. }) => undo_edit(e),
                _ => put_back(&dir, e.path(), e.blob()),
            };
            result.err().map(|err| (e.path().to_path_buf(), err))
        })
        .collect()
}

/// Write the captured bytes back, or delete the file when there were none.
fn put_back(dir: &Path, path: &Path, blob: Option<&str>) -> io::Result<()> {
    match blob {
        Some(name) => atomic_write(path, &fs::read(dir.join(name))?),
        None => remove(path),
    }
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if !is_absent(&e) => Err(e),
        _ => Ok(()),
    }
}

/// Take Irida's block or key out of the current file, keeping everything else.
fn undo_edit(e: &Entry) -> io::Result<()> {
    let path = e.path();
    let current = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(err) if is_absent(&err) => return Ok(()),
        Err(err) => return Err(err),
    };
    let restored = match e {
        Entry::Block { comment, before, appended_newline, .. } => {
            restore_block(&current, comment, before.as_deref(), *appended_newline)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?
        }
        Entry::Key { key, before, appended_newline, .. } => {
            restore_key(&current, key, before.as_deref(), *appended_newline)
        }
        Entry::File { .. } => return Ok(()),
    };
    if e.blob().is_none() && restored.is_empty() {
        return remove(path);
    }
    if restored == current {
        return Ok(());
    }
    atomic_write(path, restored.as_bytes())
}

pub fn delete(dirs: &Dirs, id: u32) -> io::Result<()> {
    fs::remove_dir_all(record_dir(dirs, id))
}

/// Keep the `keep` newest records and delete the rest.
pub fn prune(dirs: &Dirs, keep: usize) -> io::Result<()> {
    let ids = record_ids(dirs)?;
    let excess = ids.len().saturating_sub(keep);
    for id in &ids[..excess] {
        delete(dirs, *id)?;
    }
    Ok(())
}
