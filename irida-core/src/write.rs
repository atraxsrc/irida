//! Atomic, symlink-safe file writes and reversible block and key edits.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Symlink hops followed before giving up, as Linux does.
const MAX_SYMLINK_HOPS: usize = 40;

/// The path to write to: symlinks are followed, even when the final target is
/// missing, so writing creates the target and keeps the link. An existing
/// result is canonicalized.
pub fn resolve_write_path(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    let mut hops = 0;
    while fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink()) {
        if hops == MAX_SYMLINK_HOPS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{}: too many levels of symbolic links", path.display()),
            ));
        }
        hops += 1;
        let target = fs::read_link(&current)?;
        current = match current.parent() {
            Some(parent) => parent.join(target),
            None => target,
        };
    }
    if current.exists() {
        fs::canonicalize(&current)
    } else {
        Ok(current)
    }
}

/// Write via a temporary sibling and rename, keeping symlinks and modes.
pub fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let target = resolve_write_path(path)?;
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(parent)?;
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_string_lossy();
    let tmp = parent.join(format!(".{name}.irida-tmp"));
    let mode = match fs::metadata(&target) {
        Ok(m) => m.permissions().mode() & 0o7777,
        Err(_) => 0o644,
    };
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(contents)?;
        f.set_permissions(fs::Permissions::from_mode(mode))?;
        f.sync_all()?;
        fs::rename(&tmp, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

pub fn begin_marker(comment: &str) -> String {
    format!("{comment} >>> irida >>>")
}

pub fn end_marker(comment: &str) -> String {
    format!("{comment} <<< irida <<<")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockError {
    Broken,
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockError::Broken => write!(f, "irida block markers are unbalanced"),
        }
    }
}

impl std::error::Error for BlockError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEdit {
    pub contents: String,
    pub before: Option<String>,
    pub appended_newline: bool,
}

/// Line indexes of the begin and end markers, `None` when absent.
fn find_block(lines: &[&str], comment: &str) -> Result<Option<(usize, usize)>, BlockError> {
    find_markers(lines, &begin_marker(comment), &end_marker(comment))
}

/// Line indexes of exactly one `begin` line before exactly one `end` line.
fn find_markers(
    lines: &[&str],
    begin: &str,
    end: &str,
) -> Result<Option<(usize, usize)>, BlockError> {
    let begins: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].trim_end() == begin)
        .collect();
    let ends: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].trim_end() == end)
        .collect();
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([b], [e]) if b < e => Ok(Some((*b, *e))),
        _ => Err(BlockError::Broken),
    }
}

fn with_newline(body: &str) -> String {
    if body.ends_with('\n') {
        body.to_string()
    } else {
        format!("{body}\n")
    }
}

pub fn set_block(
    existing: Option<&str>,
    comment: &str,
    body: &str,
) -> Result<BlockEdit, BlockError> {
    let text = existing.unwrap_or("");
    let body = with_newline(body);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    match find_block(&lines, comment)? {
        None => {
            let mut contents = text.to_string();
            let appended_newline = !contents.is_empty() && !contents.ends_with('\n');
            if appended_newline {
                contents.push('\n');
            }
            contents.push_str(&format!(
                "{}\n{body}{}\n",
                begin_marker(comment),
                end_marker(comment)
            ));
            Ok(BlockEdit {
                contents,
                before: None,
                appended_newline,
            })
        }
        Some((b, e)) => {
            let before: String = lines[b + 1..e].concat();
            let contents = format!("{}{body}{}", lines[..=b].concat(), lines[e..].concat());
            Ok(BlockEdit {
                contents,
                before: Some(before),
                appended_newline: false,
            })
        }
    }
}

/// Replace the lines between the `begin` and `end` marker lines with `body`.
/// `Ok(None)` when neither marker is present.
pub fn replace_marked(
    existing: &str,
    begin: &str,
    end: &str,
    body: &str,
) -> Result<Option<String>, BlockError> {
    let lines: Vec<&str> = existing.split_inclusive('\n').collect();
    Ok(find_markers(&lines, begin, end)?.map(|(b, e)| {
        format!(
            "{}{}{}",
            lines[..=b].concat(),
            with_newline(body),
            lines[e..].concat()
        )
    }))
}

pub fn restore_block(
    current: &str,
    comment: &str,
    before: Option<&str>,
    appended_newline: bool,
) -> Result<String, BlockError> {
    let lines: Vec<&str> = current.split_inclusive('\n').collect();
    let Some((b, e)) = find_block(&lines, comment)? else {
        return Ok(current.to_string());
    };
    let after = lines[e + 1..].concat();
    match before {
        Some(old) => Ok(format!(
            "{}{}{}",
            lines[..=b].concat(),
            with_newline(old),
            lines[e..].concat()
        )),
        None => {
            let mut head = lines[..b].concat();
            if appended_newline && after.is_empty() && head.ends_with('\n') {
                head.pop();
            }
            Ok(head + &after)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEdit {
    pub contents: String,
    pub before: Option<String>,
    pub appended_newline: bool,
}

fn is_key_line(line: &str, key: &str) -> bool {
    line.trim_start()
        .strip_prefix(key)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

fn strip_eol(line: &str) -> (&str, &str) {
    match line.strip_suffix('\n') {
        Some(l) => (l, "\n"),
        None => (line, ""),
    }
}

pub fn set_key(existing: Option<&str>, key: &str, value: &str) -> KeyEdit {
    let text = existing.unwrap_or("");
    let new_line = format!("{key} = {value}");
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if let Some(i) = lines.iter().position(|l| is_key_line(l, key)) {
        let (old, eol) = strip_eol(lines[i]);
        let contents = format!(
            "{}{new_line}{eol}{}",
            lines[..i].concat(),
            lines[i + 1..].concat()
        );
        return KeyEdit {
            contents,
            before: Some(old.to_string()),
            appended_newline: false,
        };
    }
    let mut contents = text.to_string();
    let appended_newline = !contents.is_empty() && !contents.ends_with('\n');
    if appended_newline {
        contents.push('\n');
    }
    contents.push_str(&new_line);
    contents.push('\n');
    KeyEdit {
        contents,
        before: None,
        appended_newline,
    }
}

pub fn restore_key(
    current: &str,
    key: &str,
    before: Option<&str>,
    appended_newline: bool,
) -> String {
    let lines: Vec<&str> = current.split_inclusive('\n').collect();
    let Some(i) = lines.iter().position(|l| is_key_line(l, key)) else {
        return current.to_string();
    };
    match before {
        Some(old) => {
            let (_, eol) = strip_eol(lines[i]);
            format!(
                "{}{old}{eol}{}",
                lines[..i].concat(),
                lines[i + 1..].concat()
            )
        }
        None => {
            let mut head = lines[..i].concat();
            let after = lines[i + 1..].concat();
            if appended_newline && after.is_empty() && head.ends_with('\n') {
                head.pop();
            }
            head + &after
        }
    }
}
