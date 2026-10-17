//! Target definitions: built-in and user folders, validation and detection.

use std::collections::BTreeMap;
use std::fmt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use include_dir::{Dir, include_dir};
use serde::Deserialize;

use crate::dirs::Dirs;

static BUILTIN: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/targets");

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Detect {
    pub binary: Option<String>,
    pub config: Option<String>,
    pub flatpak: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriteKind {
    File,
    Block,
    Key,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub path: String,
    pub write: WriteKind,
    pub template: String,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    /// Write only when the file does not exist yet.
    #[serde(default)]
    pub create_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum NativeKind {
    #[serde(rename = "cosmic-term")]
    CosmicTerm,
    #[serde(rename = "cosmic-desktop")]
    CosmicDesktop,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalReload {
    pub process: String,
    pub signal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinVersion {
    pub command: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Reload {
    pub signal: Option<SignalReload>,
    pub command: Option<String>,
    pub min_version: Option<MinVersion>,
    /// Restart advice shown when nothing can be reloaded.
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiceEntry {
    /// The repo path, itself a template.
    pub path: String,
    pub template: String,
    #[serde(default)]
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetDef {
    pub name: String,
    #[serde(default)]
    pub detect: Detect,
    #[serde(default)]
    pub native: Option<NativeKind>,
    #[serde(rename = "file", default)]
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub reload: Option<Reload>,
    #[serde(default)]
    pub rice: Vec<RiceEntry>,
    #[serde(default)]
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Builtin,
    User(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub def: TargetDef,
    pub templates: BTreeMap<String, String>,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    Parse(String),
    Invalid(String),
    Io(String),
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetError::Parse(m) => write!(f, "cannot parse target.toml: {m}"),
            TargetError::Invalid(m) => write!(f, "invalid target: {m}"),
            TargetError::Io(m) => write!(f, "cannot read target: {m}"),
        }
    }
}

impl std::error::Error for TargetError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTarget {
    pub name: String,
    pub result: Result<Target, TargetError>,
}

fn build(
    folder: &str,
    toml_text: &str,
    templates: BTreeMap<String, String>,
    source: Source,
) -> Result<Target, TargetError> {
    let def: TargetDef =
        toml::from_str(toml_text).map_err(|e| TargetError::Parse(e.to_string()))?;
    let invalid = |m: String| Err(TargetError::Invalid(m));
    if def.name != folder {
        return invalid(format!(
            "folder {folder:?} does not match name {:?}",
            def.name
        ));
    }
    if def.native.is_some() && source != Source::Builtin {
        return invalid("native is only allowed in built-in targets".into());
    }
    for f in &def.files {
        match f.write {
            WriteKind::Block if f.comment.is_none() => {
                return invalid(format!("{}: block needs comment", f.path));
            }
            WriteKind::Key if f.key.is_none() => {
                return invalid(format!("{}: key needs key", f.path));
            }
            _ => {}
        }
        if f.create_only && f.write != WriteKind::File {
            return invalid(format!("{}: create_only needs write = \"file\"", f.path));
        }
        if !templates.contains_key(&f.template) {
            return invalid(format!("{}: missing template {}", f.path, f.template));
        }
    }
    if let Some(sig) = def.reload.as_ref().and_then(|r| r.signal.as_ref())
        && !crate::reload::SIGNAL_NAMES.contains(&sig.signal.as_str())
    {
        return invalid(format!("unknown reload signal {:?}", sig.signal));
    }
    Ok(Target {
        def,
        templates,
        source,
    })
}

fn load_builtin(dir: &Dir<'_>) -> LoadedTarget {
    let name = dir
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut toml_text = None;
    let mut templates = BTreeMap::new();
    for file in dir.files() {
        let fname = file
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let (Some(fname), Some(text)) = (fname, file.contents_utf8()) else {
            continue;
        };
        if fname == "target.toml" {
            toml_text = Some(text.to_string());
        } else {
            templates.insert(fname, text.to_string());
        }
    }
    let result = match toml_text {
        Some(t) => build(&name, &t, templates, Source::Builtin),
        None => Err(TargetError::Io("target.toml is missing".into())),
    };
    LoadedTarget { name, result }
}

fn load_user_folder(path: &Path) -> LoadedTarget {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let result = (|| {
        let io = |e: std::io::Error| TargetError::Io(e.to_string());
        let toml_text = std::fs::read_to_string(path.join("target.toml")).map_err(io)?;
        let mut templates = BTreeMap::new();
        for entry in std::fs::read_dir(path).map_err(io)? {
            let entry = entry.map_err(io)?;
            let fname = entry.file_name().to_string_lossy().into_owned();
            if fname == "target.toml" || !entry.path().is_file() {
                continue;
            }
            let text = std::fs::read_to_string(entry.path()).map_err(io)?;
            templates.insert(fname, text);
        }
        build(
            &name,
            &toml_text,
            templates,
            Source::User(path.to_path_buf()),
        )
    })();
    LoadedTarget { name, result }
}

/// Combine built-in and user targets; a user target replaces a built-in of the same name.
fn merge(builtin: Vec<LoadedTarget>, user: Vec<LoadedTarget>) -> Vec<LoadedTarget> {
    let mut map: BTreeMap<String, LoadedTarget> =
        builtin.into_iter().map(|t| (t.name.clone(), t)).collect();
    for t in user {
        map.insert(t.name.clone(), t);
    }
    map.into_values().collect()
}

/// All targets, sorted by name. Failures are reported per target, never fatal.
pub fn load_targets(dirs: &Dirs) -> Vec<LoadedTarget> {
    let builtin: Vec<LoadedTarget> = BUILTIN.dirs().map(load_builtin).collect();
    let mut user = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dirs.config.join("irida/targets")) {
        for entry in rd.flatten() {
            if entry.path().is_dir() {
                user.push(load_user_folder(&entry.path()));
            }
        }
    }
    merge(builtin, user)
}

/// Whether `p` is a file with an execute bit.
pub fn is_executable(p: &Path) -> bool {
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Whether the application behind a target appears to be installed.
pub fn is_detected(def: &TargetDef, dirs: &Dirs, path: &[PathBuf]) -> bool {
    let d = &def.detect;
    if d.binary.is_none() && d.config.is_none() && d.flatpak.is_none() {
        return true;
    }
    if let Some(bin) = &d.binary
        && path.iter().any(|p| is_executable(&p.join(bin)))
    {
        return true;
    }
    if let Some(cfg) = &d.config
        && dirs.expand(cfg).exists()
    {
        return true;
    }
    if let Some(id) = &d.flatpak
        && dirs.home.join(".var/app").join(id).join("config").is_dir()
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lt(name: &str, source: Source) -> LoadedTarget {
        LoadedTarget {
            name: name.into(),
            result: Ok(Target {
                def: TargetDef {
                    name: name.into(),
                    detect: Detect::default(),
                    native: None,
                    files: vec![],
                    reload: None,
                    rice: vec![],
                    live: false,
                },
                templates: BTreeMap::new(),
                source,
            }),
        }
    }

    #[test]
    fn user_replaces_builtin() {
        let merged = merge(
            vec![lt("a", Source::Builtin), lt("b", Source::Builtin)],
            vec![lt("b", Source::User(PathBuf::from("/u/b")))],
        );
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].name, "a");
        let b = merged[1].result.as_ref().unwrap();
        assert!(matches!(b.source, Source::User(_)));
    }
}
