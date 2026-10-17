//! Atomic, symlink-safe file writes and reversible block and key edits.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
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
    let mode = match fs::metadata(&target) {
        Ok(m) => m.permissions().mode() & 0o7777,
        Err(_) => 0o644,
    };
    // A fresh, unpredictable name created exclusively, so a file or symlink
    // already in the folder (a cloned repo can carry one) is never opened.
    // Dropped unpersisted, the temporary file removes itself.
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".irida-tmp")
        .tempfile_in(parent)?;
    tmp.write_all(contents)?;
    tmp.as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    tmp.as_file().sync_all()?;
    tmp.persist(&target).map_err(|e| e.error)?;
    Ok(())
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

/// Where a key line lives in its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyScope {
    /// Anywhere; a missing line is appended at the end.
    End,
    /// Before the first `[section]` header.
    Top,
    /// Inside the named `[section]`.
    Section(String),
}

/// How a key line is matched and written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyOpts {
    /// The exact text between key and value, `" = "` by default.
    pub separator: String,
    pub scope: KeyScope,
    /// The key may appear more than once: only a line carrying Irida's own
    /// value matches, any other line with the key is left alone.
    pub repeatable: bool,
}

impl Default for KeyOpts {
    fn default() -> KeyOpts {
        KeyOpts {
            separator: " = ".to_string(),
            scope: KeyScope::End,
            repeatable: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEdit {
    pub contents: String,
    pub before: Option<String>,
    pub appended_newline: bool,
    /// Irida added the `[section]` header too.
    pub created_section: bool,
}

/// Whether `line` is `key`, optional spaces, then the separator's first
/// non-space character; a separator with no such character needs whitespace
/// and something after it.
fn is_key_line(line: &str, key: &str, separator: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix(key) else {
        return false;
    };
    match separator.trim().chars().next() {
        Some(c) => rest.trim_start().starts_with(c),
        None => rest.starts_with(char::is_whitespace) && !rest.trim().is_empty(),
    }
}

fn strip_eol(line: &str) -> (&str, &str) {
    match line.strip_suffix('\n') {
        Some(l) => (l, "\n"),
        None => (line, ""),
    }
}

pub fn set_key(existing: Option<&str>, key: &str, value: &str, opts: &KeyOpts) -> KeyEdit {
    let text = existing.unwrap_or("");
    let new_line = format!("{key}{}{value}", opts.separator);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if let Some(i) = lines
        .iter()
        .position(|l| is_key_line(l, key, &opts.separator))
    {
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
            created_section: false,
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
        created_section: false,
    }
}

/// Put back the line a `set_key` replaced, or remove the one it added.
/// `written` is the line Irida wrote.
pub fn restore_key(
    current: &str,
    key: &str,
    written: &str,
    before: Option<&str>,
    appended_newline: bool,
    created_section: bool,
    opts: &KeyOpts,
) -> String {
    let _ = (written, created_section);
    let lines: Vec<&str> = current.split_inclusive('\n').collect();
    let Some(i) = lines
        .iter()
        .position(|l| is_key_line(l, key, &opts.separator))
    else {
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
