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

/// The line ending of `text`'s first line: `"\r\n"` for a CRLF file, `"\n"`
/// otherwise, also for a file with no line break yet.
fn eol_of(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if text[..i].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// `body` with every line ending in `eol`.
fn with_eol(body: &str, eol: &str) -> String {
    if eol == "\n" {
        return body.to_string();
    }
    body.split_inclusive('\n')
        .map(|l| match l.strip_suffix('\n') {
            Some(l) => format!("{}{eol}", l.strip_suffix('\r').unwrap_or(l)),
            None => l.to_string(),
        })
        .collect()
}

/// `text` without the one line ending at its end.
fn pop_eol(text: &mut String) {
    if text.pop() == Some('\n') && text.ends_with('\r') {
        text.pop();
    }
}

pub fn set_block(
    existing: Option<&str>,
    comment: &str,
    body: &str,
) -> Result<BlockEdit, BlockError> {
    let text = existing.unwrap_or("");
    let eol = eol_of(text);
    let body = with_eol(&with_newline(body), eol);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    match find_block(&lines, comment)? {
        None => {
            let mut contents = text.to_string();
            let appended_newline = !contents.is_empty() && !contents.ends_with('\n');
            if appended_newline {
                contents.push_str(eol);
            }
            contents.push_str(&format!(
                "{}{eol}{body}{}{eol}",
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
                pop_eol(&mut head);
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
    if let Some(l) = line.strip_suffix("\r\n") {
        return (l, "\r\n");
    }
    match line.strip_suffix('\n') {
        Some(l) => (l, "\n"),
        None => (line, ""),
    }
}

/// The name of a `[section]` header line, with spaces inside the brackets
/// and a trailing `#` or `;` comment ignored; `None` for any other line.
fn header_name(line: &str) -> Option<&str> {
    let t = line.trim().strip_prefix('[')?;
    let close = t.find(']')?;
    let rest = t[close + 1..].trim();
    if !(rest.is_empty() || rest.starts_with('#') || rest.starts_with(';')) {
        return None;
    }
    Some(t[..close].trim())
}

/// Whether `line` is a `[section]` header.
fn is_header(line: &str) -> bool {
    header_name(line).is_some()
}

/// Whether `text` sets `section` before its first header, as an inline
/// table (`section = {...}`) or a dotted key (`section.key = ...`), so a
/// `[section]` header would define it twice.
pub fn sets_inline(text: &str, section: &str) -> bool {
    text.lines()
        .take_while(|l| !is_header(l))
        .any(|l| match l.trim_start().strip_prefix(section) {
            Some(rest) => rest.trim_start().starts_with(['=', '.']),
            None => false,
        })
}

/// `depth` after the brackets of `s` outside double quotes and before a
/// `#` comment: `[` and `{` open, `]` and `}` close.
fn bracket_depth(s: &str, mut depth: i32) -> i32 {
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            '[' | '{' if !quoted => depth += 1,
            ']' | '}' if !quoted => depth -= 1,
            '#' if !quoted => break,
            _ => {}
        }
    }
    depth
}

/// The entries of `names` that `text` sets: a key with a line of its own
/// (matched as with `separator`), or for a `[table]` entry that table's header
/// or a subtable's. In the order of `names`.
pub fn overrides<'n>(text: &str, names: &'n [String], separator: &str) -> Vec<&'n str> {
    names
        .iter()
        .map(String::as_str)
        .filter(
            |name| match name.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
                Some(table) => text.lines().filter_map(header_name).any(|h| {
                    h == table || h.strip_prefix(table).is_some_and(|r| r.starts_with('.'))
                }),
                None => text.lines().any(|l| is_key_line(l, name, separator)),
            },
        )
        .collect()
}

/// Where a missing line goes.
enum Insert {
    /// At the end of the file.
    Append,
    /// Before line `i`.
    Before(usize),
    /// At the end of the file, after a new `[section]` header.
    NewSection(String),
}

/// The lines a scope covers and where a missing line is inserted.
fn locate(lines: &[&str], scope: &KeyScope) -> (std::ops::Range<usize>, Insert) {
    let n = lines.len();
    match scope {
        KeyScope::End => (0..n, Insert::Append),
        KeyScope::Top => match lines.iter().position(|l| is_header(l)) {
            Some(h) => (0..h, Insert::Before(h)),
            None => (0..n, Insert::Append),
        },
        KeyScope::Section(name) => {
            let Some(h) = lines
                .iter()
                .position(|l| header_name(l) == Some(name.as_str()))
            else {
                return (n..n, Insert::NewSection(name.clone()));
            };
            let end = (h + 1..n).find(|&i| is_header(lines[i])).unwrap_or(n);
            let last = (h + 1..end)
                .rev()
                .find(|&i| !lines[i].trim().is_empty())
                .map_or(h + 1, |i| i + 1);
            (h + 1..end, Insert::Before(last))
        }
    }
}

/// The value of a key line: what follows the key and the separator, trimmed.
fn key_value<'l>(line: &'l str, key: &str, separator: &str) -> Option<&'l str> {
    if !is_key_line(line, key, separator) {
        return None;
    }
    let rest = line.trim_start().strip_prefix(key)?.trim_start();
    let rest = match separator.trim().chars().next() {
        Some(c) => rest.strip_prefix(c)?,
        None => rest,
    };
    Some(rest.trim())
}

/// The first line in `range` with `key`; with `own`, only a line whose value
/// equals `own`'s. A line `own` has no value for matches any line with the key.
fn find_line(
    lines: &[&str],
    range: std::ops::Range<usize>,
    key: &str,
    own: Option<&str>,
    separator: &str,
) -> Option<usize> {
    let want = own.and_then(|w| key_value(w, key, separator));
    range.into_iter().find(|&i| match want {
        Some(v) => key_value(lines[i], key, separator) == Some(v),
        None => is_key_line(lines[i], key, separator),
    })
}

pub fn set_key(existing: Option<&str>, key: &str, value: &str, opts: &KeyOpts) -> KeyEdit {
    let text = existing.unwrap_or("");
    let eol = eol_of(text);
    let new_line = format!("{key}{}{value}", opts.separator);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let (range, insert) = locate(&lines, &opts.scope);
    let end = range.end;
    // A repeatable key replaces only its own line, any other key any line.
    let own = opts.repeatable.then_some(new_line.as_str());
    if let Some(i) = find_line(&lines, range, key, own, &opts.separator) {
        // A value that opens a bracket and closes it on a later line (a TOML
        // array over several lines) is replaced as a whole.
        let mut j = i;
        if !opts.repeatable {
            let after_key = lines[i].trim_start().strip_prefix(key).unwrap_or("");
            let mut depth = bracket_depth(after_key, 0);
            while depth > 0 && j + 1 < end {
                j += 1;
                depth = bracket_depth(lines[j], depth);
            }
        }
        let span = lines[i..=j].concat();
        let (old, line_eol) = strip_eol(&span);
        // The replaced line keeps its indent.
        let indent = &lines[i][..lines[i].len() - lines[i].trim_start().len()];
        let contents = format!(
            "{}{indent}{new_line}{line_eol}{}",
            lines[..i].concat(),
            lines[j + 1..].concat()
        );
        return KeyEdit {
            contents,
            before: Some(old.to_string()),
            appended_newline: false,
            created_section: false,
        };
    }
    if let Insert::Before(i) = insert
        && i < lines.len()
    {
        let contents = format!(
            "{}{new_line}{eol}{}",
            lines[..i].concat(),
            lines[i..].concat()
        );
        return KeyEdit {
            contents,
            before: None,
            appended_newline: false,
            created_section: false,
        };
    }
    let mut contents = text.to_string();
    let appended_newline = !contents.is_empty() && !contents.ends_with('\n');
    if appended_newline {
        contents.push_str(eol);
    }
    let created_section = if let Insert::NewSection(name) = &insert {
        contents.push_str(&format!("[{name}]{eol}"));
        true
    } else {
        false
    };
    contents.push_str(&new_line);
    contents.push_str(eol);
    KeyEdit {
        contents,
        before: None,
        appended_newline,
        created_section,
    }
}

/// Put back the line a `set_key` replaced, or remove the one it added.
/// `written` is the line Irida wrote, and only a line with its value is
/// touched; an empty `written` (a v0.1 record) matches any line with the key.
/// `created_section` removes the header Irida added when nothing else is left
/// under it.
pub fn restore_key(
    current: &str,
    key: &str,
    written: &str,
    before: Option<&str>,
    appended_newline: bool,
    created_section: bool,
    opts: &KeyOpts,
) -> String {
    let lines: Vec<&str> = current.split_inclusive('\n').collect();
    let (range, _) = locate(&lines, &opts.scope);
    // Only the line Irida wrote: one the user has changed since is theirs.
    let Some(i) = find_line(&lines, range, key, Some(written), &opts.separator) else {
        return current.to_string();
    };
    if let Some(old) = before {
        let (_, eol) = strip_eol(lines[i]);
        return format!(
            "{}{old}{eol}{}",
            lines[..i].concat(),
            lines[i + 1..].concat()
        );
    }
    let mut kept: Vec<&str> = lines[..i].to_vec();
    kept.extend_from_slice(&lines[i + 1..]);
    if created_section
        && let KeyScope::Section(name) = &opts.scope
        && let Some(h) = kept
            .iter()
            .position(|l| header_name(l) == Some(name.as_str()))
    {
        let end = (h + 1..kept.len())
            .find(|&j| is_header(kept[j]))
            .unwrap_or(kept.len());
        if kept[h + 1..end].iter().all(|l| l.trim().is_empty()) {
            kept.drain(h..end);
        }
    }
    let mut out = kept.concat();
    if appended_newline && out.ends_with('\n') && lines[i + 1..].concat().is_empty() {
        pop_eol(&mut out);
    }
    out
}
