//! Export a palette as a complete `cosmic-<slug>-theme` rice repo.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use include_dir::{Dir, DirEntry, include_dir};

use crate::contrast::{self, ContrastReport};
use crate::dirs::Dirs;
use crate::export_docs;
use crate::native;
use crate::palette::Palette;
use crate::target::{LoadedTarget, Source, Target, load_targets};
use crate::template::{Renderer, check_syntax};
use crate::write::{atomic_write, replace_marked, resolve_write_path};

static RICE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/rice");

/// What a rice author still does by hand after an export.
pub const BY_HAND: &str = "wallpaper (backgrounds/), icons (icons/, icon_theme), cursor (cursors/), Firefox (firefox/), Claude Desktop (claude-desktop/), scripts/update_system.sh, LICENSE, screenshot (assets/screenshot.png)";

/// The name Irida's own files are planned under.
const IRIDA: &str = "irida";

#[derive(Debug, Clone)]
pub struct ExportRequest<'a> {
    pub palette: &'a Palette,
    /// The other half of a pair, shown in the README.
    pub pair: Option<&'a Palette>,
    /// The repo itself (`cosmic-<slug>-theme`) or the folder to create it in.
    pub folder: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportStatus {
    Created,
    Updated,
    Unchanged,
    /// A create-only file that already exists and was left alone.
    Kept,
    Skipped(String),
    /// A generated-looking file this export did not write; never deleted.
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportLine {
    pub path: String,
    pub status: ExportStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportReport {
    pub repo: PathBuf,
    pub lines: Vec<ExportLine>,
    pub notes: Vec<String>,
    pub contrast: ContrastReport,
}

#[derive(Debug)]
pub enum ExportError {
    WrongFolder {
        folder: PathBuf,
        slug: String,
        name: String,
    },
    Render {
        target: String,
        message: String,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::WrongFolder { folder, slug, name } => write!(
                f,
                "{} belongs to another rice; export {name} into a folder named cosmic-{slug}-theme or into its parent",
                folder.display()
            ),
            ExportError::Render { target, message } => {
                write!(f, "{target}: {message}; nothing was written")
            }
            ExportError::Io { path, source } => {
                write!(f, "cannot write {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ExportError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// One file the export will write, relative to the repo.
pub(crate) struct Planned {
    pub path: String,
    pub contents: String,
    pub executable: bool,
    /// Written only when the file does not exist yet.
    pub create_only: bool,
}

/// Every planned file and skipped entry, in source order; the first source of a path wins.
#[derive(Default)]
pub(crate) struct Plan {
    files: Vec<Planned>,
    owners: BTreeMap<String, String>,
    reserved: Vec<String>,
    skipped: Vec<ExportLine>,
}

impl Plan {
    fn skip(&mut self, path: &str, reason: impl Into<String>) {
        self.skipped.push(ExportLine {
            path: path.to_string(),
            status: ExportStatus::Skipped(reason.into()),
        });
    }

    /// Add a file from `source`, or skip it when an earlier source has the path.
    pub(crate) fn add(&mut self, source: &str, file: Planned) {
        if let Some(first) = self.owners.get(&file.path) {
            let reason = format!("already written by {first}");
            self.skip(&file.path, reason);
            return;
        }
        self.owners.insert(file.path.clone(), source.to_string());
        self.files.push(file);
    }

    /// Skip `path` and keep later sources from writing it.
    fn reserve(&mut self, path: &str, reason: &str) {
        self.owners.insert(path.to_string(), IRIDA.to_string());
        self.reserved.push(path.to_string());
        self.skip(path, reason);
    }

    pub(crate) fn has(&self, path: &str) -> bool {
        self.owners.contains_key(path)
    }
}

/// Whether a rendered repo path stays inside the repo: relative, with no
/// empty, `.` or `..` component. Rejecting `.` keeps every file to one
/// spelling, so `./README.md` cannot get past the owner of `README.md`.
fn stays_inside(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

/// Plan one target's `[[rice]]` entries.
fn plan_rice(plan: &mut Plan, target: &Target, renderer: &Renderer) -> Result<(), ExportError> {
    let name = &target.def.name;
    let render_error = |message: String| ExportError::Render {
        target: name.clone(),
        message,
    };
    for entry in &target.def.rice {
        let path = renderer
            .render(&format!("{name} rice path"), &entry.path)
            .map_err(|e| render_error(e.to_string()))?;
        if !stays_inside(&path) {
            plan.skip(&path, "path leaves the repo");
            continue;
        }
        let Some(source) = target.templates.get(&entry.template) else {
            plan.skip(&path, format!("missing template {}", entry.template));
            continue;
        };
        if let Err(e) = check_syntax(&entry.template, source) {
            plan.skip(&path, e.to_string());
            continue;
        }
        let contents = renderer
            .render(&entry.template, source)
            .map_err(|e| render_error(e.to_string()))?;
        plan.add(
            name,
            Planned {
                path,
                contents,
                executable: entry.executable,
                create_only: false,
            },
        );
    }
    Ok(())
}

/// The embedded file at `path`, as text.
pub(crate) fn rice_asset(path: &str) -> &'static str {
    RICE.get_file(path)
        .and_then(|f| f.contents_utf8())
        .unwrap_or_else(|| panic!("embedded rice asset {path}"))
}

/// Every embedded file below `dir`, recursively.
fn walk<'a>(dir: &'a Dir<'a>, out: &mut Vec<&'a include_dir::File<'a>>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(d) => walk(d, out),
            DirEntry::File(f) => out.push(f),
        }
    }
}

/// Irida's own files: the COSMIC installer, the `cosmic/config/` baseline and the dotfiles.
fn plan_assets(plan: &mut Plan, renderer: &Renderer) -> Result<(), ExportError> {
    let template = "cosmic/install.sh.tmpl";
    let contents = renderer
        .render(template, rice_asset(template))
        .map_err(|e| ExportError::Render {
            target: IRIDA.into(),
            message: e.to_string(),
        })?;
    plan.add(
        IRIDA,
        Planned {
            path: "cosmic/install.sh".into(),
            contents,
            executable: true,
            create_only: false,
        },
    );

    let mut baseline = Vec::new();
    if let Some(dir) = RICE.get_dir("cosmic/config") {
        walk(dir, &mut baseline);
    }
    baseline.sort_by_key(|f| f.path());
    let assets = baseline
        .into_iter()
        .map(|f| (f.path().to_string_lossy().into_owned(), f))
        .chain(
            [
                (".gitignore", "gitignore"),
                (".pre-commit-config.yaml", "pre-commit-config.yaml"),
            ]
            .into_iter()
            .filter_map(|(to, from)| RICE.get_file(from).map(|f| (to.to_string(), f))),
        );
    for (path, file) in assets {
        plan.add(
            IRIDA,
            Planned {
                path,
                contents: file.contents_utf8().unwrap_or_default().to_string(),
                executable: false,
                create_only: true,
            },
        );
    }
    Ok(())
}

/// The repo folder for `req`: the folder itself when it is named for this
/// palette, an error when it is named for another rice, else a child of it.
fn repo_folder(req: &ExportRequest) -> Result<PathBuf, ExportError> {
    let p = req.palette;
    let folder = req.folder;
    let dotted = matches!(
        folder.components().next_back(),
        Some(Component::CurDir | Component::ParentDir)
    ) || folder.to_string_lossy().ends_with("/.");
    let named = if dotted {
        folder.canonicalize().map_err(|source| ExportError::Io {
            path: folder.to_path_buf(),
            source,
        })?
    } else {
        folder.to_path_buf()
    };
    let name = named
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mine = format!("cosmic-{}-theme", p.slug);
    if name == mine {
        return Ok(named);
    }
    let other = name
        .strip_prefix("cosmic-")
        .and_then(|n| n.strip_suffix("-theme"))
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase()));
    if other {
        return Err(ExportError::WrongFolder {
            folder: named,
            slug: p.slug.clone(),
            name: p.name.clone(),
        });
    }
    Ok(named.join(mine))
}

/// Where `rel` lands when written, when that is inside `canonical_repo`.
fn inside(repo: &Path, canonical_repo: &Path, rel: &str) -> Option<PathBuf> {
    let resolved = resolve_write_path(&repo.join(rel)).ok()?;
    let mut ancestor = resolved.as_path();
    let mut rest = Vec::new();
    while !ancestor.exists() {
        rest.push(ancestor.file_name()?.to_os_string());
        ancestor = ancestor.parent()?;
    }
    let mut real = ancestor.canonicalize().ok()?;
    for part in rest.iter().rev() {
        if part == ".." || part == "." {
            return None;
        }
        real.push(part);
    }
    real.starts_with(canonical_repo).then_some(real)
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> ExportError + '_ {
    move |source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Write one planned file and say what happened to it.
fn write_planned(
    repo: &Path,
    canonical_repo: &Path,
    f: &Planned,
) -> Result<ExportStatus, ExportError> {
    let Some(real) = inside(repo, canonical_repo, &f.path) else {
        return Ok(ExportStatus::Skipped("path leaves the repo".into()));
    };
    let existing = match fs::read(&real) {
        Ok(bytes) => Some(bytes),
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound) => None,
        Err(e) => return Err(io_error(&real)(e)),
    };
    let status = match &existing {
        Some(_) if f.create_only => return Ok(ExportStatus::Kept),
        Some(bytes) if bytes == f.contents.as_bytes() => ExportStatus::Unchanged,
        Some(_) => ExportStatus::Updated,
        None => ExportStatus::Created,
    };
    if status != ExportStatus::Unchanged {
        atomic_write(&real, f.contents.as_bytes()).map_err(io_error(&real))?;
    }
    if f.executable {
        fs::set_permissions(&real, fs::Permissions::from_mode(0o755)).map_err(io_error(&real))?;
    }
    Ok(status)
}

/// Generated-looking files that exist in the repo.
fn generated_files(repo: &Path) -> Vec<String> {
    let patterns: [(&str, &str); 6] = [
        ("cosmic", ".ron"),
        ("cosmic-term", "-term.ron"),
        ("btop", ".theme"),
        ("nvim/colors", ".lua"),
        ("nvim/lua/plugins", ".lua"),
        ("zsh", ".zsh"),
    ];
    let mut out = Vec::new();
    for (dir, suffix) in patterns {
        let Ok(rd) = fs::read_dir(repo.join(dir)) else {
            continue;
        };
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(suffix) && entry.path().is_file() {
                out.push(format!("{dir}/{name}"));
            }
        }
    }
    out
}

/// Plan every file of the repo before anything is written.
fn plan_export(dirs: &Dirs, repo: &Path, req: &ExportRequest) -> Result<Plan, ExportError> {
    let p = req.palette;
    let renderer = Renderer::for_export(p);
    let loaded = load_targets(dirs);
    let mut plan = Plan::default();

    let builtin = |lt: &&LoadedTarget| {
        lt.result
            .as_ref()
            .is_ok_and(|t| t.source == Source::Builtin)
    };
    for lt in &loaded {
        if let Err(e) = &lt.result {
            plan.skip(&format!("target {}", lt.name), e.to_string());
        }
    }
    for lt in loaded.iter().filter(builtin) {
        let target = lt.result.as_ref().expect("filtered on Ok");
        if let Some(kind) = target.def.native {
            for f in native::export(kind, p) {
                plan.add(
                    &target.def.name,
                    Planned {
                        path: f.path,
                        contents: f.contents,
                        executable: false,
                        create_only: false,
                    },
                );
            }
        }
    }
    for lt in loaded.iter().filter(builtin) {
        plan_rice(
            &mut plan,
            lt.result.as_ref().expect("filtered on Ok"),
            &renderer,
        )?;
    }
    plan_assets(&mut plan, &renderer)?;
    plan_docs(&mut plan, repo, req, &renderer)?;
    for lt in &loaded {
        if let Ok(target) = &lt.result
            && target.source != Source::Builtin
        {
            plan_rice(&mut plan, target, &renderer)?;
        }
    }
    Ok(plan)
}

/// The text of `rel` in the repo, `None` when it does not exist yet.
fn read_existing(repo: &Path, rel: &str) -> Result<Option<String>, ExportError> {
    let Ok(canonical_repo) = repo.canonicalize() else {
        return Ok(None);
    };
    let Some(real) = inside(repo, &canonical_repo, rel) else {
        return Ok(None);
    };
    match fs::read_to_string(&real) {
        Ok(text) => Ok(Some(text)),
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound) => Ok(None),
        Err(e) => Err(io_error(&real)(e)),
    }
}

/// Plan a marked Markdown file: the skeleton when absent, else only the block replaced.
fn plan_marked(
    plan: &mut Plan,
    repo: &Path,
    rel: &str,
    (begin, end): (&str, &str),
    block: &str,
    skeleton: impl FnOnce() -> String,
) -> Result<(), ExportError> {
    let contents = match read_existing(repo, rel)? {
        None => skeleton(),
        Some(text) => match replace_marked(&text, begin, end, &export_docs::marked_body(block)) {
            Ok(Some(updated)) => updated,
            Ok(None) => {
                plan.reserve(rel, "no irida markers");
                return Ok(());
            }
            Err(_) => {
                plan.reserve(rel, "irida markers are broken");
                return Ok(());
            }
        },
    };
    plan.add(
        IRIDA,
        Planned {
            path: rel.into(),
            contents,
            executable: false,
            create_only: false,
        },
    );
    Ok(())
}

/// The README, showcase and `scripts/switch.sh`.
fn plan_docs(
    plan: &mut Plan,
    repo: &Path,
    req: &ExportRequest,
    renderer: &Renderer,
) -> Result<(), ExportError> {
    let p = req.palette;
    let readme = export_docs::readme_block(p, req.pair);
    plan_marked(
        plan,
        repo,
        "README.md",
        (export_docs::README_BEGIN, export_docs::README_END),
        &readme,
        || export_docs::readme_skeleton(p, &readme),
    )?;
    let showcase = export_docs::showcase_block(p);
    plan_marked(
        plan,
        repo,
        "docs/showcase.md",
        (export_docs::SHOWCASE_BEGIN, export_docs::SHOWCASE_END),
        &showcase,
        || export_docs::showcase_skeleton(p, req.pair, &showcase),
    )?;

    let template = "scripts/switch.sh.tmpl";
    let contents = renderer
        .render_with(
            template,
            rice_asset(template),
            &[
                ("settings", export_docs::switch_settings(p)),
                ("rices", export_docs::rices_list(&p.slug)),
            ],
        )
        .map_err(|e| ExportError::Render {
            target: IRIDA.into(),
            message: e.to_string(),
        })?;
    plan.add(
        IRIDA,
        Planned {
            path: "scripts/switch.sh".into(),
            contents,
            executable: true,
            create_only: false,
        },
    );
    Ok(())
}

pub fn export(dirs: &Dirs, req: &ExportRequest) -> Result<ExportReport, ExportError> {
    let p = req.palette;
    let repo = repo_folder(req)?;
    let plan = plan_export(dirs, &repo, req)?;

    fs::create_dir_all(&repo).map_err(io_error(&repo))?;
    let canonical_repo = repo.canonicalize().map_err(io_error(&repo))?;
    let stale: Vec<String> = generated_files(&repo)
        .into_iter()
        .filter(|path| !plan.has(path))
        .collect();

    // The first source to reach a real file owns it however the path is
    // spelled, so a symlinked folder in the repo cannot route a later source
    // onto a file Irida or an earlier target already wrote or reserved.
    let mut claimed: BTreeMap<PathBuf, &str> = BTreeMap::new();
    for path in &plan.reserved {
        if let Some(real) = inside(&repo, &canonical_repo, path) {
            claimed.insert(real, IRIDA);
        }
    }
    let mut lines = plan.skipped.clone();
    for f in &plan.files {
        let owner = plan.owners.get(&f.path).map_or(IRIDA, String::as_str);
        let real = inside(&repo, &canonical_repo, &f.path);
        let status = match real.as_ref().and_then(|r| claimed.get(r)) {
            Some(first) => ExportStatus::Skipped(format!("already written by {first}")),
            None => {
                if let Some(real) = real {
                    claimed.insert(real, owner);
                }
                write_planned(&repo, &canonical_repo, f)?
            }
        };
        lines.push(ExportLine {
            path: f.path.clone(),
            status,
        });
    }
    lines.extend(stale.into_iter().map(|path| ExportLine {
        path,
        status: ExportStatus::Stale,
    }));
    lines.sort_by(|a, b| a.path.cmp(&b.path));

    let mut notes = Vec::new();
    let ron = format!("cosmic/{}.ron", p.stem());
    if lines
        .iter()
        .any(|l| l.path == ron && l.status == ExportStatus::Updated)
    {
        notes.push(format!(
            "{ron}: rewritten from the palette; hand-tuned radii, gaps, frosting and colours in the old file are replaced (git diff shows them)"
        ));
    }
    Ok(ExportReport {
        repo,
        lines,
        notes,
        contrast: contrast::check(p),
    })
}
