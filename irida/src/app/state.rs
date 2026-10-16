//! The app's state machine. Every app behaviour that has logic is decided
//! here, in `update`; IO happens in `effects::perform`, and the views only
//! read `State` and emit `Msg`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use cosmic::cosmic_theme::palette::{FromColor, Hsv, Srgb};
use cosmic::iced::Color;
use cosmic::widget::color_picker::{ColorPickerModel, ColorPickerUpdate};
use irida_core::apply::{ApplyReport, Ctx, TargetInfo, TargetStatus};
use irida_core::color::Rgb;
use irida_core::contrast::{RoleContrast, check};
use irida_core::derive::derive;
use irida_core::draft::{Cluster, draft, draft_palette};
use irida_core::export::ExportLine;
use irida_core::live::LiveSession;
use irida_core::palette::{Mode, Palette, Roles, slug_from_name, validate_name};
use irida_core::reload::ReloadStatus;
use irida_core::role::Role;
use irida_core::slot::{Ansi, PinValue, Slot, Tone};
use irida_core::store::{PairLookup, SNAPSHOT_NAME_MAX, Settings, VersionInfo, starters};
use irida_core::wallpaper::name_from_file;

/// One stored palette as the Switch list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySummary {
    pub slug: String,
    /// The palette's name and mode, or why it cannot be read.
    pub palette: Result<(String, Mode), String>,
    /// The slug its `pair` names, when it can be read.
    pub pair: Option<String>,
}

impl EntrySummary {
    pub fn readable(&self) -> bool {
        self.palette.is_ok()
    }

    /// The palette's name, or its slug when it cannot be read.
    pub fn label(&self) -> &str {
        match &self.palette {
            Ok((name, _)) => name,
            Err(_) => &self.slug,
        }
    }
}

/// The open palette.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::large_enum_variant,
    reason = "one value, held in State; boxing would only add noise"
)]
pub enum Current {
    /// `saved` is `None` until the first save (a wallpaper draft).
    Editable {
        slug: String,
        saved: Option<Palette>,
        working: Palette,
    },
    /// A palette file that cannot be read: shown with its error and text,
    /// never written.
    Unreadable {
        slug: String,
        error: String,
        text: String,
    },
}

impl Current {
    pub fn slug(&self) -> &str {
        match self {
            Current::Editable { slug, .. } | Current::Unreadable { slug, .. } => slug,
        }
    }

    /// Unsaved changes: the working copy differs from the last save.
    pub fn dirty(&self) -> bool {
        match self {
            Current::Editable { saved, working, .. } => saved.as_ref() != Some(working),
            Current::Unreadable { .. } => false,
        }
    }

    pub fn working(&self) -> Option<&Palette> {
        match self {
            Current::Editable { working, .. } => Some(working),
            Current::Unreadable { .. } => None,
        }
    }
}

/// A toast, with an optional action button that sends `Msg`.
#[derive(Debug, Clone)]
pub struct ToastItem {
    pub text: String,
    pub action: Option<(String, Msg)>,
}

/// What the name dialog is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFor {
    New,
    Duplicate,
}

/// Something that leaves the open palette, held while the unsaved-changes
/// question is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leave {
    Open(String),
    Create(Palette),
    Duplicate {
        from: String,
        name: String,
    },
    /// Open a wallpaper draft, unsaved.
    Draft(Palette),
    /// Replace the open palette with an import, then open it.
    Import {
        palette: Box<Palette>,
        notes: Vec<String>,
    },
    Exit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    /// The list of stored palettes.
    Switch,
    /// A name for a new palette or a copy; `mode` is used by New only.
    Name {
        purpose: NameFor,
        name: String,
        mode: Mode,
    },
    /// Delete the open palette, named by `label`.
    Delete { slug: String, label: String },
    /// Save, Discard or Cancel before `Leave` happens.
    Unsaved(Leave),
    /// The targets Apply writes, with a toggle each.
    Apply(ApplySheet),
    /// An imported palette whose slug is taken: Replace or Keep both.
    /// `replace_ok` is false when the palette there cannot be read, or an
    /// open unsaved draft has the slug.
    ImportCollision {
        palette: Palette,
        notes: Vec<String>,
        replace_ok: bool,
    },
    /// What an export wrote.
    Exported(ExportSummary),
    /// New from wallpaper.
    Wallpaper(WallpaperDialog),
    /// Pair with...: the palettes of the other mode. `confirm` is a chosen
    /// palette that is paired already, waiting for a yes.
    PairWith {
        choices: Vec<PairChoice>,
        confirm: Option<String>,
    },
    /// A snapshot name for saved version `n`.
    Snapshot { n: u32, name: String },
    /// Keep or Revert the live session, then do `then`. `pending` is the
    /// session a previous run left open, which has no Cancel.
    KeepOrRevert {
        then: Option<LiveThen>,
        pending: bool,
    },
}

/// What waits for the live session to end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveThen {
    /// Show the Switch list.
    Switch,
    /// Ask to delete the open palette.
    Delete,
    /// Open the Apply sheet.
    Apply,
    /// Leave the open palette (open another, create, close).
    Leave(Leave),
}

/// How a live session ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveEnd {
    Keep,
    Revert,
}

/// An open try-it-live session, as the app drives it. Applies run one at a
/// time: an edit made while one runs waits, and the newest runs next.
#[derive(Debug, Clone)]
pub struct Live {
    /// `None` while the session is starting.
    session: Option<LiveSession>,
    /// The palette it applies.
    slug: String,
    /// The text-field edit the next tick applies; older ticks are stale.
    generation: u64,
    /// A start, an apply or the end is running.
    busy: bool,
    /// An edit waits for the running apply.
    queued: bool,
    /// The session a previous run left open: offered Keep or Revert, never
    /// applied to.
    from_last_run: bool,
    /// What the last apply sent, so an unchanged copy is not sent again.
    applied: Option<Palette>,
    /// Keep or Revert, chosen; runs once nothing else is running.
    end: Option<(LiveEnd, Option<LiveThen>)>,
}

impl Live {
    fn new(slug: String, session: Option<LiveSession>, from_last_run: bool) -> Live {
        Live {
            busy: session.is_none(),
            session,
            slug,
            generation: 0,
            queued: false,
            from_last_run,
            applied: None,
            end: None,
        }
    }
}

/// A palette the open one can be paired with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairChoice {
    pub slug: String,
    pub name: String,
    /// The name of the palette it is paired with now.
    pub paired_with: Option<String>,
}

impl PairChoice {
    /// "Dossier", or "Dossier, paired with Noir".
    pub fn label(&self) -> String {
        match &self.paired_with {
            Some(other) => format!("{}, paired with {other}", self.name),
            None => self.name.clone(),
        }
    }
}

/// The text a failed draft shows for a file that is not an image.
pub const NOT_IMAGE: &str = "This file could not be read as an image. Try a JPEG, PNG or WebP.";

/// What the New from wallpaper dialog shows when COSMIC's wallpaper is not
/// an image file Irida can find (a colour, a gradient, a missing file).
pub const NO_WALLPAPER: &str =
    "Your wallpaper is not an image file Irida can find. Choose an image to draft from.";

/// New from wallpaper: the image, the colours found in it, and the draft
/// for the chosen mode.
#[derive(Debug, Clone, PartialEq)]
pub struct WallpaperDialog {
    /// Looking up COSMIC's current wallpaper.
    pub finding: bool,
    /// The image being read; a result for any other path is stale.
    pub reading: Option<PathBuf>,
    /// The image the draft below was made from.
    pub image: Option<PathBuf>,
    /// Its colours, heaviest first (at most eight).
    pub clusters: Vec<Cluster>,
    pub mode: Mode,
    /// The draft's roles for `mode`, once an image was read.
    pub roles: Option<Roles>,
    /// The cluster the accent came from.
    pub accent_from: Option<usize>,
    pub name: String,
    /// The user typed a name, so a new image keeps it.
    pub name_edited: bool,
    /// Why the last image could not be used.
    pub error: Option<String>,
}

impl WallpaperDialog {
    fn new() -> WallpaperDialog {
        WallpaperDialog {
            finding: true,
            reading: None,
            image: None,
            clusters: Vec::new(),
            mode: Mode::Dark,
            roles: None,
            accent_from: None,
            name: String::new(),
            name_edited: false,
            error: None,
        }
    }

    /// Draft the roles again for `mode`.
    fn redraft(&mut self, mode: Mode) {
        let d = draft(&self.clusters, mode);
        self.mode = mode;
        self.roles = Some(d.roles);
        self.accent_from = d.accent_from;
    }

    /// Start reading `path`; the effect that does it.
    fn read(&mut self, path: PathBuf) -> Effect {
        self.reading = Some(path.clone());
        Effect::DraftImage(path)
    }
}

/// What a folder is chosen for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderFor {
    Import,
    Export,
}

impl FolderFor {
    /// The file chooser's title.
    pub fn title(self) -> &'static str {
        match self {
            FolderFor::Import => "Import a rice repo",
            FolderFor::Export => "Export as rice repo",
        }
    }
}

/// The Apply sheet: every target, and what is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplySheet {
    pub targets: Vec<TargetInfo>,
    /// The other half of a pair, applied with the palette.
    pub pair: Option<Palette>,
    /// What is applied, like "Applies Ember and Ember Day".
    pub applies: String,
    /// Why a named pair is left out.
    pub pair_note: Option<String>,
}

/// The Apply sheet's targets in its three groups, each sorted by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetGroups {
    /// Installed, working targets with whether they are turned on.
    pub toggles: Vec<(String, bool)>,
    /// Installed targets that cannot be applied, with the error.
    pub broken: Vec<(String, String)>,
    pub not_installed: Vec<String>,
}

impl SheetGroups {
    /// "Not installed: a, b", or `None` when every target is installed.
    pub fn not_installed_line(&self) -> Option<String> {
        (!self.not_installed.is_empty())
            .then(|| format!("Not installed: {}", self.not_installed.join(", ")))
    }
}

/// Group `targets` for the sheet; whether a target is on comes from
/// `settings`, which a toggle changes at once.
pub fn sheet_groups(targets: &[TargetInfo], settings: &Settings) -> SheetGroups {
    let mut groups = SheetGroups {
        toggles: Vec::new(),
        broken: Vec::new(),
        not_installed: Vec::new(),
    };
    for t in targets {
        if !t.detected {
            groups.not_installed.push(t.name.clone());
        } else if let Some(e) = &t.broken {
            groups.broken.push((t.name.clone(), e.clone()));
        } else {
            let on = !settings.disabled.contains(&t.name);
            groups.toggles.push((t.name.clone(), on));
        }
    }
    groups
}

/// Why an apply failed, in a form messages can carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyFailure {
    Write {
        path: PathBuf,
        reason: String,
        rolled_back: bool,
    },
    Render {
        target: String,
        message: String,
    },
    NothingToApply,
    Other(String),
}

impl ApplyFailure {
    /// The toast for this failure.
    pub fn text(&self) -> String {
        match self {
            ApplyFailure::Write {
                path,
                reason,
                rolled_back: true,
            } => format!(
                "Could not write {}: {reason}. Nothing was changed.",
                path.display()
            ),
            ApplyFailure::Write {
                path,
                reason,
                rolled_back: false,
            } => format!(
                "Could not write {}: {reason}; run Undo last apply to finish.",
                path.display()
            ),
            ApplyFailure::Render { target, message } => {
                format!("{target}: {message}. Nothing was changed.")
            }
            ApplyFailure::NothingToApply => {
                "Nothing to apply: turn on an installed app.".to_string()
            }
            ApplyFailure::Other(e) => format!("Could not apply: {e}"),
        }
    }

    /// Undo is offered when a failed rollback left files to put back.
    fn needs_undo(&self) -> bool {
        matches!(
            self,
            ApplyFailure::Write {
                rolled_back: false,
                ..
            }
        )
    }
}

/// The toast after an apply: how many apps, one line per app that needs a
/// restart or failed to reload, then the warnings.
pub fn applied_text(report: &ApplyReport) -> String {
    let applied = report
        .targets
        .iter()
        .filter(|t| matches!(t.status, TargetStatus::Applied(_)))
        .count();
    let mut lines = vec![match applied {
        1 => "Applied to 1 app".to_string(),
        n => format!("Applied to {n} apps"),
    }];
    for t in &report.targets {
        match &t.status {
            TargetStatus::Applied(ReloadStatus::Restart(hint)) => {
                lines.push(format!("{}: {hint}", t.name));
            }
            TargetStatus::Applied(ReloadStatus::Failed(reason)) => lines.push(format!(
                "Could not reload {} ({reason}); restart it to see the colours.",
                t.name
            )),
            _ => {}
        }
    }
    for t in &report.targets {
        lines.extend(t.warnings.iter().cloned());
    }
    lines.join("\n")
}

/// What an undo did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoDone {
    pub record_id: u32,
    /// One line per file that could not be restored.
    pub failures: Vec<String>,
}

/// What an export wrote, and the text the result dialog shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSummary {
    pub lines: Vec<ExportLine>,
    pub text: String,
}

/// The number words Keep both appends.
const KEEP_BOTH: [&str; 9] = [
    "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
];

/// `"<name> Two"` up to `"<name> Ten"`: the first whose slug is not
/// `taken`.
pub fn keep_both_name(name: &str, taken: impl Fn(&str) -> bool) -> Option<String> {
    KEEP_BOTH
        .iter()
        .map(|w| format!("{name} {w}"))
        .find(|n| !taken(&slug_from_name(n)))
}

/// The palette menu's items, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Switch,
    New,
    NewFromWallpaper,
    Duplicate,
    Unpair,
    Import,
    Export,
    UndoLastApply,
    Delete,
}

impl MenuItem {
    pub const ALL: [MenuItem; 9] = [
        MenuItem::Switch,
        MenuItem::New,
        MenuItem::NewFromWallpaper,
        MenuItem::Duplicate,
        MenuItem::Unpair,
        MenuItem::Import,
        MenuItem::Export,
        MenuItem::UndoLastApply,
        MenuItem::Delete,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Switch => "Switch",
            MenuItem::New => "New",
            MenuItem::NewFromWallpaper => "New from wallpaper",
            MenuItem::Duplicate => "Duplicate",
            MenuItem::Unpair => "Unpair",
            MenuItem::Import => "Import",
            MenuItem::Export => "Export as rice repo",
            MenuItem::UndoLastApply => "Undo last apply",
            MenuItem::Delete => "Delete",
        }
    }

    pub fn msg(self) -> Msg {
        match self {
            MenuItem::Switch => Msg::ShowSwitch,
            MenuItem::New => Msg::ShowNew,
            MenuItem::Duplicate => Msg::ShowDuplicate,
            MenuItem::Unpair => Msg::Unpair,
            MenuItem::Delete => Msg::ShowDelete,
            MenuItem::Import => Msg::ShowImport,
            MenuItem::Export => Msg::ShowExport,
            MenuItem::UndoLastApply => Msg::UndoLastApply,
            MenuItem::NewFromWallpaper => Msg::ShowWallpaper,
        }
    }
}

/// The header's buttons beside the palette menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderItem {
    Mode,
    Pair,
    Compare,
    TryLive,
    Apply,
}

/// The saved version Compare shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionRef {
    /// The last saved version, following each save.
    Latest,
    Version(u32),
}

/// How the preview is split; only one split at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Off,
    /// The open palette beside its pair.
    Pair,
    /// A saved version beside the working copy.
    Saved(VersionRef),
}

/// One column of a split preview.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitColumn {
    pub label: String,
    /// `None` while it is read, or when it cannot be (`note` says which).
    pub palette: Option<Palette>,
    pub note: Option<String>,
}

/// The left column's version list in Compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionChoice {
    /// Newest first, like "v3 Warm accent" and "v2".
    pub labels: Vec<String>,
    /// The shown version's place in `labels`.
    pub selected: Option<usize>,
    /// The shown version's snapshot name.
    pub snapshot: Option<String>,
}

/// What a split preview shows.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitView {
    pub left: SplitColumn,
    pub right: SplitColumn,
    /// Compare's version list; `None` for the pair.
    pub versions: Option<VersionChoice>,
}

/// "v3 Warm accent", or "v2" for a version without a snapshot name.
pub fn version_label(v: &VersionInfo) -> String {
    match &v.snapshot {
        Some(name) => format!("v{} {name}", v.n),
        None => format!("v{}", v.n),
    }
}

/// A column label like "Ember (dark)".
fn half_label(p: &Palette) -> String {
    format!("{} ({})", p.name, p.mode)
}

/// What the Compare chip's tooltip says while it is disabled.
pub const COMPARE_NEEDS_VERSION: &str = "Save a version first (Ctrl+S)";

/// How long after the last keystroke a text field's edit is applied live.
pub const LIVE_DELAY: Duration = Duration::from_millis(400);

/// The try-live toggle's tooltip for a draft that was never saved.
pub const LIVE_NEEDS_SAVE: &str = "Save the palette first";

/// The try-live toggle's tooltip when no target reloads live.
pub const LIVE_NEEDS_TARGET: &str = "Try it live needs COSMIC, COSMIC Terminal or cava";

/// The try-live toggle's tooltip while another Irida window is open.
pub const LIVE_IN_OTHER_WINDOW: &str = "Irida is open in another window: use Try it live there";

/// Undo during a live session.
pub const LIVE_BLOCKS_UNDO: &str = "Turn off Try it live first: Keep or Revert, then undo.";

/// The error a hex field shows for anything but six hex digits.
pub const HEX_ERROR: &str = "Use six hex digits, like #E8713A";

/// A text field in the role editor whose text can differ from the colour
/// it edits (while typing, or after invalid input).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    /// The expanded role's hex field.
    Hex(Role),
    /// A pinned slot's hex field.
    PinHex(Slot),
}

/// What the user typed in a `Field`, and why it was not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldEdit {
    pub text: String,
    pub error: Option<String>,
}

/// One row of the role list.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleRow {
    pub role: Role,
    /// The role's `[names]` entry, else the capitalised role.
    pub name: String,
    pub color: Rgb,
    /// Both ratios and the minimum; `None` for surfaces.
    pub contrast: Option<RoleContrast>,
}

impl RoleRow {
    /// The lower of the two ratios, to one decimal, like `4.5:1`.
    pub fn ratio_label(&self) -> Option<String> {
        self.contrast
            .as_ref()
            .map(|c| ratio_text(c.on_background.min(c.on_pane)))
    }

    /// Under the role's minimum on either surface (the raw ratio decides).
    pub fn fails(&self) -> bool {
        self.contrast.as_ref().is_some_and(|c| !c.passes())
    }
}

/// A ratio to one decimal, like `4.5:1`.
pub fn ratio_text(r: f64) -> String {
    format!("{r:.1}:1")
}

/// `accent` gives `Accent`.
pub fn capitalised(role: Role) -> String {
    let s = role.as_str();
    let mut out = s[..1].to_ascii_uppercase();
    out.push_str(&s[1..]);
    out
}

/// The name the role list shows for `role`.
pub fn display_name(p: &Palette, role: Role) -> String {
    p.names
        .get(&role)
        .filter(|n| !n.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| capitalised(role))
}

/// The role list, in `Role::ALL` order.
pub fn role_rows(p: &Palette) -> Vec<RoleRow> {
    let report = check(p);
    Role::ALL
        .into_iter()
        .map(|role| RoleRow {
            role,
            name: display_name(p, role),
            color: p.roles.get(role),
            contrast: report.rows.iter().find(|r| r.role == role).cloned(),
        })
        .collect()
}

/// The slots a role's row shows, following `derive`: every pinnable slot
/// appears under exactly one role in each mode.
pub fn role_slots(role: Role, mode: Mode) -> Vec<Slot> {
    let dark = mode == Mode::Dark;
    let tones = |a: Ansi| {
        [Tone::Normal, Tone::Bright, Tone::Dim]
            .into_iter()
            .map(move |t| Slot::Ansi(t, a))
    };
    let hue = |a: Ansi| tones(a).collect::<Vec<_>>();
    match role {
        Role::Error => hue(Ansi::Red),
        Role::Success => hue(Ansi::Green),
        Role::Warning => hue(Ansi::Yellow),
        Role::Blue => hue(Ansi::Blue),
        Role::Magenta => hue(Ansi::Magenta),
        Role::Cyan => hue(Ansi::Cyan),
        Role::Text => [
            Slot::Foreground,
            Slot::BrightForeground,
            Slot::CosmicNeutral10,
        ]
        .into_iter()
        .chain(tones(if dark { Ansi::White } else { Ansi::Black }))
        .collect(),
        Role::Muted if dark => vec![Slot::DimForeground, Slot::Ansi(Tone::Bright, Ansi::Black)],
        Role::Muted => std::iter::once(Slot::DimForeground)
            .chain(tones(Ansi::White))
            .collect(),
        Role::Container if dark => vec![
            Slot::Ansi(Tone::Normal, Ansi::Black),
            Slot::Ansi(Tone::Dim, Ansi::Black),
            Slot::CosmicGray2,
        ],
        Role::Pane if dark => vec![Slot::Background],
        Role::Pane => vec![Slot::Background, Slot::CosmicGray2],
        Role::Accent => vec![Slot::Cursor],
        Role::Container | Role::Background | Role::Comment => Vec::new(),
    }
}

/// An iced colour as `Rgb`, rounded to the nearest 8-bit channel.
pub fn rgb_from_color(c: Color) -> Rgb {
    let [r, g, b, _] = c.into_rgba8();
    Rgb { r, g, b }
}

fn rgb_from_hsv(hsv: Hsv) -> Rgb {
    rgb_from_color(Color::from(Srgb::from_color(hsv)))
}

/// A colour picker showing `c`.
fn picker_for(c: Rgb) -> ColorPickerModel {
    let color = Color::from_rgb8(c.r, c.g, c.b);
    ColorPickerModel::new("Hex", "RGB", Some(color), Some(color))
}

/// Why a palette could not be opened: the error, and the file's text when
/// it could be read (then it opens read-only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenFailure {
    pub error: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    // Results of effects.
    Seeded(Result<usize, String>),
    StoreLoaded {
        entries: Vec<EntrySummary>,
        settings: Result<Settings, String>,
    },
    Entries(Vec<EntrySummary>),
    Opened {
        slug: String,
        result: Result<Palette, OpenFailure>,
    },
    /// The version number and the palette as saved.
    Saved(Result<(u32, Palette), String>),
    Created(Result<Palette, String>),
    Deleted {
        slug: String,
        result: Result<(), String>,
    },
    SettingsSaved(Result<(), String>),
    // Editing.
    SetRole(Role, Rgb),
    SetMode(Mode),
    /// Open one role's row in place, or close it.
    Expand(Option<Role>),
    /// The expanded row's colour picker.
    Picker(ColorPickerUpdate),
    HexInput(Role, String),
    SetName(Role, String),
    /// Pin a slot at its current effective colour.
    Pin(Slot),
    Unpin(Slot),
    PinHexInput(Slot, String),
    /// Pin `background` to `none`, or remove that pin.
    NoBackground(bool),
    SetNotes(String),
    /// Ctrl+S.
    Save,
    // Header, menu and dialogs.
    Menu(bool),
    ShowSwitch,
    Choose(String),
    ShowNew,
    ShowDuplicate,
    NameInput(String),
    NameMode(Mode),
    NameConfirm,
    ShowDelete,
    ConfirmDelete,
    UnsavedSave,
    UnsavedDiscard,
    CloseDialog,
    CloseRequested,
    // Apply, undo, import and export.
    /// Open the Apply sheet.
    ShowApply,
    ApplySheetLoaded {
        slug: String,
        targets: Vec<TargetInfo>,
        /// Boxed: a found pair is a whole palette.
        pair: Box<PairLookup>,
    },
    /// Turn a target on or off in the sheet.
    SetTarget(String, bool),
    ConfirmApply,
    Applied(Result<ApplyReport, ApplyFailure>),
    /// The toast's Undo and the menu's Undo last apply.
    UndoLastApply,
    Undone(Result<UndoDone, String>),
    ShowImport,
    ShowExport,
    /// The file chooser's folder, or why it could not open.
    Folder(FolderFor, Result<PathBuf, String>),
    /// A rice repo read as a palette, with its notes.
    Imported(Result<(Palette, Vec<String>), String>),
    ImportReplace,
    ImportKeepBoth,
    ImportSaved {
        result: Result<Palette, String>,
        notes: Vec<String>,
    },
    Exported(Result<ExportSummary, String>),
    // New from wallpaper.
    ShowWallpaper,
    /// COSMIC's current wallpaper, when it is an image file.
    WallpaperFound(Option<PathBuf>),
    /// Choose another image.
    ChooseImage,
    /// The file chooser's image, or why it could not open.
    Image(Result<PathBuf, String>),
    DraftReady {
        path: PathBuf,
        clusters: Vec<Cluster>,
        mode: Mode,
    },
    DraftFailed {
        path: PathBuf,
        error: String,
    },
    WallpaperMode(Mode),
    WallpaperName(String),
    WallpaperCreate,
    // Compare, pairs and snapshots.
    /// The open palette's saved versions, newest first.
    Versions {
        slug: String,
        result: Result<Vec<VersionInfo>, String>,
    },
    /// The pair chip: show the pair, or choose one.
    PairChip,
    PairLoaded {
        slug: String,
        /// Boxed: a found pair is a whole palette.
        lookup: Box<PairLookup>,
    },
    /// A palette chosen in Pair with...
    PairChoose(String),
    /// Yes, pair with the chosen palette although it is paired.
    PairConfirm,
    Linked {
        slug: String,
        with: String,
        result: Result<(), String>,
    },
    Unpair,
    Unlinked {
        slug: String,
        result: Result<(), String>,
    },
    /// The Compare chip.
    Compare,
    /// A version from Compare's list, by its place in it.
    CompareVersion(usize),
    VersionLoaded {
        slug: String,
        n: u32,
        result: Result<Palette, String>,
    },
    ShowPinSnapshot,
    SnapshotInput(String),
    SnapshotConfirm,
    /// Remove the shown version's snapshot name.
    UnpinSnapshot,
    /// A snapshot name was written (`Some`) or removed (`None`).
    SnapshotSaved {
        slug: String,
        n: u32,
        name: Option<String>,
        result: Result<(), String>,
    },
    // Try it live.
    /// The header toggle.
    LiveToggle(bool),
    LiveStarted(Result<LiveSession, String>),
    /// A text field's timer ran out for this generation.
    LiveTick(u64),
    /// A live apply finished; `session` carries its record.
    LiveApplied {
        session: LiveSession,
        result: Result<ApplyReport, ApplyFailure>,
    },
    /// Keep or Revert in the dialog.
    LiveKeep,
    LiveRevert,
    LiveKept(Result<(), String>),
    /// `None` when the session had written nothing.
    LiveReverted(Result<Option<UndoDone>, String>),
    /// The session a previous run left open, read at start.
    LivePending(Option<LiveSession>),
    /// Whether a target that reloads live is installed and usable.
    LiveTargets(bool),
    /// An effect with no failure Msg of its own failed.
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum Effect {
    SeedStarters,
    LoadStore,
    ListEntries,
    OpenPalette(String),
    /// Save a version; `fresh` refuses a slug that already exists.
    SaveVersion {
        palette: Palette,
        fresh: bool,
    },
    /// Save a new palette as version 1, refusing a slug that exists.
    Create(Palette),
    Duplicate {
        from: String,
        name: String,
    },
    Delete(String),
    SaveSettings(Settings),
    /// Read the targets and the pair for the Apply sheet.
    LoadApplySheet {
        palette: Palette,
        settings: Settings,
    },
    Apply {
        palette: Palette,
        pair: Option<Palette>,
        settings: Settings,
    },
    Undo,
    /// Read a rice repo.
    Import(PathBuf),
    /// Save an imported palette: as a new palette (refusing a slug that
    /// exists), or with `replace` as a new version of the readable palette
    /// with its slug, keeping that palette's pair and notes.
    SaveImport {
        palette: Palette,
        notes: Vec<String>,
        replace: bool,
    },
    Export {
        palette: Palette,
        folder: PathBuf,
    },
    /// Find COSMIC's current wallpaper, reading missing keys from this
    /// system folder.
    FindWallpaper(PathBuf),
    /// Read an image and find its colours.
    DraftImage(PathBuf),
    LoadVersions(String),
    LoadVersion {
        slug: String,
        n: u32,
    },
    /// Read the pair of this working copy.
    LoadPair(Palette),
    LinkPair {
        slug: String,
        with: String,
    },
    UnlinkPair(String),
    PinSnapshot {
        slug: String,
        n: u32,
        name: String,
    },
    UnpinSnapshot {
        slug: String,
        n: u32,
    },
    Toast(ToastItem),
    /// Put text on the clipboard.
    Copy(String),
    /// Ask for a folder with the file chooser.
    ChooseFolder(FolderFor),
    /// Ask for an image with the file chooser.
    ChooseImage,
    /// Open a live session for this palette slug.
    StartLive(String),
    /// Apply to the targets that reload live, in the session.
    LiveApply {
        session: LiveSession,
        palette: Palette,
        settings: Settings,
    },
    /// Send `Msg::LiveTick(generation)` after the delay.
    StartTimer(Duration, u64),
    KeepLive(LiveSession),
    RevertLive(LiveSession),
    /// Read the session a previous run left open.
    FindPendingLive,
    /// Read whether a target that reloads live can be used.
    LoadLiveTargets(Settings),
    Exit,
}

impl Effect {
    /// Whether `perform` runs it (rather than the window).
    pub fn is_io(&self) -> bool {
        !matches!(
            self,
            Effect::Toast(_)
                | Effect::Copy(_)
                | Effect::ChooseFolder(_)
                | Effect::ChooseImage
                | Effect::StartTimer(..)
                | Effect::Exit
        )
    }
}

pub struct State {
    pub ctx: Ctx,
    /// The system COSMIC config folder (`/usr/share/cosmic` in the app).
    pub wallpaper_system: PathBuf,
    pub settings: Settings,
    /// `settings.toml` could not be read: defaults are in use and the file
    /// is not written until the user changes a setting.
    pub settings_unreadable: bool,
    pub entries: Vec<EntrySummary>,
    pub current: Option<Current>,
    pub dialog: Option<Dialog>,
    /// Every toast shown, newest last.
    pub toasts: Vec<ToastItem>,
    pub menu_open: bool,
    /// The role whose row is open in place.
    pub expanded: Option<Role>,
    /// The expanded row's colour picker, rebuilt on `Expand`.
    pub picker: ColorPickerModel,
    /// Hex fields whose text differs from their colour.
    pub fields: BTreeMap<Field, FieldEdit>,
    /// What happens once the save asked for by the unsaved-changes question
    /// succeeds.
    after_save: Option<Leave>,
    /// A save is running; another Ctrl+S waits for its result.
    saving: bool,
    pub split: Split,
    /// The open palette's saved versions, newest first.
    pub versions: Vec<VersionInfo>,
    /// The open palette's pair, read when the pair split opened.
    pair_palette: Option<Palette>,
    /// A version Compare read, by number, or why it could not be read.
    compare_version: Option<(u32, Result<Palette, String>)>,
    /// The open try-it-live session.
    live: Option<Live>,
    /// A target that reloads live is installed and usable.
    live_targets: bool,
    /// The app's lock, held while the window is open.
    _app_lock: Option<File>,
    /// Another Irida window holds the app's lock: the live marker may be its
    /// open session, so this window neither offers it back nor starts one.
    other_window: bool,
}

/// Toasts kept in `State::toasts`.
const TOASTS_KEPT: usize = 50;

impl State {
    /// `app_lock` is what `live::lock_app` gave. A lock that could not be
    /// taken for an error counts as no other window.
    pub fn new(
        ctx: Ctx,
        wallpaper_system: PathBuf,
        app_lock: io::Result<Option<File>>,
    ) -> (State, Vec<Effect>) {
        let other_window = matches!(app_lock, Ok(None));
        let state = State {
            ctx,
            wallpaper_system,
            settings: Settings::default(),
            settings_unreadable: false,
            entries: Vec::new(),
            current: None,
            dialog: None,
            toasts: Vec::new(),
            menu_open: false,
            expanded: None,
            picker: picker_for(Rgb { r: 0, g: 0, b: 0 }),
            fields: BTreeMap::new(),
            after_save: None,
            saving: false,
            split: Split::Off,
            versions: Vec::new(),
            pair_palette: None,
            compare_version: None,
            live: None,
            live_targets: false,
            _app_lock: app_lock.ok().flatten(),
            other_window,
        };
        (state, vec![Effect::SeedStarters, Effect::LoadStore])
    }

    pub fn dirty(&self) -> bool {
        self.current.as_ref().is_some_and(Current::dirty)
    }

    /// The header's palette name, with a dot after it for unsaved changes.
    pub fn title(&self) -> String {
        match &self.current {
            Some(c @ Current::Editable { working, .. }) if c.dirty() => {
                format!("{} \u{2022}", working.name)
            }
            Some(Current::Editable { working, .. }) => working.name.clone(),
            Some(Current::Unreadable { slug, .. }) => format!("{slug} (read-only)"),
            None => "No palette".to_string(),
        }
    }

    /// The main area's text when no palette is open. A store with palettes
    /// can get here when the one opened at start fails to read.
    pub fn empty_text(&self) -> &'static str {
        if self.entries.is_empty() {
            "No palettes yet. Use New in the palette menu to make one."
        } else {
            "Choose a palette from the palette menu."
        }
    }

    fn editable(&self) -> bool {
        matches!(self.current, Some(Current::Editable { .. }))
    }

    /// The working copy, when the open palette can be edited.
    fn working_mut(&mut self) -> Option<&mut Palette> {
        match &mut self.current {
            Some(Current::Editable { working, .. }) => Some(working),
            _ => None,
        }
    }

    /// What the user typed in `field`, when it differs from the colour.
    pub fn field(&self, field: Field) -> Option<&FieldEdit> {
        self.fields.get(&field)
    }

    fn set_field(&mut self, field: Field, text: String, error: Option<&str>) {
        let error = error.map(str::to_string);
        self.fields.insert(field, FieldEdit { text, error });
    }

    /// Close the role editor and any split, for a newly opened palette.
    fn reset_editor(&mut self) {
        self.expanded = None;
        self.fields.clear();
        self.split = Split::Off;
        self.versions.clear();
        self.pair_palette = None;
        self.compare_version = None;
    }

    /// An open wallpaper draft that has never been saved: it is not in the
    /// store yet.
    fn unsaved_draft(&self) -> bool {
        matches!(self.current, Some(Current::Editable { saved: None, .. }))
    }

    /// The open palette is an unsaved draft with this slug.
    pub fn draft_is(&self, slug: &str) -> bool {
        self.unsaved_draft() && self.open_is(slug)
    }

    pub fn menu_enabled(&self, item: MenuItem) -> bool {
        match item {
            MenuItem::Switch => !self.entries.is_empty(),
            MenuItem::New | MenuItem::NewFromWallpaper | MenuItem::Import => true,
            // A read-only palette offers only what the plan lists for it.
            MenuItem::UndoLastApply => !matches!(self.current, Some(Current::Unreadable { .. })),
            MenuItem::Export => self.editable(),
            // Both work on the stored palette, which a draft does not have.
            MenuItem::Duplicate => self.editable() && !self.unsaved_draft(),
            MenuItem::Delete => self.current.is_some() && !self.unsaved_draft(),
            MenuItem::Unpair => self
                .stored_palette()
                .is_some_and(|(_, working)| working.pair.is_some()),
        }
    }

    /// The open palette's saved copy and working copy, when it is editable
    /// and in the store (not a never-saved draft).
    fn stored_palette(&self) -> Option<(&Palette, &Palette)> {
        match &self.current {
            Some(Current::Editable {
                saved: Some(saved),
                working,
                ..
            }) => Some((saved, working)),
            _ => None,
        }
    }

    /// A stored palette's name, else its slug.
    fn entry_label(&self, slug: &str) -> String {
        self.entries
            .iter()
            .find(|e| e.slug == slug)
            .map_or(slug, EntrySummary::label)
            .to_string()
    }

    /// The pair chip: "Pair: Ember Day", or "Pair with..." when unpaired.
    pub fn pair_label(&self) -> String {
        match self
            .current
            .as_ref()
            .and_then(Current::working)
            .and_then(|w| w.pair.as_deref())
        {
            Some(slug) => format!("Pair: {}", self.entry_label(slug)),
            None => "Pair with...".to_string(),
        }
    }

    /// Why a header item is disabled, when there is something to say.
    pub fn header_tooltip(&self, item: HeaderItem) -> Option<&'static str> {
        match item {
            HeaderItem::Compare if self.editable() && !self.header_enabled(item) => {
                Some(COMPARE_NEEDS_VERSION)
            }
            HeaderItem::Pair if self.unsaved_draft() => Some("Save it first (Ctrl+S)"),
            HeaderItem::TryLive if self.live.is_none() => {
                if self.other_window {
                    Some(LIVE_IN_OTHER_WINDOW)
                } else if self.unsaved_draft() {
                    Some(LIVE_NEEDS_SAVE)
                } else if self.editable() && !self.live_targets {
                    Some(LIVE_NEEDS_TARGET)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The palettes of the other mode than `mode`, in list order.
    fn pair_choices(&self, mode: Mode) -> Vec<PairChoice> {
        self.entries
            .iter()
            .filter_map(|e| match &e.palette {
                Ok((name, m)) if *m != mode && !self.open_is(&e.slug) => Some(PairChoice {
                    slug: e.slug.clone(),
                    name: name.clone(),
                    paired_with: e.pair.as_deref().map(|p| self.entry_label(p)),
                }),
                _ => None,
            })
            .collect()
    }

    /// Set `pair` on both copies of the open palette when it is `slug`, so
    /// a later save keeps what the store now holds.
    fn set_pair(&mut self, slug: &str, pair: Option<String>) {
        if let Some(Current::Editable {
            slug: open,
            saved,
            working,
        }) = &mut self.current
            && open == slug
        {
            if let Some(saved) = saved {
                saved.pair = pair.clone();
            }
            working.pair = pair;
        }
    }

    fn open_is(&self, slug: &str) -> bool {
        self.current.as_ref().is_some_and(|c| c.slug() == slug)
    }

    /// The version number Compare shows.
    fn shown_version(&self) -> Option<u32> {
        match self.split {
            Split::Saved(VersionRef::Latest) => self.versions.first().map(|v| v.n),
            Split::Saved(VersionRef::Version(n)) => Some(n),
            _ => None,
        }
    }

    /// The split preview's two columns, when the preview is split.
    pub fn split_view(&self) -> Option<SplitView> {
        let Some(Current::Editable { saved, working, .. }) = &self.current else {
            return None;
        };
        let working_column = |label: String| SplitColumn {
            label,
            palette: Some(working.clone()),
            note: None,
        };
        match self.split {
            Split::Off => None,
            Split::Pair => {
                let right = match &self.pair_palette {
                    Some(p) => SplitColumn {
                        label: half_label(p),
                        palette: Some(p.clone()),
                        note: None,
                    },
                    None => SplitColumn {
                        label: working
                            .pair
                            .as_deref()
                            .map_or_else(String::new, |s| self.entry_label(s)),
                        palette: None,
                        note: Some("Reading the pair".to_string()),
                    },
                };
                Some(SplitView {
                    left: working_column(half_label(working)),
                    right,
                    versions: None,
                })
            }
            Split::Saved(which) => {
                let n = self.shown_version()?;
                let info = self.versions.iter().find(|v| v.n == n);
                let (palette, note) = match which {
                    VersionRef::Latest => (saved.clone(), None),
                    VersionRef::Version(_) => match &self.compare_version {
                        Some((k, Ok(p))) if *k == n => (Some(p.clone()), None),
                        Some((k, Err(e))) if *k == n => (None, Some(e.clone())),
                        _ => (None, Some(format!("Reading version {n}"))),
                    },
                };
                let left = SplitColumn {
                    label: info.map_or_else(|| format!("v{n}"), version_label),
                    palette,
                    note,
                };
                let versions = VersionChoice {
                    labels: self.versions.iter().map(version_label).collect(),
                    selected: self.versions.iter().position(|v| v.n == n),
                    snapshot: info.and_then(|v| v.snapshot.clone()),
                };
                Some(SplitView {
                    left,
                    right: working_column("Working copy".to_string()),
                    versions: Some(versions),
                })
            }
        }
    }

    /// The snapshot name typed in the Snapshot dialog, trimmed, or why it
    /// cannot be used (`Err("")` while it is empty).
    pub fn snapshot_check(&self) -> Result<String, String> {
        let Some(Dialog::Snapshot { name, .. }) = &self.dialog else {
            return Err(String::new());
        };
        let name = name.trim();
        if name.is_empty() {
            Err(String::new())
        } else if name.chars().count() > SNAPSHOT_NAME_MAX {
            Err(format!("Use at most {SNAPSHOT_NAME_MAX} characters"))
        } else {
            Ok(name.to_string())
        }
    }

    /// The New from wallpaper dialog can create: an image was drafted, no
    /// other is being read, and the name can be used for a new palette.
    pub fn wallpaper_ready(&self) -> bool {
        match &self.dialog {
            Some(Dialog::Wallpaper(d)) => {
                d.roles.is_some() && d.reading.is_none() && self.name_check(&d.name).is_ok()
            }
            _ => false,
        }
    }

    fn wallpaper_dialog(&mut self) -> Option<&mut WallpaperDialog> {
        match &mut self.dialog {
            Some(Dialog::Wallpaper(d)) => Some(d),
            _ => None,
        }
    }

    pub fn header_enabled(&self, item: HeaderItem) -> bool {
        match item {
            HeaderItem::Mode | HeaderItem::Apply => self.editable(),
            // On, it can always be turned off (unless it is ending).
            HeaderItem::TryLive => match &self.live {
                Some(live) => live.end.is_none(),
                None => self.stored_palette().is_some() && self.live_targets && !self.other_window,
            },
            // Pairing works on the stored palette, which a draft is not yet.
            HeaderItem::Pair => self.stored_palette().is_some(),
            HeaderItem::Compare => self.stored_palette().is_some() && !self.versions.is_empty(),
        }
    }

    /// The slug a name gives, or why the name cannot be used for a new
    /// palette. An empty name gives `Err("")` (nothing to say yet).
    pub fn name_check(&self, name: &str) -> Result<String, String> {
        if name.is_empty() {
            return Err(String::new());
        }
        if validate_name(name).is_err() {
            return Err("Use letters and digits, with single spaces between words".to_string());
        }
        let slug = slug_from_name(name);
        if slug.is_empty() {
            return Err("Add a letter: the file name is made from the letters".to_string());
        }
        if self.entries.iter().any(|e| e.slug == slug) {
            return Err(format!("A palette called {slug} already exists"));
        }
        // An open draft holds its slug too: a second palette made with it
        // could never be saved, since the draft takes the name first.
        if let Some(Current::Editable {
            saved: None,
            slug: open,
            ..
        }) = &self.current
            && *open == slug
        {
            return Err(format!("A palette called {slug} is open and not saved yet"));
        }
        Ok(slug)
    }

    fn toast(&mut self, text: impl Into<String>) -> Effect {
        self.toast_with(text, None)
    }

    /// A toast with an Undo button that sends `Msg::UndoLastApply`.
    fn toast_undo(&mut self, text: impl Into<String>) -> Effect {
        self.toast_with(text, Some(("Undo".to_string(), Msg::UndoLastApply)))
    }

    fn toast_with(&mut self, text: impl Into<String>, action: Option<(String, Msg)>) -> Effect {
        let item = ToastItem {
            text: text.into(),
            action,
        };
        self.toasts.push(item.clone());
        if self.toasts.len() > TOASTS_KEPT {
            self.toasts.remove(0);
        }
        Effect::Toast(item)
    }

    /// Open `p` for editing as saved, read its versions, and remember it as
    /// the last palette.
    fn open_editable(&mut self, p: Palette) -> Vec<Effect> {
        let slug = p.slug.clone();
        self.reset_editor();
        self.current = Some(Current::Editable {
            slug: slug.clone(),
            saved: Some(p.clone()),
            working: p,
        });
        let mut effects = vec![Effect::LoadVersions(slug.clone())];
        effects.extend(self.remember(slug));
        effects
    }

    /// Write `slug` as `settings.last_palette`, unless it is already there or
    /// `settings.toml` could not be read.
    fn remember(&mut self, slug: String) -> Vec<Effect> {
        if self.settings_unreadable || self.settings.last_palette.as_deref() == Some(&slug) {
            return Vec::new();
        }
        self.settings.last_palette = Some(slug);
        vec![Effect::SaveSettings(self.settings.clone())]
    }

    /// Do `leave` the user asked for, as `leave_now` does. It replaces any
    /// answer still waiting on a save: the newest answer decides.
    fn leave(&mut self, leave: Leave) -> Vec<Effect> {
        self.after_save = None;
        self.leave_now(leave)
    }

    /// Do `leave`, first asking Save, Discard or Cancel when there are
    /// unsaved changes, and Keep or Revert when Try it live is on.
    fn leave_now(&mut self, leave: Leave) -> Vec<Effect> {
        if self.live.is_some() {
            return self.ask_live(LiveThen::Leave(leave));
        }
        if self.dirty() {
            self.dialog = Some(Dialog::Unsaved(leave));
            Vec::new()
        } else {
            self.dialog = None;
            self.go(leave)
        }
    }

    fn go(&mut self, leave: Leave) -> Vec<Effect> {
        match leave {
            Leave::Open(slug) => vec![Effect::OpenPalette(slug)],
            Leave::Create(p) => vec![Effect::Create(p)],
            Leave::Duplicate { from, name } => vec![Effect::Duplicate { from, name }],
            // Not remembered as the last palette until it is saved.
            Leave::Draft(p) => {
                self.reset_editor();
                self.current = Some(Current::Editable {
                    slug: p.slug.clone(),
                    saved: None,
                    working: p,
                });
                Vec::new()
            }
            // On Discard the edits go at once, so a save cannot bury the
            // import, and so does what was typed for them.
            Leave::Import { palette, notes } => {
                if let Some(Current::Editable {
                    saved: Some(saved),
                    working,
                    ..
                }) = &mut self.current
                {
                    *working = saved.clone();
                    self.expanded = None;
                    self.fields.clear();
                }
                vec![Effect::SaveImport {
                    palette: *palette,
                    notes,
                    replace: true,
                }]
            }
            Leave::Exit => vec![Effect::Exit],
        }
    }

    /// The Save effect for the working copy, or a toast when it cannot be
    /// saved. `None` when there is nothing to save.
    fn save(&mut self) -> Option<Effect> {
        // Display names are kept as typed while editing and saved trimmed.
        if !self.saving
            && let Some(w) = self.working_mut()
        {
            for name in w.names.values_mut() {
                if name.trim().len() != name.len() {
                    *name = name.trim().to_string();
                }
            }
        }
        let Some(Current::Editable { saved, working, .. }) = &self.current else {
            return None;
        };
        if self.saving || saved.as_ref() == Some(working) {
            return None;
        }
        if let Err(e) = working.validate() {
            let text = format!("Not saved: {e}");
            return Some(self.toast(text));
        }
        let effect = Effect::SaveVersion {
            palette: working.clone(),
            fresh: saved.is_none(),
        };
        self.saving = true;
        Some(effect)
    }

    /// The palette New starts from: the open palette's roles (Ember's when
    /// none is open), with the new name and mode and no pins, pair, names
    /// or notes.
    fn new_palette(&self, name: &str, slug: String, mode: Mode) -> Palette {
        let roles = match self.current.as_ref().and_then(Current::working) {
            Some(p) => p.roles.clone(),
            None => {
                starters()
                    .into_iter()
                    .next()
                    .expect("Ember is the first starter")
                    .roles
            }
        };
        Palette {
            name: name.to_string(),
            slug,
            mode,
            pair: None,
            notes: None,
            roles,
            names: Default::default(),
            pins: Default::default(),
        }
    }

    /// The working copy when it can be applied or exported, else the
    /// effects saying why not: a toast, or nothing when no palette can be
    /// edited (`doing` is "applied" or "exported").
    fn checked_working(&mut self, doing: &str) -> Result<Palette, Vec<Effect>> {
        let Some(working) = self.current.as_ref().and_then(Current::working) else {
            return Err(Vec::new());
        };
        match working.validate() {
            Ok(()) => Ok(working.clone()),
            Err(e) => Err(vec![self.toast(format!("Not {doing}: {e}"))]),
        }
    }

    /// The Apply sheet for `working` with `targets` and its pair lookup.
    fn apply_sheet(working: &Palette, targets: Vec<TargetInfo>, pair: PairLookup) -> ApplySheet {
        let name = &working.name;
        let alone = format!("Applies {name}");
        let (pair, applies, pair_note) = match pair {
            PairLookup::Found(p) => {
                let applies = format!("Applies {name} and {}", p.name);
                (Some(p), applies, None)
            }
            PairLookup::None => (None, alone, None),
            PairLookup::SameMode(slug) => (
                None,
                alone,
                Some(format!(
                    "Its pair {slug} has the same mode, so {name} is applied alone."
                )),
            ),
            PairLookup::Missing { slug, reason } => (
                None,
                alone,
                Some(format!(
                    "Its pair {slug} cannot be loaded ({reason}), so {name} is applied alone."
                )),
            ),
        };
        ApplySheet {
            targets,
            pair,
            applies,
            pair_note,
        }
    }

    /// Try it live is on (or ending).
    pub fn live_on(&self) -> bool {
        self.live.is_some()
    }

    /// Ask Keep or Revert, then do `then`.
    fn ask_live(&mut self, then: LiveThen) -> Vec<Effect> {
        self.menu_open = false;
        // Already ending: `then` follows it (the newest request wins).
        if let Some(Live {
            end: Some((_, waiting)),
            ..
        }) = &mut self.live
        {
            *waiting = Some(then);
            return Vec::new();
        }
        // The start-up prompt stays, without Cancel; `then` follows it.
        if let Some(Dialog::KeepOrRevert {
            then: waiting,
            pending: true,
        }) = &mut self.dialog
        {
            *waiting = Some(then);
            return Vec::new();
        }
        self.dialog = Some(Dialog::KeepOrRevert {
            then: Some(then),
            pending: false,
        });
        Vec::new()
    }

    /// The session, when the open palette's edits are applied to it.
    fn live_for_edits(&mut self) -> Option<&mut Live> {
        let slug = match &self.current {
            Some(Current::Editable { slug, .. }) => slug.clone(),
            _ => return None,
        };
        self.live
            .as_mut()
            .filter(|l| !l.from_last_run && l.end.is_none() && l.slug == slug)
    }

    /// An edit to apply at once (a picker release, a pin): it runs now, or
    /// next when an apply is running. A text field's waiting timer is
    /// made stale, since this apply carries its edit too.
    fn live_now(&mut self) -> Vec<Effect> {
        let Some(live) = self.live_for_edits() else {
            return Vec::new();
        };
        live.generation += 1;
        live.queued = true;
        self.live_next()
    }

    /// A text field's edit: applied `LIVE_DELAY` after the last keystroke.
    fn live_later(&mut self) -> Vec<Effect> {
        let Some(live) = self.live_for_edits() else {
            return Vec::new();
        };
        live.generation += 1;
        vec![Effect::StartTimer(LIVE_DELAY, live.generation)]
    }

    /// What the session does next, once nothing runs: end when asked,
    /// else apply the waiting edit.
    fn live_next(&mut self) -> Vec<Effect> {
        let working = self.current.as_ref().and_then(Current::working).cloned();
        let settings = self.settings.clone();
        let Some(live) = self.live.as_mut() else {
            return Vec::new();
        };
        if live.busy {
            return Vec::new();
        }
        let Some(session) = live.session.clone() else {
            return Vec::new();
        };
        if let Some((end, _)) = &live.end {
            live.busy = true;
            live.queued = false;
            return vec![match end {
                LiveEnd::Keep => Effect::KeepLive(session),
                LiveEnd::Revert => Effect::RevertLive(session),
            }];
        }
        if !std::mem::take(&mut live.queued) {
            return Vec::new();
        }
        // Edits that cannot be saved are not applied either; Save says why.
        let Some(palette) = working.filter(|w| {
            w.slug == live.slug && w.validate().is_ok() && live.applied.as_ref() != Some(w)
        }) else {
            return Vec::new();
        };
        live.busy = true;
        live.applied = Some(palette.clone());
        vec![Effect::LiveApply {
            session,
            palette,
            settings,
        }]
    }

    /// The session ended (or never started): do what waited for it.
    fn live_then(&mut self, then: Option<LiveThen>) -> Vec<Effect> {
        match then {
            None => Vec::new(),
            Some(LiveThen::Switch) => update(self, Msg::ShowSwitch),
            Some(LiveThen::Delete) => update(self, Msg::ShowDelete),
            Some(LiveThen::Apply) => update(self, Msg::ShowApply),
            // Asked before, so an answer waiting on a save still stands.
            Some(LiveThen::Leave(leave)) => self.leave_now(leave),
        }
    }

    /// A Keep or Revert that failed: the session stays on, so it can be
    /// ended again.
    fn live_end_failed(&mut self) {
        if let Some(live) = self.live.as_mut() {
            live.busy = false;
            live.end = None;
        }
    }

    /// The slug to open at start: the last palette when it is readable, else
    /// the first readable entry, else the first entry.
    fn start_slug(&self) -> Option<String> {
        let last = self.settings.last_palette.as_deref();
        self.entries
            .iter()
            .find(|e| Some(e.slug.as_str()) == last && e.readable())
            .or_else(|| self.entries.iter().find(|e| e.readable()))
            .or_else(|| self.entries.first())
            .map(|e| e.slug.clone())
    }
}

pub fn update(state: &mut State, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Seeded(Ok(_)) => Vec::new(),
        Msg::Seeded(Err(e)) => {
            vec![state.toast(format!("Could not add the starter palettes: {e}"))]
        }
        Msg::StoreLoaded { entries, settings } => {
            state.entries = entries;
            let mut effects = Vec::new();
            match settings {
                Ok(s) => {
                    state.settings = s;
                    state.settings_unreadable = false;
                }
                Err(e) => {
                    state.settings = Settings::default();
                    state.settings_unreadable = true;
                    effects.push(state.toast(format!(
                        "Could not read your settings ({e}). Using defaults; the file is kept until you change a setting."
                    )));
                }
            }
            // Another window's open session is not one left by a crash.
            if !state.other_window {
                effects.push(Effect::FindPendingLive);
            }
            effects.push(Effect::LoadLiveTargets(state.settings.clone()));
            if let Some(slug) = state.start_slug() {
                effects.push(Effect::OpenPalette(slug));
            }
            effects
        }
        Msg::Entries(entries) => {
            state.entries = entries;
            Vec::new()
        }
        Msg::Opened { slug, result } => match result {
            Ok(p) => state.open_editable(p),
            Err(OpenFailure {
                error,
                text: Some(text),
            }) => {
                state.reset_editor();
                state.current = Some(Current::Unreadable {
                    slug: slug.clone(),
                    error,
                    text,
                });
                state.remember(slug)
            }
            Err(OpenFailure { error, text: None }) => vec![
                state.toast(format!("Could not open {slug}: {error}")),
                Effect::ListEntries,
            ],
        },
        Msg::Saved(result) => {
            let leave = state.after_save.take();
            state.saving = false;
            match result {
                Ok((n, p)) => {
                    let mut effects = vec![state.toast(format!("Saved version {n}"))];
                    if let Some(Current::Editable {
                        slug,
                        saved,
                        working,
                    }) = &mut state.current
                        && *slug == p.slug
                    {
                        // The save wrote the store's pair, not an edit.
                        working.pair = p.pair.clone();
                        *saved = Some(p.clone());
                        if !state.versions.iter().any(|v| v.n == n) {
                            state.versions.insert(0, VersionInfo { n, snapshot: None });
                        }
                    }
                    // The list shows the saved name and mode.
                    match state.entries.iter_mut().find(|e| e.slug == p.slug) {
                        Some(e) => {
                            e.palette = Ok((p.name.clone(), p.mode));
                            e.pair = p.pair.clone();
                        }
                        None => effects.push(Effect::ListEntries),
                    }
                    // A draft's first save makes it the last palette.
                    effects.extend(state.remember(p.slug.clone()));
                    // An edit made while the save ran is not in it: ask
                    // again rather than drop it.
                    if let Some(leave) = leave {
                        effects.extend(state.leave_now(leave));
                    }
                    effects
                }
                Err(e) => {
                    let mut effects = vec![state.toast(format!("Not saved: {e}"))];
                    // Asked from the unsaved-changes question: ask it again,
                    // so a draft is not lost.
                    if let Some(leave) = leave {
                        effects.extend(state.leave_now(leave));
                    }
                    effects
                }
            }
        }
        Msg::Created(Ok(p)) => {
            let text = format!("Created {}", p.name);
            let mut effects = vec![state.toast(text), Effect::ListEntries];
            effects.extend(state.open_editable(p));
            effects
        }
        Msg::Created(Err(e)) => vec![state.toast(e)],
        Msg::Deleted { slug, result } => match result {
            Ok(()) => {
                let label = state
                    .entries
                    .iter()
                    .find(|e| e.slug == slug)
                    .map_or_else(|| slug.clone(), |e| e.label().to_string());
                let mut effects = vec![state.toast(format!("Deleted {label}"))];
                if let Some(i) = state.entries.iter().position(|e| e.slug == slug) {
                    state.entries.remove(i);
                    if state.current.as_ref().is_some_and(|c| c.slug() == slug) {
                        state.current = None;
                        state.reset_editor();
                        let next = state
                            .entries
                            .get(i)
                            .or_else(|| i.checked_sub(1).and_then(|j| state.entries.get(j)));
                        if let Some(next) = next {
                            effects.push(Effect::OpenPalette(next.slug.clone()));
                        }
                    }
                } else if state.current.as_ref().is_some_and(|c| c.slug() == slug) {
                    state.current = None;
                    state.reset_editor();
                }
                effects.push(Effect::ListEntries);
                effects
            }
            Err(e) => vec![state.toast(format!("Could not delete {slug}: {e}"))],
        },
        Msg::SettingsSaved(Ok(())) => Vec::new(),
        Msg::SettingsSaved(Err(e)) => {
            vec![state.toast(format!("Could not save your settings: {e}"))]
        }
        Msg::SetRole(role, value) => {
            set_role(state, role, value);
            state.live_now()
        }
        Msg::SetMode(mode) => match state.working_mut() {
            Some(working) => {
                working.mode = mode;
                state.live_later()
            }
            None => Vec::new(),
        },
        Msg::Expand(role) => {
            let Some(working) = state.current.as_ref().and_then(Current::working) else {
                return Vec::new();
            };
            if let Some(r) = role {
                state.picker = picker_for(working.roles.get(r));
                state.fields.remove(&Field::Hex(r));
            }
            state.expanded = role;
            Vec::new()
        }
        Msg::Picker(u) => {
            let Some(role) = state.expanded.filter(|_| state.editable()) else {
                return Vec::new();
            };
            // The model's own task only writes the clipboard on Copied,
            // which `Effect::Copy` does instead.
            drop(state.picker.update::<Msg>(u.clone()));
            // A drag shows in the editor at once and reaches the live apps
            // when it is released.
            let (color, release) = match u {
                ColorPickerUpdate::ActiveColor(hsv) => (Some(rgb_from_hsv(hsv)), false),
                ColorPickerUpdate::ActionFinished | ColorPickerUpdate::AppliedColor => {
                    (state.picker.get_applied_color().map(rgb_from_color), true)
                }
                ColorPickerUpdate::Copied(_) => {
                    let hex = state
                        .current
                        .as_ref()
                        .and_then(Current::working)
                        .map(|p| p.roles.get(role).to_hex());
                    return hex.map(Effect::Copy).into_iter().collect();
                }
                _ => (None, false),
            };
            match color {
                Some(c) if release => update(state, Msg::SetRole(role, c)),
                Some(c) => {
                    set_role(state, role, c);
                    Vec::new()
                }
                None => Vec::new(),
            }
        }
        Msg::HexInput(role, text) => {
            if !state.editable() {
                return Vec::new();
            }
            match Rgb::from_hex(text.trim()) {
                Ok(c) => {
                    if let Some(working) = state.working_mut() {
                        working.roles.set(role, c);
                    }
                    if state.expanded == Some(role) {
                        state.picker = picker_for(c);
                    }
                    state.set_field(Field::Hex(role), text, None);
                }
                Err(_) => state.set_field(Field::Hex(role), text, Some(HEX_ERROR)),
            }
            state.live_later()
        }
        Msg::SetName(role, text) => {
            let Some(working) = state.working_mut() else {
                return Vec::new();
            };
            if text.trim().is_empty() {
                working.names.remove(&role);
            } else {
                working.names.insert(role, text);
            }
            state.live_later()
        }
        Msg::Pin(slot) => {
            if let Some(working) = state.working_mut()
                && !working.pins.is_pinned(slot)
                && let Some(c) = derive(working).slot(slot)
            {
                working
                    .pins
                    .set(slot, PinValue::Color(c))
                    .expect("a colour pin is always allowed");
                state.fields.remove(&Field::PinHex(slot));
            }
            state.live_now()
        }
        Msg::Unpin(slot) => {
            if let Some(working) = state.working_mut() {
                working.pins.remove(slot);
                state.fields.remove(&Field::PinHex(slot));
            }
            state.live_now()
        }
        Msg::PinHexInput(slot, text) => {
            let Some(working) = state.working_mut() else {
                return Vec::new();
            };
            if !matches!(working.pins.get(slot), Some(PinValue::Color(_))) {
                return Vec::new();
            }
            match Rgb::from_hex(text.trim()) {
                Ok(c) => {
                    working
                        .pins
                        .set(slot, PinValue::Color(c))
                        .expect("a colour pin is always allowed");
                    state.set_field(Field::PinHex(slot), text, None);
                }
                Err(_) => state.set_field(Field::PinHex(slot), text, Some(HEX_ERROR)),
            }
            state.live_later()
        }
        Msg::NoBackground(on) => {
            if let Some(working) = state.working_mut() {
                if on {
                    working
                        .pins
                        .set(Slot::Background, PinValue::None)
                        .expect("background can be none");
                    state.fields.remove(&Field::PinHex(Slot::Background));
                } else if working.pins.get(Slot::Background) == Some(PinValue::None) {
                    working.pins.remove(Slot::Background);
                }
            }
            state.live_now()
        }
        Msg::SetNotes(text) => match state.working_mut() {
            Some(working) => {
                working.notes = (!text.trim().is_empty()).then_some(text);
                state.live_later()
            }
            None => Vec::new(),
        },
        Msg::Save => state.save().into_iter().collect(),
        Msg::Menu(open) => {
            state.menu_open = open;
            Vec::new()
        }
        Msg::ShowSwitch => {
            state.menu_open = false;
            if state.live.is_some() {
                return state.ask_live(LiveThen::Switch);
            }
            if state.menu_enabled(MenuItem::Switch) {
                state.dialog = Some(Dialog::Switch);
            }
            Vec::new()
        }
        Msg::Choose(slug) => {
            if !matches!(state.dialog, Some(Dialog::Switch)) {
                return Vec::new();
            }
            if state.current.as_ref().is_some_and(|c| c.slug() == slug) {
                state.dialog = None;
                return Vec::new();
            }
            state.leave(Leave::Open(slug))
        }
        Msg::ShowNew => {
            state.menu_open = false;
            let mode = state
                .current
                .as_ref()
                .and_then(Current::working)
                .map_or(Mode::Dark, |p| p.mode);
            state.dialog = Some(Dialog::Name {
                purpose: NameFor::New,
                name: String::new(),
                mode,
            });
            Vec::new()
        }
        Msg::ShowDuplicate => {
            state.menu_open = false;
            if let Some(Current::Editable { working, .. }) = &state.current {
                state.dialog = Some(Dialog::Name {
                    purpose: NameFor::Duplicate,
                    name: String::new(),
                    mode: working.mode,
                });
            }
            Vec::new()
        }
        Msg::NameInput(text) => {
            if let Some(Dialog::Name { name, .. }) = &mut state.dialog {
                *name = text;
            }
            Vec::new()
        }
        Msg::NameMode(m) => {
            if let Some(Dialog::Name { mode, .. }) = &mut state.dialog {
                *mode = m;
            }
            Vec::new()
        }
        Msg::NameConfirm => {
            let Some(Dialog::Name {
                purpose,
                name,
                mode,
            }) = state.dialog.clone()
            else {
                return Vec::new();
            };
            let Ok(slug) = state.name_check(&name) else {
                return Vec::new();
            };
            match purpose {
                NameFor::New => {
                    let p = state.new_palette(&name, slug, mode);
                    state.leave(Leave::Create(p))
                }
                NameFor::Duplicate => {
                    let Some(Current::Editable { slug: from, .. }) = &state.current else {
                        state.dialog = None;
                        return Vec::new();
                    };
                    let from = from.clone();
                    state.leave(Leave::Duplicate { from, name })
                }
            }
        }
        Msg::ShowDelete => {
            state.menu_open = false;
            if state.live.is_some() && state.current.is_some() {
                return state.ask_live(LiveThen::Delete);
            }
            if let Some(current) = &state.current {
                let slug = current.slug().to_string();
                let label = match current {
                    Current::Editable { working, .. } => working.name.clone(),
                    Current::Unreadable { slug, .. } => slug.clone(),
                };
                state.dialog = Some(Dialog::Delete { slug, label });
            }
            Vec::new()
        }
        Msg::ConfirmDelete => match state.dialog.take() {
            Some(Dialog::Delete { slug, .. }) => vec![Effect::Delete(slug)],
            other => {
                state.dialog = other;
                Vec::new()
            }
        },
        Msg::UnsavedSave => match state.dialog.take() {
            // A save already running finishes first, then `leave` happens.
            Some(Dialog::Unsaved(leave)) if state.saving => {
                state.after_save = Some(leave);
                Vec::new()
            }
            Some(Dialog::Unsaved(leave)) => match state.save() {
                Some(effect @ Effect::SaveVersion { .. }) => {
                    state.after_save = Some(leave);
                    vec![effect]
                }
                Some(toast) => {
                    state.dialog = Some(Dialog::Unsaved(leave));
                    vec![toast]
                }
                None => state.go(leave),
            },
            other => {
                state.dialog = other;
                Vec::new()
            }
        },
        Msg::UnsavedDiscard => match state.dialog.take() {
            Some(Dialog::Unsaved(leave)) => {
                state.after_save = None;
                state.go(leave)
            }
            other => {
                state.dialog = other;
                Vec::new()
            }
        },
        // The session a previous run left open waits for an answer.
        Msg::CloseDialog
            if matches!(
                state.dialog,
                Some(Dialog::KeepOrRevert { pending: true, .. })
            ) =>
        {
            Vec::new()
        }
        Msg::CloseDialog => {
            // Cancel on the unsaved-changes question drops an earlier answer.
            if matches!(state.dialog, Some(Dialog::Unsaved(_))) {
                state.after_save = None;
            }
            state.dialog = None;
            state.menu_open = false;
            Vec::new()
        }
        Msg::CloseRequested => state.leave(Leave::Exit),
        Msg::ShowApply => {
            if !state.header_enabled(HeaderItem::Apply) {
                return Vec::new();
            }
            if state.live.is_some() {
                return state.ask_live(LiveThen::Apply);
            }
            match state.checked_working("applied") {
                Ok(palette) => vec![Effect::LoadApplySheet {
                    palette,
                    settings: state.settings.clone(),
                }],
                Err(effects) => effects,
            }
        }
        Msg::ApplySheetLoaded {
            slug,
            targets,
            pair,
        } => {
            // The palette changed while the targets were read.
            let Some(Current::Editable { working, .. }) = &state.current else {
                return Vec::new();
            };
            if working.slug != slug || state.dialog.is_some() {
                return Vec::new();
            }
            state.live_targets = any_live_target(&targets);
            state.dialog = Some(Dialog::Apply(State::apply_sheet(working, targets, *pair)));
            Vec::new()
        }
        Msg::SetTarget(name, on) => {
            let Some(Dialog::Apply(sheet)) = &state.dialog else {
                return Vec::new();
            };
            let groups = sheet_groups(&sheet.targets, &state.settings);
            if !groups.toggles.iter().any(|(n, _)| *n == name) {
                return Vec::new();
            }
            let changed = if on {
                state.settings.disabled.remove(&name)
            } else {
                state.settings.disabled.insert(name)
            };
            if !changed {
                return Vec::new();
            }
            // The user changed a setting, so an unreadable file is replaced.
            state.settings_unreadable = false;
            vec![Effect::SaveSettings(state.settings.clone())]
        }
        Msg::ConfirmApply => {
            let sheet = match state.dialog.take() {
                Some(Dialog::Apply(sheet)) => sheet,
                other => {
                    state.dialog = other;
                    return Vec::new();
                }
            };
            match state.checked_working("applied") {
                Ok(palette) => vec![Effect::Apply {
                    palette,
                    pair: sheet.pair,
                    settings: state.settings.clone(),
                }],
                Err(effects) => effects,
            }
        }
        Msg::Applied(Ok(report)) => vec![state.toast_undo(applied_text(&report))],
        Msg::Applied(Err(failure)) => {
            let text = failure.text();
            if failure.needs_undo() {
                vec![state.toast_undo(text)]
            } else {
                vec![state.toast(text)]
            }
        }
        Msg::UndoLastApply => {
            state.menu_open = false;
            if state.live.is_some() {
                return vec![state.toast(LIVE_BLOCKS_UNDO)];
            }
            vec![Effect::Undo]
        }
        Msg::Undone(Ok(done)) => {
            let mut lines = vec![format!("Restored apply {}", done.record_id)];
            lines.extend(done.failures);
            vec![state.toast(lines.join("\n"))]
        }
        Msg::Undone(Err(e)) => vec![state.toast(e)],
        Msg::ShowImport => {
            state.menu_open = false;
            vec![Effect::ChooseFolder(FolderFor::Import)]
        }
        Msg::ShowExport => {
            state.menu_open = false;
            if !state.menu_enabled(MenuItem::Export) {
                return Vec::new();
            }
            match state.checked_working("exported") {
                Ok(_) => vec![Effect::ChooseFolder(FolderFor::Export)],
                Err(effects) => effects,
            }
        }
        Msg::Folder(_, Err(e)) => vec![state.toast(format!("Could not choose a folder: {e}"))],
        Msg::Folder(FolderFor::Import, Ok(path)) => vec![Effect::Import(path)],
        Msg::Folder(FolderFor::Export, Ok(folder)) => match state.checked_working("exported") {
            Ok(palette) => vec![Effect::Export { palette, folder }],
            Err(effects) => effects,
        },
        Msg::Imported(Ok((palette, notes))) => {
            // An open draft's slug is taken too, and Replace never writes
            // over a draft.
            let draft = state.draft_is(&palette.slug);
            let entry = state.entries.iter().find(|e| e.slug == palette.slug);
            if entry.is_some() || draft {
                state.dialog = Some(Dialog::ImportCollision {
                    replace_ok: !draft && entry.is_some_and(|e| e.readable()),
                    palette,
                    notes,
                });
                Vec::new()
            } else {
                vec![Effect::SaveImport {
                    palette,
                    notes,
                    replace: false,
                }]
            }
        }
        Msg::Imported(Err(e)) => vec![state.toast(format!("Not imported: {e}"))],
        Msg::ImportReplace => match state.dialog.take() {
            // Over the open palette, its edits are asked about before the
            // import is written.
            Some(Dialog::ImportCollision {
                palette,
                notes,
                replace_ok: true,
            }) if state.open_is(&palette.slug) => state.leave(Leave::Import {
                palette: Box::new(palette),
                notes,
            }),
            Some(Dialog::ImportCollision {
                palette,
                notes,
                replace_ok: true,
            }) => vec![Effect::SaveImport {
                palette,
                notes,
                replace: true,
            }],
            other => {
                state.dialog = other;
                Vec::new()
            }
        },
        Msg::ImportKeepBoth => match state.dialog.take() {
            Some(Dialog::ImportCollision {
                mut palette, notes, ..
            }) => {
                // An open draft's slug is taken too, though not yet stored.
                let name = keep_both_name(&palette.name, |slug| {
                    state.entries.iter().any(|e| e.slug == slug) || state.open_is(slug)
                });
                let Some(name) = name else {
                    let text = format!(
                        "Not imported: {0} Two to {0} Ten all exist. Delete one, or import under another name.",
                        palette.name
                    );
                    return vec![state.toast(text)];
                };
                palette.slug = slug_from_name(&name);
                palette.name = name;
                vec![Effect::SaveImport {
                    palette,
                    notes,
                    replace: false,
                }]
            }
            other => {
                state.dialog = other;
                Vec::new()
            }
        },
        Msg::ImportSaved {
            result: Ok(p),
            notes,
        } => {
            let mut lines = vec![format!("Imported {}", p.name)];
            lines.extend(notes);
            let mut effects = vec![state.toast(lines.join("\n")), Effect::ListEntries];
            // A Replace over the open palette asked about its edits before
            // it wrote, so this only asks about an edit made since, or an
            // open draft. A Save then exit the user asked for still exits.
            effects.extend(state.leave_now(Leave::Open(p.slug)));
            effects
        }
        Msg::ImportSaved { result: Err(e), .. } => {
            vec![state.toast(format!("Not imported: {e}"))]
        }
        Msg::Exported(Ok(summary)) => {
            state.dialog = Some(Dialog::Exported(summary));
            Vec::new()
        }
        Msg::Exported(Err(e)) => vec![state.toast(format!("Not exported: {e}"))],
        Msg::ShowWallpaper => {
            state.menu_open = false;
            state.dialog = Some(Dialog::Wallpaper(WallpaperDialog::new()));
            vec![Effect::FindWallpaper(state.wallpaper_system.clone())]
        }
        Msg::WallpaperFound(found) => {
            let Some(d) = state.wallpaper_dialog().filter(|d| d.finding) else {
                return Vec::new();
            };
            d.finding = false;
            // An image the user chose meanwhile wins.
            match found {
                Some(path) if d.reading.is_none() && d.image.is_none() => vec![d.read(path)],
                _ => Vec::new(),
            }
        }
        Msg::ChooseImage => {
            if state.wallpaper_dialog().is_some() {
                vec![Effect::ChooseImage]
            } else {
                Vec::new()
            }
        }
        Msg::Image(result) => {
            let Some(d) = state.wallpaper_dialog() else {
                return Vec::new();
            };
            match result {
                Ok(path) => {
                    d.error = None;
                    vec![d.read(path)]
                }
                Err(e) => {
                    d.error = Some(format!("Could not choose an image: {e}"));
                    Vec::new()
                }
            }
        }
        Msg::DraftReady {
            path,
            clusters,
            mode,
        } => {
            let Some(d) = state
                .wallpaper_dialog()
                .filter(|d| d.reading.as_ref() == Some(&path))
            else {
                return Vec::new();
            };
            d.reading = None;
            d.error = None;
            if !d.name_edited {
                d.name = name_from_file(&path);
            }
            d.image = Some(path);
            d.clusters = clusters;
            d.redraft(mode);
            Vec::new()
        }
        Msg::DraftFailed { path, error } => {
            // The last good draft, if any, stays.
            if let Some(d) = state
                .wallpaper_dialog()
                .filter(|d| d.reading.as_ref() == Some(&path))
            {
                d.reading = None;
                d.error = Some(error);
            }
            Vec::new()
        }
        Msg::WallpaperMode(mode) => {
            if let Some(d) = state.wallpaper_dialog().filter(|d| d.roles.is_some()) {
                d.redraft(mode);
            }
            Vec::new()
        }
        Msg::WallpaperName(name) => {
            if let Some(d) = state.wallpaper_dialog() {
                d.name = name;
                d.name_edited = true;
            }
            Vec::new()
        }
        Msg::WallpaperCreate => {
            if !state.wallpaper_ready() {
                return Vec::new();
            }
            let Some(Dialog::Wallpaper(d)) = &state.dialog else {
                return Vec::new();
            };
            let p = draft_palette(&d.clusters, d.mode, &d.name);
            state.leave(Leave::Draft(p))
        }
        Msg::Versions { slug, result } => {
            if !state.open_is(&slug) {
                return Vec::new();
            }
            let effects = match result {
                Ok(versions) => {
                    state.versions = versions;
                    Vec::new()
                }
                Err(e) => {
                    state.versions.clear();
                    vec![state.toast(format!("Could not read the saved versions of {slug}: {e}"))]
                }
            };
            // The shown version must still exist.
            match state.split {
                Split::Saved(_) if state.versions.is_empty() => state.split = Split::Off,
                Split::Saved(VersionRef::Version(n))
                    if !state.versions.iter().any(|v| v.n == n) =>
                {
                    state.split = Split::Saved(VersionRef::Latest);
                }
                _ => {}
            }
            effects
        }
        Msg::PairChip => {
            if !state.header_enabled(HeaderItem::Pair) {
                return Vec::new();
            }
            let Some((saved, working)) = state.stored_palette() else {
                return Vec::new();
            };
            if working.pair.is_some() {
                if state.split == Split::Pair {
                    state.split = Split::Off;
                    state.pair_palette = None;
                    return Vec::new();
                }
                let effect = Effect::LoadPair(working.clone());
                state.split = Split::Pair;
                state.pair_palette = None;
                return vec![effect];
            }
            // The store pairs by the saved mode.
            if working.mode != saved.mode {
                return vec![state.toast("Save the mode change first (Ctrl+S), then pair.")];
            }
            let mode = saved.mode;
            let choices = state.pair_choices(mode);
            if choices.is_empty() {
                let other = match mode {
                    Mode::Dark => Mode::Light,
                    Mode::Light => Mode::Dark,
                };
                return vec![state.toast(format!(
                    "There is no {other} palette to pair with. Make one with New."
                ))];
            }
            state.dialog = Some(Dialog::PairWith {
                choices,
                confirm: None,
            });
            Vec::new()
        }
        Msg::PairLoaded { slug, lookup } => {
            if !state.open_is(&slug) || state.split != Split::Pair {
                return Vec::new();
            }
            let text = match *lookup {
                PairLookup::Found(p) => {
                    state.pair_palette = Some(p);
                    return Vec::new();
                }
                PairLookup::None => "It has no pair to show.".to_string(),
                PairLookup::SameMode(pair) => {
                    format!("Its pair {pair} has the same mode, so there is no other half to show.")
                }
                PairLookup::Missing { slug, reason } => {
                    format!("Its pair {slug} cannot be loaded ({reason}).")
                }
            };
            state.split = Split::Off;
            vec![state.toast(text)]
        }
        Msg::PairChoose(with) => {
            let Some(Dialog::PairWith { choices, confirm }) = &mut state.dialog else {
                return Vec::new();
            };
            let Some(choice) = choices.iter().find(|c| c.slug == with) else {
                return Vec::new();
            };
            if choice.paired_with.is_some() {
                *confirm = Some(with);
                return Vec::new();
            }
            link(state, with)
        }
        Msg::PairConfirm => match &state.dialog {
            Some(Dialog::PairWith {
                confirm: Some(with),
                ..
            }) => {
                let with = with.clone();
                link(state, with)
            }
            _ => Vec::new(),
        },
        Msg::Linked { slug, with, result } => match result {
            Ok(()) => {
                // The entries still hold the pairs from before the link.
                let name = state.entry_label(&slug);
                let mut text = format!("Paired {name} with {}", state.entry_label(&with));
                let old = state
                    .entries
                    .iter()
                    .find(|e| e.slug == with)
                    .and_then(|e| e.pair.clone())
                    .filter(|old| *old != slug);
                if let Some(old) = old {
                    text.push_str(&format!(
                        ". {} is no longer paired.",
                        state.entry_label(&old)
                    ));
                }
                state.set_pair(&slug, Some(with));
                let mut effects = vec![state.toast(text), Effect::ListEntries];
                // Live writes the pair too.
                if state.open_is(&slug) {
                    effects.extend(state.live_now());
                }
                effects
            }
            Err(e) => vec![state.toast(format!("Not paired: {e}"))],
        },
        Msg::Unpair => {
            state.menu_open = false;
            if !state.menu_enabled(MenuItem::Unpair) {
                return Vec::new();
            }
            let slug = state.current.as_ref().map(|c| c.slug().to_string());
            slug.map(Effect::UnlinkPair).into_iter().collect()
        }
        Msg::Unlinked { slug, result } => match result {
            Ok(()) => {
                let partner = state
                    .current
                    .as_ref()
                    .filter(|c| c.slug() == slug)
                    .and_then(Current::working)
                    .and_then(|w| w.pair.clone());
                let mut text = format!("Unpaired {}", state.entry_label(&slug));
                if let Some(p) = partner {
                    text.push_str(&format!(" and {}", state.entry_label(&p)));
                }
                state.set_pair(&slug, None);
                if state.open_is(&slug) && state.split == Split::Pair {
                    state.split = Split::Off;
                    state.pair_palette = None;
                }
                let mut effects = vec![state.toast(text), Effect::ListEntries];
                if state.open_is(&slug) {
                    effects.extend(state.live_now());
                }
                effects
            }
            Err(e) => vec![state.toast(format!("Not unpaired: {e}"))],
        },
        Msg::Compare => {
            if !state.header_enabled(HeaderItem::Compare) {
                return Vec::new();
            }
            if matches!(state.split, Split::Saved(_)) {
                state.split = Split::Off;
            } else {
                state.split = Split::Saved(VersionRef::Latest);
                state.pair_palette = None;
            }
            Vec::new()
        }
        Msg::CompareVersion(i) => {
            if !matches!(state.split, Split::Saved(_)) {
                return Vec::new();
            }
            let (Some(v), Some(current)) = (state.versions.get(i), &state.current) else {
                return Vec::new();
            };
            let (n, slug) = (v.n, current.slug().to_string());
            state.split = Split::Saved(VersionRef::Version(n));
            if matches!(&state.compare_version, Some((k, Ok(_))) if *k == n) {
                return Vec::new();
            }
            state.compare_version = None;
            vec![Effect::LoadVersion { slug, n }]
        }
        Msg::VersionLoaded { slug, n, result } => {
            if state.open_is(&slug) && state.split == Split::Saved(VersionRef::Version(n)) {
                let result = result.map_err(|e| format!("Could not read version {n}: {e}"));
                state.compare_version = Some((n, result));
            }
            Vec::new()
        }
        Msg::ShowPinSnapshot => {
            let Some(n) = state.shown_version() else {
                return Vec::new();
            };
            let name = state
                .versions
                .iter()
                .find(|v| v.n == n)
                .and_then(|v| v.snapshot.clone())
                .unwrap_or_default();
            state.dialog = Some(Dialog::Snapshot { n, name });
            Vec::new()
        }
        Msg::SnapshotInput(text) => {
            if let Some(Dialog::Snapshot { name, .. }) = &mut state.dialog {
                *name = text;
            }
            Vec::new()
        }
        Msg::SnapshotConfirm => {
            let (Some(Dialog::Snapshot { n, .. }), Ok(name)) =
                (&state.dialog, state.snapshot_check())
            else {
                return Vec::new();
            };
            let n = *n;
            let Some(slug) = state.current.as_ref().map(|c| c.slug().to_string()) else {
                return Vec::new();
            };
            state.dialog = None;
            vec![Effect::PinSnapshot { slug, n, name }]
        }
        Msg::UnpinSnapshot => {
            let Some(n) = state.shown_version() else {
                return Vec::new();
            };
            let named = state
                .versions
                .iter()
                .any(|v| v.n == n && v.snapshot.is_some());
            match &state.current {
                Some(c) if named => vec![Effect::UnpinSnapshot {
                    slug: c.slug().to_string(),
                    n,
                }],
                _ => Vec::new(),
            }
        }
        Msg::SnapshotSaved {
            slug,
            n,
            name,
            result,
        } => match result {
            Ok(()) => {
                if state.open_is(&slug)
                    && let Some(v) = state.versions.iter_mut().find(|v| v.n == n)
                {
                    v.snapshot = name.clone();
                }
                let text = match name {
                    Some(name) => format!("Named version {n} {name}"),
                    None => format!("Removed the name of version {n}"),
                };
                vec![state.toast(text)]
            }
            Err(e) if name.is_some() => vec![state.toast(format!("Snapshot not saved: {e}"))],
            Err(e) => vec![state.toast(format!("Could not remove the snapshot name: {e}"))],
        },
        Msg::LiveToggle(true) => {
            if state.live.is_some() || !state.header_enabled(HeaderItem::TryLive) {
                return Vec::new();
            }
            let slug = state.current.as_ref().map(|c| c.slug().to_string());
            let Some(slug) = slug else {
                return Vec::new();
            };
            state.live = Some(Live::new(slug.clone(), None, false));
            vec![Effect::StartLive(slug)]
        }
        Msg::LiveToggle(false) => {
            if state.live.as_ref().is_some_and(|l| l.end.is_none()) {
                state.dialog = Some(Dialog::KeepOrRevert {
                    then: None,
                    pending: false,
                });
            }
            Vec::new()
        }
        Msg::LiveStarted(Ok(session)) => {
            let Some(live) = state.live.as_mut().filter(|l| l.session.is_none()) else {
                return Vec::new();
            };
            live.session = Some(session);
            live.busy = false;
            // The open palette shows live at once.
            live.queued = true;
            state.live_next()
        }
        Msg::LiveStarted(Err(e)) => {
            let then = state
                .live
                .take()
                .and_then(|l| l.end)
                .and_then(|(_, then)| then);
            let mut effects = vec![state.toast(format!("Could not start Try it live: {e}"))];
            effects.extend(state.live_then(then));
            effects
        }
        Msg::LiveTick(generation) => {
            match state.live_for_edits() {
                Some(live) if live.generation == generation => live.queued = true,
                _ => return Vec::new(),
            }
            state.live_next()
        }
        Msg::LiveApplied { session, result } => {
            let Some(live) = state.live.as_mut() else {
                return Vec::new();
            };
            live.session = Some(session);
            live.busy = false;
            let mut effects = Vec::new();
            if let Err(failure) = result {
                // Sent again with the next edit.
                live.applied = None;
                effects.push(state.toast(live_failure_text(&failure)));
            }
            effects.extend(state.live_next());
            effects
        }
        Msg::LiveKeep | Msg::LiveRevert => {
            let Some(Dialog::KeepOrRevert { then, .. }) = state.dialog.clone() else {
                return Vec::new();
            };
            let Some(live) = state.live.as_mut().filter(|l| l.end.is_none()) else {
                state.dialog = None;
                return Vec::new();
            };
            let end = if matches!(msg, Msg::LiveKeep) {
                LiveEnd::Keep
            } else {
                LiveEnd::Revert
            };
            live.end = Some((end, then));
            state.dialog = None;
            state.live_next()
        }
        Msg::LiveKept(Ok(())) => {
            let then = state
                .live
                .take()
                .and_then(|l| l.end)
                .and_then(|(_, then)| then);
            let mut effects =
                vec![state.toast(
                    "Kept the live colours. Undo last apply puts back what was there before.",
                )];
            effects.extend(state.live_then(then));
            effects
        }
        Msg::LiveKept(Err(e)) => {
            state.live_end_failed();
            vec![state.toast(format!(
                "Could not end Try it live: {e}. Turn it off to try again."
            ))]
        }
        Msg::LiveReverted(Ok(done)) if done.as_ref().is_none_or(|d| d.failures.is_empty()) => {
            let then = state
                .live
                .take()
                .and_then(|l| l.end)
                .and_then(|(_, then)| then);
            let mut effects = vec![state.toast("Reverted the live colours")];
            effects.extend(state.live_then(then));
            effects
        }
        Msg::LiveReverted(result) => {
            // Files that could not be put back keep the session open, so
            // Revert is offered again (here, and at the next start).
            state.live_end_failed();
            let mut lines = match result {
                Ok(done) => done.map(|d| d.failures).unwrap_or_default(),
                Err(e) => vec![format!("Could not revert: {e}")],
            };
            lines.push("Turn Try it live off to try Revert again.".to_string());
            vec![state.toast(lines.join("\n"))]
        }
        Msg::LivePending(Some(session)) => {
            if state.live.is_some() {
                return Vec::new();
            }
            let slug = session.palette().to_string();
            state.live = Some(Live::new(slug, Some(session), true));
            state.menu_open = false;
            state.dialog = Some(Dialog::KeepOrRevert {
                then: None,
                pending: true,
            });
            Vec::new()
        }
        Msg::LivePending(None) => Vec::new(),
        Msg::LiveTargets(any) => {
            state.live_targets = any;
            Vec::new()
        }
        Msg::Failed(e) => vec![state.toast(e)],
    }
}

/// Set `role` in the working copy, as typed or picked.
fn set_role(state: &mut State, role: Role, value: Rgb) {
    if let Some(working) = state.working_mut() {
        working.roles.set(role, value);
        state.fields.remove(&Field::Hex(role));
    }
}

/// A target that reloads live is installed and can be applied.
pub fn any_live_target(targets: &[TargetInfo]) -> bool {
    targets
        .iter()
        .any(|t| t.live && t.detected && t.broken.is_none())
}

/// The toast for a live apply that failed. A write that could not be
/// rolled back is put back by Revert, not by Undo.
fn live_failure_text(failure: &ApplyFailure) -> String {
    match failure {
        ApplyFailure::Write {
            path,
            reason,
            rolled_back: false,
        } => format!(
            "Could not write {}: {reason}. Turn Try it live off and Revert to put your files back.",
            path.display()
        ),
        other => format!("Not applied live: {}", other.text()),
    }
}

/// Pair the open palette with `with`, closing the dialog.
fn link(state: &mut State, with: String) -> Vec<Effect> {
    state.dialog = None;
    match &state.current {
        Some(c) => vec![Effect::LinkPair {
            slug: c.slug().to_string(),
            with,
        }],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use irida_core::apply::{SkipReason, TargetResult};
    use irida_core::dirs::Dirs;
    use irida_core::export::{BY_HAND, ExportStatus};
    use irida_core::store::Store;
    use tempfile::TempDir;

    use cosmic::cosmic_theme::palette::{FromColor, Hsv, Srgb};
    use irida_core::color::{WHITE, mix};
    use irida_core::derive::derive;
    use irida_core::slot::{Ansi, PinValue, Slot, Tone};

    use super::*;
    use crate::app::effects::{drive, drive_effects, panicked, perform};

    fn ctx(tmp: &TempDir) -> Ctx {
        Ctx {
            dirs: Dirs::under(tmp.path()),
            path: vec![],
        }
    }

    fn starter(slug: &str) -> Palette {
        starters()
            .into_iter()
            .find(|p| p.slug == slug)
            .expect("a starter")
    }

    /// A store holding these starters, each saved as version 1.
    fn store_with(tmp: &TempDir, slugs: &[&str]) -> Store {
        let store = Store::new(Dirs::under(tmp.path()));
        for slug in slugs {
            store.save_version(&starter(slug)).unwrap();
        }
        store
    }

    /// A started app on the store in `tmp`, with every start effect run. It
    /// takes the app's lock, so a second one while it is open is another
    /// window.
    fn start(tmp: &TempDir) -> State {
        let lock = irida_core::live::lock_app(&Dirs::under(tmp.path()));
        let (mut state, effects) = State::new(ctx(tmp), tmp.path().join("system"), lock);
        drive_effects(&mut state, effects);
        state
    }

    fn open_slug(state: &State) -> Option<&str> {
        state.current.as_ref().map(Current::slug)
    }

    fn working(state: &State) -> &Palette {
        state
            .current
            .as_ref()
            .and_then(Current::working)
            .expect("an editable palette")
    }

    fn last_toast(state: &State) -> &str {
        &state.toasts.last().expect("a toast").text
    }

    fn red() -> Rgb {
        Rgb::from_hex("#FF0000").unwrap()
    }

    fn palettes_dir(tmp: &TempDir) -> PathBuf {
        Dirs::under(tmp.path()).data.join("irida/palettes")
    }

    /// Every file under the palettes folder with its bytes, sorted by path.
    fn snapshot(tmp: &TempDir) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            let Ok(rd) = fs::read_dir(dir) else { return };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    out.push((p.clone(), fs::read(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(&palettes_dir(tmp), &mut out);
        out.sort();
        out
    }

    #[test]
    fn starts_on_last_palette() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "noir"]);
        store
            .save_settings(&Settings {
                last_palette: Some("noir".to_string()),
                ..Settings::default()
            })
            .unwrap();
        let state = start(&tmp);
        assert_eq!(open_slug(&state), Some("noir"));
        assert!(!state.dirty());
        assert_eq!(state.entries.len(), 2);
        assert_eq!(store.list().len(), 2, "nothing seeded");
    }

    #[test]
    fn starts_on_first_when_none() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["noir", "ember"]);
        let state = start(&tmp);
        assert_eq!(open_slug(&state), Some("ember"));
        assert_eq!(
            store.settings().unwrap().last_palette.as_deref(),
            Some("ember"),
            "opening writes the last palette"
        );

        // A last palette that no longer exists falls back to the first.
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["noir", "ember"]);
        store
            .save_settings(&Settings {
                last_palette: Some("gone".to_string()),
                ..Settings::default()
            })
            .unwrap();
        assert_eq!(open_slug(&start(&tmp)), Some("ember"));

        // An unreadable first entry is skipped for the first readable one.
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["noir"]);
        fs::create_dir_all(palettes_dir(&tmp).join("aaa")).unwrap();
        fs::write(palettes_dir(&tmp).join("aaa/palette.toml"), "name = 1\n").unwrap();
        assert_eq!(open_slug(&start(&tmp)), Some("noir"));
    }

    #[test]
    fn empty_store_is_seeded() {
        let tmp = TempDir::new().unwrap();
        let state = start(&tmp);
        let slugs: Vec<&str> = state.entries.iter().map(|e| e.slug.as_str()).collect();
        assert_eq!(slugs, ["dossier", "ember", "emberday", "noir"]);
        assert!(state.entries.iter().all(EntrySummary::readable));
        assert_eq!(open_slug(&state), Some("dossier"));
        assert!(!state.dirty());
    }

    #[test]
    fn empty_area_says_choose_when_the_store_has_palettes() {
        // The only palette fails to open at the IO level, so nothing is open.
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["noir"]);
        fs::write(palettes_dir(&tmp).join("noir/palette.toml"), [0xff, 0xfe]).unwrap();
        let state = start(&tmp);
        assert!(state.current.is_none());
        assert!(!state.entries.is_empty());
        assert_eq!(
            state.empty_text(),
            "Choose a palette from the palette menu."
        );

        let mut state = state;
        state.entries.clear();
        assert_eq!(
            state.empty_text(),
            "No palettes yet. Use New in the palette menu to make one."
        );
    }

    #[test]
    fn unreadable_settings_are_kept() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let path = Dirs::under(tmp.path()).config.join("irida/settings.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "disabled = 3\n").unwrap();
        let state = start(&tmp);
        assert_eq!(open_slug(&state), Some("ember"));
        assert!(state.settings_unreadable);
        assert_eq!(state.settings, Settings::default());
        assert!(last_toast(&state).starts_with("Could not read your settings"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "disabled = 3\n");
    }

    #[test]
    fn edit_marks_dirty_and_save_clears() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        assert!(!state.dirty());
        assert_eq!(state.title(), "Ember");
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        assert!(state.dirty());
        assert_eq!(state.title(), "Ember \u{2022}");
        assert_eq!(working(&state).roles.get(Role::Accent), red());

        let ui = drive(&mut state, Msg::Save);
        assert!(!state.dirty());
        assert_eq!(state.title(), "Ember");
        assert!(matches!(ui.as_slice(), [Effect::Toast(t)] if t.text == "Saved version 2"));
        let versions = store.versions("ember").unwrap();
        assert_eq!(versions.len(), 2);
        let saved = store.load_version("ember", 2).unwrap();
        assert_eq!(saved.roles.get(Role::Accent), red());
        assert_eq!(store.load("ember").unwrap(), saved);

        // A clean palette is not saved again.
        assert!(update(&mut state, Msg::Save).is_empty());
        assert_eq!(store.versions("ember").unwrap().len(), 2);

        // A second Ctrl+S while a save runs waits for it.
        drive(
            &mut state,
            Msg::SetRole(Role::Accent, Rgb::from_hex("#00FF00").unwrap()),
        );
        let first = update(&mut state, Msg::Save);
        assert!(matches!(first.as_slice(), [Effect::SaveVersion { .. }]));
        assert!(update(&mut state, Msg::Save).is_empty());
        drive_effects(&mut state, first);
        assert!(!state.dirty());
        assert_eq!(store.versions("ember").unwrap().len(), 3);

        // The mode chip edits too.
        drive(&mut state, Msg::SetMode(Mode::Light));
        assert!(state.dirty());
        assert_eq!(working(&state).mode, Mode::Light);
    }

    #[test]
    fn save_invalid_changes_nothing() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        let before = snapshot(&tmp);
        if let Some(Current::Editable { working, .. }) = &mut state.current {
            working.name = "Bad-Name".to_string();
        }
        assert!(state.dirty());
        let ui = drive(&mut state, Msg::Save);
        assert!(matches!(ui.as_slice(), [Effect::Toast(_)]));
        assert!(
            last_toast(&state).contains("Bad-Name"),
            "{}",
            last_toast(&state)
        );
        assert!(state.dirty(), "the edit stays");
        assert_eq!(snapshot(&tmp), before);

        // The store refuses it too, should it get that far.
        let p = working(&state).clone();
        let msgs = perform(
            Effect::SaveVersion {
                palette: p,
                fresh: false,
            },
            &state.ctx,
        );
        assert!(matches!(msgs.as_slice(), [Msg::Saved(Err(_))]));
        assert_eq!(snapshot(&tmp), before);
    }

    #[test]
    fn read_only_blocks_edits() {
        let tmp = TempDir::new().unwrap();
        let text = "name = \"Broken\"\nslug = \"broken\"\nmode = \"dusk\"\n";
        fs::create_dir_all(palettes_dir(&tmp).join("broken")).unwrap();
        fs::write(palettes_dir(&tmp).join("broken/palette.toml"), text).unwrap();
        let mut state = start(&tmp);
        let Some(Current::Unreadable {
            slug,
            error,
            text: shown,
        }) = &state.current
        else {
            panic!("opens read-only: {:?}", state.current);
        };
        assert_eq!(slug, "broken");
        assert!(!error.is_empty());
        assert_eq!(shown, text);
        assert!(!state.dirty());

        let before = snapshot(&tmp);
        assert!(update(&mut state, Msg::SetRole(Role::Accent, red())).is_empty());
        assert!(update(&mut state, Msg::SetMode(Mode::Light)).is_empty());
        assert!(update(&mut state, Msg::Save).is_empty());
        assert!(update(&mut state, Msg::ShowDuplicate).is_empty());
        assert!(state.dialog.is_none());
        let bright_red = Slot::Ansi(Tone::Bright, Ansi::Red);
        for msg in [
            Msg::Expand(Some(Role::Accent)),
            Msg::Picker(ColorPickerUpdate::ActionFinished),
            Msg::HexInput(Role::Accent, "#00FF00".to_string()),
            Msg::HexInput(Role::Accent, "nope".to_string()),
            Msg::SetName(Role::Accent, "Flame".to_string()),
            Msg::Pin(bright_red),
            Msg::Unpin(bright_red),
            Msg::PinHexInput(bright_red, "#00FF00".to_string()),
            Msg::NoBackground(true),
            Msg::SetNotes("notes".to_string()),
        ] {
            assert!(update(&mut state, msg.clone()).is_empty(), "{msg:?}");
        }
        assert_eq!(state.expanded, None);
        assert!(state.fields.is_empty());
        assert_eq!(snapshot(&tmp), before);

        for item in MenuItem::ALL {
            let enabled = matches!(
                item,
                MenuItem::Switch
                    | MenuItem::New
                    | MenuItem::NewFromWallpaper
                    | MenuItem::Import
                    | MenuItem::Delete
            );
            assert_eq!(state.menu_enabled(item), enabled, "{item:?}");
        }
        assert!(!state.header_enabled(HeaderItem::Apply));
        assert!(!state.header_enabled(HeaderItem::Mode));

        // A file whose slug differs from its folder opens read-only too.
        let tmp = TempDir::new().unwrap();
        let mut p = starter("noir");
        p.slug = "other".to_string();
        fs::create_dir_all(palettes_dir(&tmp).join("noir")).unwrap();
        fs::write(palettes_dir(&tmp).join("noir/palette.toml"), p.to_toml()).unwrap();
        let state = start(&tmp);
        assert!(
            matches!(&state.current, Some(Current::Unreadable { slug, .. }) if slug == "noir"),
            "{:?}",
            state.current
        );
    }

    #[test]
    fn new_duplicate_delete() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "noir"]);
        let mut state = start(&tmp);
        assert_eq!(open_slug(&state), Some("ember"));

        // New "Moss", dark.
        drive(&mut state, Msg::ShowNew);
        assert!(matches!(
            state.dialog,
            Some(Dialog::Name {
                purpose: NameFor::New,
                ..
            })
        ));
        assert_eq!(state.name_check(""), Err(String::new()));
        assert!(state.name_check("Bad-Name").is_err());
        assert!(state.name_check("2024").is_err());
        assert_eq!(
            state.name_check("Ember"),
            Err("A palette called ember already exists".to_string())
        );
        drive(&mut state, Msg::NameInput("Ember".to_string()));
        assert!(
            drive(&mut state, Msg::NameConfirm).is_empty(),
            "Exists is refused"
        );
        assert!(state.dialog.is_some());
        drive(&mut state, Msg::NameInput("Moss".to_string()));
        assert_eq!(state.name_check("Moss"), Ok("moss".to_string()));
        drive(&mut state, Msg::NameMode(Mode::Dark));
        drive(&mut state, Msg::NameConfirm);
        assert!(state.dialog.is_none());
        assert_eq!(open_slug(&state), Some("moss"));
        assert!(!state.dirty());
        let moss = store.load("moss").unwrap();
        assert_eq!(moss.name, "Moss");
        assert_eq!(moss.mode, Mode::Dark);
        assert_eq!(moss.roles, starter("ember").roles);
        assert!(moss.pins.is_empty() && moss.pair.is_none() && moss.names.is_empty());
        assert_eq!(store.versions("moss").unwrap().len(), 1);
        assert!(state.entries.iter().any(|e| e.slug == "moss"));
        assert_eq!(
            store.settings().unwrap().last_palette.as_deref(),
            Some("moss")
        );

        // Duplicate Ember as "Ember Two".
        drive(&mut state, Msg::ShowSwitch);
        drive(&mut state, Msg::Choose("ember".to_string()));
        assert_eq!(open_slug(&state), Some("ember"));
        drive(&mut state, Msg::ShowDuplicate);
        drive(&mut state, Msg::NameInput("Ember Two".to_string()));
        drive(&mut state, Msg::NameConfirm);
        assert_eq!(open_slug(&state), Some("embertwo"));
        let copy = store.load("embertwo").unwrap();
        assert_eq!(copy.name, "Ember Two");
        assert_eq!(copy.roles, starter("ember").roles);

        // Delete removes its folder and opens another entry.
        drive(&mut state, Msg::ShowDelete);
        assert!(matches!(
            &state.dialog,
            Some(Dialog::Delete { slug, label }) if slug == "embertwo" && label == "Ember Two"
        ));
        drive(&mut state, Msg::ConfirmDelete);
        assert!(!palettes_dir(&tmp).join("embertwo").exists());
        assert!(state.entries.iter().all(|e| e.slug != "embertwo"));
        assert_eq!(open_slug(&state), Some("moss"), "the next entry");
        assert_eq!(last_toast(&state), "Deleted Ember Two");
    }

    #[test]
    fn new_with_unsaved_edit_asks() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::ShowNew);
        drive(&mut state, Msg::NameInput("Moss".to_string()));
        drive(&mut state, Msg::NameConfirm);
        assert!(matches!(
            state.dialog,
            Some(Dialog::Unsaved(Leave::Create(_)))
        ));
        assert!(!store.exists("moss"));
        drive(&mut state, Msg::UnsavedSave);
        assert_eq!(store.versions("ember").unwrap().len(), 2, "saved first");
        assert_eq!(open_slug(&state), Some("moss"));
        assert_eq!(working(&state).roles.get(Role::Accent), red());
    }

    #[test]
    fn switch_asks_when_dirty() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "noir"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));

        // Cancel keeps the edit.
        drive(&mut state, Msg::ShowSwitch);
        assert_eq!(state.dialog, Some(Dialog::Switch));
        drive(&mut state, Msg::Choose("noir".to_string()));
        assert_eq!(
            state.dialog,
            Some(Dialog::Unsaved(Leave::Open("noir".to_string())))
        );
        assert_eq!(open_slug(&state), Some("ember"));
        drive(&mut state, Msg::CloseDialog);
        assert!(state.dialog.is_none());
        assert_eq!(open_slug(&state), Some("ember"));
        assert!(state.dirty());
        assert_eq!(working(&state).roles.get(Role::Accent), red());

        // Discard opens the target and saves nothing.
        drive(&mut state, Msg::ShowSwitch);
        drive(&mut state, Msg::Choose("noir".to_string()));
        drive(&mut state, Msg::UnsavedDiscard);
        assert_eq!(open_slug(&state), Some("noir"));
        assert!(!state.dirty());
        assert_eq!(store.versions("ember").unwrap().len(), 1);
        assert_eq!(store.load("ember").unwrap(), starter("ember"));
        assert_eq!(
            store.settings().unwrap().last_palette.as_deref(),
            Some("noir")
        );

        // Save saves, then opens the target.
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::ShowSwitch);
        drive(&mut state, Msg::Choose("ember".to_string()));
        drive(&mut state, Msg::UnsavedSave);
        assert_eq!(open_slug(&state), Some("ember"));
        assert_eq!(store.versions("noir").unwrap().len(), 2);
        assert_eq!(store.load("noir").unwrap().roles.get(Role::Accent), red());

        // Not dirty: Switch opens at once.
        drive(&mut state, Msg::ShowSwitch);
        drive(&mut state, Msg::Choose("noir".to_string()));
        assert!(state.dialog.is_none());
        assert_eq!(open_slug(&state), Some("noir"));
    }

    #[test]
    fn close_with_unsaved_asks() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);

        // Not dirty: Exit at once.
        assert!(matches!(
            update(&mut state, Msg::CloseRequested).as_slice(),
            [Effect::Exit]
        ));

        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        assert!(update(&mut state, Msg::CloseRequested).is_empty());
        assert_eq!(state.dialog, Some(Dialog::Unsaved(Leave::Exit)));

        // Cancel returns nothing.
        assert!(update(&mut state, Msg::CloseDialog).is_empty());
        assert!(state.dialog.is_none());

        // Discard returns Exit.
        update(&mut state, Msg::CloseRequested);
        assert!(matches!(
            update(&mut state, Msg::UnsavedDiscard).as_slice(),
            [Effect::Exit]
        ));
        assert_eq!(store.versions("ember").unwrap().len(), 1);

        // Save saves, then exits.
        update(&mut state, Msg::CloseRequested);
        let ui = drive(&mut state, Msg::UnsavedSave);
        assert!(matches!(ui.last(), Some(Effect::Exit)), "{ui:?}");
        assert_eq!(store.versions("ember").unwrap().len(), 2);

        // Save while a Ctrl+S is still running waits for it, then exits.
        drive(
            &mut state,
            Msg::SetRole(Role::Accent, Rgb::from_hex("#0000FF").unwrap()),
        );
        let running = update(&mut state, Msg::Save);
        update(&mut state, Msg::CloseRequested);
        assert!(update(&mut state, Msg::UnsavedSave).is_empty());
        let ui = drive_effects(&mut state, running);
        assert!(matches!(ui.last(), Some(Effect::Exit)), "{ui:?}");
        assert_eq!(store.versions("ember").unwrap().len(), 3);

        // An edit made while that save runs is not dropped: it asks again.
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        update(&mut state, Msg::CloseRequested);
        let running = update(&mut state, Msg::UnsavedSave);
        assert!(state.dialog.is_none(), "the dialog closes on Save");
        drive(
            &mut state,
            Msg::SetRole(Role::Accent, Rgb::from_hex("#00FF00").unwrap()),
        );
        let ui = drive_effects(&mut state, running);
        assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
        assert_eq!(state.dialog, Some(Dialog::Unsaved(Leave::Exit)));
        assert_eq!(store.versions("ember").unwrap().len(), 4);
        assert_eq!(store.load("ember").unwrap().roles.get(Role::Accent), red());
        // Saving again saves the edit, then exits.
        let ui = drive(&mut state, Msg::UnsavedSave);
        assert!(matches!(ui.last(), Some(Effect::Exit)), "{ui:?}");
        assert_eq!(store.versions("ember").unwrap().len(), 5);
        assert_eq!(
            store.load("ember").unwrap().roles.get(Role::Accent),
            Rgb::from_hex("#00FF00").unwrap()
        );

        // So is an edit made while a Ctrl+S runs that Save then waits for.
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        let running = update(&mut state, Msg::Save);
        update(&mut state, Msg::CloseRequested);
        assert!(update(&mut state, Msg::UnsavedSave).is_empty());
        drive(
            &mut state,
            Msg::SetRole(Role::Accent, Rgb::from_hex("#0000FF").unwrap()),
        );
        let ui = drive_effects(&mut state, running);
        assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
        assert_eq!(state.dialog, Some(Dialog::Unsaved(Leave::Exit)));
        assert_eq!(store.versions("ember").unwrap().len(), 6);

        // A save that fails does not exit.
        if let Some(Current::Editable { working, .. }) = &mut state.current {
            working.name = "Bad-Name".to_string();
        }
        update(&mut state, Msg::CloseRequested);
        let ui = drive(&mut state, Msg::UnsavedSave);
        assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
    }

    #[test]
    fn newest_unsaved_answer_decides() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember", "noir"]);
        let mut state = start(&tmp);

        // Save on Exit, then Discard on a switch asked while that save runs.
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        update(&mut state, Msg::CloseRequested);
        let running = update(&mut state, Msg::UnsavedSave);
        drive(&mut state, Msg::ShowSwitch);
        drive(&mut state, Msg::Choose("noir".to_string()));
        assert_eq!(
            state.dialog,
            Some(Dialog::Unsaved(Leave::Open("noir".to_string())))
        );
        drive(&mut state, Msg::UnsavedDiscard);
        assert_eq!(open_slug(&state), Some("noir"));
        let ui = drive_effects(&mut state, running);
        assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
        assert_eq!(open_slug(&state), Some("noir"));

        // Save on Exit, then Cancel on a switch asked while that save runs.
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        update(&mut state, Msg::CloseRequested);
        let running = update(&mut state, Msg::UnsavedSave);
        drive(&mut state, Msg::ShowSwitch);
        drive(&mut state, Msg::Choose("ember".to_string()));
        drive(&mut state, Msg::CloseDialog);
        let ui = drive_effects(&mut state, running);
        assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
        assert_eq!(open_slug(&state), Some("noir"));
        assert!(state.dialog.is_none());
    }

    fn derived(state: &State, slot: Slot) -> Option<Rgb> {
        derive(working(state)).slot(slot)
    }

    fn hex(s: &str) -> Rgb {
        Rgb::from_hex(s).unwrap()
    }

    fn row(state: &State, role: Role) -> RoleRow {
        role_rows(working(state))
            .into_iter()
            .find(|r| r.role == role)
            .expect("a row")
    }

    #[test]
    fn set_role_updates_ratios() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        let before = row(&state, Role::Error);
        let rc = before.contrast.clone().expect("error has a ratio");
        assert!(rc.fails_on_pane(), "{rc:?}");
        assert!(before.fails());
        drive(&mut state, Msg::SetRole(Role::Error, hex("#E06060")));
        let after = row(&state, Role::Error);
        let rc = after.contrast.clone().unwrap();
        assert!(!rc.fails_on_pane(), "{rc:?}");
        assert!(!after.fails());
        assert_eq!(after.color, hex("#E06060"));
        let lower = rc.on_background.min(rc.on_pane);
        assert_eq!(after.ratio_label(), Some(format!("{lower:.1}:1")));

        // Surfaces show no ratio; the rows follow Role::ALL.
        let rows = role_rows(working(&state));
        let roles: Vec<Role> = rows.iter().map(|r| r.role).collect();
        assert_eq!(roles, Role::ALL);
        for r in &rows {
            let surface = matches!(r.role, Role::Background | Role::Pane | Role::Container);
            assert_eq!(r.ratio_label().is_none(), surface, "{:?}", r.role);
        }
        // The display name is the [names] entry, else the capitalised role.
        assert_eq!(row(&state, Role::Error).name, "Brick");
        drive(&mut state, Msg::SetName(Role::Error, String::new()));
        assert_eq!(row(&state, Role::Error).name, "Error");
    }

    #[test]
    fn pin_keeps_value_when_role_changes() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        let br = Slot::Ansi(Tone::Bright, Ansi::Red);
        let before = derived(&state, br).unwrap();
        drive(&mut state, Msg::Pin(br));
        assert_eq!(working(&state).pins.get(br), Some(PinValue::Color(before)));
        assert!(state.dirty());

        drive(&mut state, Msg::SetRole(Role::Error, hex("#E06060")));
        assert_eq!(derived(&state, br), Some(before), "the pin holds");
        // Pinning again keeps the pinned value.
        drive(&mut state, Msg::Pin(br));
        assert_eq!(derived(&state, br), Some(before));

        // A pinned slot's hex is editable.
        drive(&mut state, Msg::PinHexInput(br, "#123456".to_string()));
        assert_eq!(derived(&state, br), Some(hex("#123456")));
        drive(&mut state, Msg::PinHexInput(br, "#1234".to_string()));
        assert_eq!(derived(&state, br), Some(hex("#123456")));
        assert_eq!(
            state
                .field(Field::PinHex(br))
                .and_then(|f| f.error.as_deref()),
            Some(HEX_ERROR)
        );

        drive(&mut state, Msg::Unpin(br));
        assert!(!working(&state).pins.is_pinned(br));
        assert_eq!(
            derived(&state, br),
            Some(mix(hex("#E06060"), WHITE, 0.12)),
            "it follows error again"
        );
        assert!(state.field(Field::PinHex(br)).is_none());

        // An unpinned slot's hex cannot be typed.
        drive(&mut state, Msg::PinHexInput(br, "#123456".to_string()));
        assert!(!working(&state).pins.is_pinned(br));
    }

    #[test]
    fn no_background_pins_none() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::NoBackground(true));
        assert_eq!(
            working(&state).pins.get(Slot::Background),
            Some(PinValue::None)
        );
        assert_eq!(derived(&state, Slot::Background), None);
        // A background pinned to none cannot be pinned to a colour by Pin.
        drive(&mut state, Msg::Pin(Slot::Background));
        assert_eq!(
            working(&state).pins.get(Slot::Background),
            Some(PinValue::None)
        );
        drive(&mut state, Msg::NoBackground(false));
        assert_eq!(working(&state).pins.get(Slot::Background), None);
        assert!(!state.dirty(), "back to the saved file");

        // Off leaves a colour pin alone.
        drive(&mut state, Msg::Pin(Slot::Background));
        let pane = working(&state).roles.get(Role::Pane);
        assert_eq!(
            working(&state).pins.get(Slot::Background),
            Some(PinValue::Color(pane))
        );
        drive(&mut state, Msg::NoBackground(false));
        assert_eq!(
            working(&state).pins.get(Slot::Background),
            Some(PinValue::Color(pane))
        );
        // And the palette still saves.
        drive(&mut state, Msg::NoBackground(true));
        let ui = drive(&mut state, Msg::Save);
        assert!(matches!(ui.as_slice(), [Effect::Toast(t)] if t.text == "Saved version 2"));
    }

    #[test]
    fn hex_input_rules() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::Expand(Some(Role::Accent)));
        assert_eq!(state.expanded, Some(Role::Accent));
        let want = hex("#E8713A");
        for input in ["e8713a", "#E8713A", "E8713A", "#e8713A"] {
            drive(&mut state, Msg::SetRole(Role::Accent, red()));
            drive(&mut state, Msg::HexInput(Role::Accent, input.to_string()));
            assert_eq!(working(&state).roles.get(Role::Accent), want, "{input}");
            let f = state.field(Field::Hex(Role::Accent)).expect("the field");
            assert_eq!(f.text, input);
            assert_eq!(f.error, None);
            assert_eq!(picker_rgb(&state), Some(want), "the picker follows");
        }
        for input in ["#E871", "", "#E8713G", "#E8713A0"] {
            drive(&mut state, Msg::HexInput(Role::Accent, input.to_string()));
            assert_eq!(working(&state).roles.get(Role::Accent), want, "{input:?}");
            let f = state.field(Field::Hex(Role::Accent)).unwrap();
            assert_eq!(f.text, input);
            assert_eq!(f.error.as_deref(), Some(HEX_ERROR));
        }
        assert_eq!(HEX_ERROR, "Use six hex digits, like #E8713A");
        // A valid entry clears the error.
        drive(
            &mut state,
            Msg::HexInput(Role::Accent, "#FF0000".to_string()),
        );
        assert_eq!(state.field(Field::Hex(Role::Accent)).unwrap().error, None);
        assert_eq!(working(&state).roles.get(Role::Accent), red());
    }

    fn picker_rgb(state: &State) -> Option<Rgb> {
        state.picker.get_applied_color().map(rgb_from_color)
    }

    #[test]
    fn picker_sets_role() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        // Nothing expanded: the picker changes nothing.
        assert!(drive(&mut state, Msg::Picker(ColorPickerUpdate::ActionFinished)).is_empty());
        assert!(!state.dirty());

        drive(&mut state, Msg::Expand(Some(Role::Blue)));
        let blue = working(&state).roles.get(Role::Blue);
        assert_eq!(picker_rgb(&state), Some(blue), "rebuilt on Expand");

        // A drag shows at once; its end keeps it.
        let green = Hsv::from_color(Srgb::new(0.0_f32, 1.0, 0.0));
        drive(
            &mut state,
            Msg::Picker(ColorPickerUpdate::ActiveColor(green)),
        );
        assert_eq!(working(&state).roles.get(Role::Blue), hex("#00FF00"));
        drive(&mut state, Msg::Picker(ColorPickerUpdate::ActionFinished));
        assert_eq!(working(&state).roles.get(Role::Blue), hex("#00FF00"));
        assert_eq!(picker_rgb(&state), Some(hex("#00FF00")));
        assert!(state.dirty());

        // Typing in the picker's own field applies on submit.
        drive(
            &mut state,
            Msg::Picker(ColorPickerUpdate::Input("#0000FF".to_string())),
        );
        assert_eq!(working(&state).roles.get(Role::Blue), hex("#00FF00"));
        drive(&mut state, Msg::Picker(ColorPickerUpdate::AppliedColor));
        assert_eq!(working(&state).roles.get(Role::Blue), hex("#0000FF"));

        // Copy hands the hex to the window.
        let ui = drive(
            &mut state,
            Msg::Picker(ColorPickerUpdate::Copied(std::time::Instant::now())),
        );
        assert!(
            matches!(ui.as_slice(), [Effect::Copy(t)] if t == "#0000FF"),
            "{ui:?}"
        );
        assert!(!Effect::Copy(String::new()).is_io());

        // Expanding another row rebuilds the picker on its colour.
        drive(&mut state, Msg::Expand(Some(Role::Cyan)));
        let cyan = working(&state).roles.get(Role::Cyan);
        assert_eq!(picker_rgb(&state), Some(cyan));
        drive(&mut state, Msg::Expand(None));
        assert_eq!(state.expanded, None);
        drive(
            &mut state,
            Msg::Picker(ColorPickerUpdate::ActiveColor(green)),
        );
        assert_eq!(working(&state).roles.get(Role::Cyan), cyan);
    }

    #[test]
    fn opening_another_palette_collapses() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember", "noir"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::Expand(Some(Role::Accent)));
        drive(&mut state, Msg::HexInput(Role::Accent, "#12".to_string()));
        drive(&mut state, Msg::ShowSwitch);
        drive(&mut state, Msg::Choose("noir".to_string()));
        assert_eq!(open_slug(&state), Some("noir"));
        assert_eq!(state.expanded, None);
        assert!(state.fields.is_empty());
    }

    #[test]
    fn empty_name_clears() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        assert_eq!(
            working(&state).names.get(&Role::Accent).map(String::as_str),
            Some("Ember")
        );
        drive(&mut state, Msg::SetName(Role::Accent, "  ".to_string()));
        assert_eq!(working(&state).names.get(&Role::Accent), None);
        drive(&mut state, Msg::SetName(Role::Accent, String::new()));
        assert_eq!(working(&state).names.get(&Role::Accent), None);

        // Typed as is; trimmed when saved.
        drive(&mut state, Msg::SetName(Role::Accent, "Hot ".to_string()));
        assert_eq!(
            working(&state).names.get(&Role::Accent).map(String::as_str),
            Some("Hot ")
        );
        drive(&mut state, Msg::Save);
        assert!(!state.dirty());
        let saved = store.load("ember").unwrap();
        assert_eq!(
            saved.names.get(&Role::Accent).map(String::as_str),
            Some("Hot")
        );
    }

    #[test]
    fn notes_set_and_clear() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::SetNotes("Warm, low glare".to_string()));
        assert_eq!(working(&state).notes.as_deref(), Some("Warm, low glare"));
        drive(&mut state, Msg::SetNotes(" ".to_string()));
        assert_eq!(working(&state).notes, None);
        drive(&mut state, Msg::SetNotes(String::new()));
        assert_eq!(working(&state).notes, None);
        assert!(!state.dirty());
    }

    #[test]
    fn every_pin_is_reachable() {
        for p in starters() {
            let reachable: Vec<Slot> = Role::ALL
                .iter()
                .flat_map(|r| role_slots(*r, p.mode))
                .collect();
            for (slot, _) in p.pins.iter() {
                assert!(reachable.contains(&slot), "{} {slot}", p.slug);
            }
        }
    }

    #[test]
    fn every_slot_under_one_role() {
        let all: Vec<Slot> = Slot::ALL_TERM
            .into_iter()
            .chain([Slot::CosmicNeutral10, Slot::CosmicGray2])
            .collect();
        for mode in [Mode::Dark, Mode::Light] {
            for slot in &all {
                let owners: Vec<Role> = Role::ALL
                    .into_iter()
                    .filter(|r| role_slots(*r, mode).contains(slot))
                    .collect();
                assert_eq!(owners.len(), 1, "{mode} {slot}: {owners:?}");
            }
            let count: usize = Role::ALL.iter().map(|r| role_slots(*r, mode).len()).sum();
            assert_eq!(count, all.len(), "{mode}");
        }
        assert!(role_slots(Role::Container, Mode::Light).is_empty());
        assert_eq!(role_slots(Role::Accent, Mode::Dark), [Slot::Cursor]);
        assert_eq!(role_slots(Role::Pane, Mode::Dark), [Slot::Background]);
        assert_eq!(
            role_slots(Role::Pane, Mode::Light),
            [Slot::Background, Slot::CosmicGray2]
        );
    }

    const A_TOML: &str = "name = \"a\"\ndetect = {}\n\n[[file]]\npath = \"~/.config/a/a.conf\"\nwrite = \"file\"\ntemplate = \"a.tmpl\"\n";

    /// A user target folder holding these files.
    fn put_target(tmp: &TempDir, name: &str, files: &[(&str, &str)]) {
        let dir = Dirs::under(tmp.path())
            .config
            .join("irida/targets")
            .join(name);
        fs::create_dir_all(&dir).unwrap();
        for (file, text) in files {
            fs::write(dir.join(file), text).unwrap();
        }
    }

    /// User target `a`: one file write, no reload.
    fn put_a(tmp: &TempDir) {
        put_target(
            tmp,
            "a",
            &[
                ("target.toml", A_TOML),
                ("a.tmpl", "accent = {{ accent }}\n"),
            ],
        );
    }

    fn sheet(state: &State) -> &ApplySheet {
        match &state.dialog {
            Some(Dialog::Apply(s)) => s,
            other => panic!("no Apply sheet: {other:?}"),
        }
    }

    const BUILTINS: [&str; 8] = [
        "btop",
        "cava",
        "cosmic-desktop",
        "cosmic-term",
        "fastfetch",
        "lsd",
        "nvim",
        "zsh",
    ];

    #[test]
    fn sheet_groups_targets() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember", "emberday"]);
        put_a(&tmp);
        put_target(
            &tmp,
            "bad",
            &[("target.toml", "name = \"bad\"\nbogus = 1\n")],
        );
        let mut state = start(&tmp);
        assert_eq!(open_slug(&state), Some("ember"));
        let before = snapshot(&tmp);
        let ui = drive(&mut state, Msg::ShowApply);
        assert!(ui.is_empty(), "{ui:?}");
        assert_eq!(snapshot(&tmp), before, "opening the sheet writes nothing");
        let first = sheet(&state);
        assert_eq!(first.applies, "Applies Ember and Ember Day");
        assert_eq!(
            first.pair.as_ref().map(|p| p.slug.as_str()),
            Some("emberday")
        );
        assert_eq!(first.pair_note, None);
        let groups = sheet_groups(&first.targets, &state.settings);
        assert_eq!(groups.toggles, [("a".to_string(), true)]);
        assert_eq!(groups.broken.len(), 1, "{:?}", groups.broken);
        assert_eq!(groups.broken[0].0, "bad");
        assert!(groups.broken[0].1.contains("bogus"), "{:?}", groups.broken);
        assert_eq!(groups.not_installed, BUILTINS);
        assert_eq!(
            groups.not_installed_line().as_deref(),
            Some(format!("Not installed: {}", BUILTINS.join(", ")).as_str())
        );

        // A pair that is not in the store is left out, and the sheet says so.
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::ShowApply);
        let alone = sheet(&state);
        assert_eq!(alone.applies, "Applies Ember");
        assert!(alone.pair.is_none());
        assert!(
            alone
                .pair_note
                .as_deref()
                .is_some_and(|n| n.contains("emberday")),
            "{:?}",
            alone.pair_note
        );
        let groups = sheet_groups(&alone.targets, &state.settings);
        assert!(groups.toggles.is_empty() && groups.broken.is_empty());
        assert_eq!(groups.not_installed, BUILTINS);
    }

    #[test]
    fn toggle_persists() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember"]);
        put_a(&tmp);
        // An unreadable settings file is replaced once a toggle changes a setting.
        let path = Dirs::under(tmp.path()).config.join("irida/settings.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "disabled = 3\n").unwrap();
        let mut state = start(&tmp);
        assert!(state.settings_unreadable);
        drive(&mut state, Msg::ShowApply);
        drive(&mut state, Msg::SetTarget("a".to_string(), false));
        assert!(!state.settings_unreadable);
        assert_eq!(
            store.settings().unwrap().disabled,
            BTreeSet::from(["a".to_string()])
        );
        let groups = sheet_groups(&sheet(&state).targets, &state.settings);
        assert_eq!(groups.toggles, [("a".to_string(), false)]);

        // A target that is not a toggle cannot be turned off.
        drive(&mut state, Msg::SetTarget("zsh".to_string(), false));
        assert_eq!(store.settings().unwrap().disabled.len(), 1);

        drive(&mut state, Msg::SetTarget("a".to_string(), true));
        assert!(store.settings().unwrap().disabled.is_empty());
    }

    #[test]
    fn apply_then_undo() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        put_a(&tmp);
        let conf = Dirs::under(tmp.path()).home.join(".config/a/a.conf");
        fs::create_dir_all(conf.parent().unwrap()).unwrap();
        fs::write(&conf, "mine\n").unwrap();
        let mut state = start(&tmp);
        drive(&mut state, Msg::ShowApply);
        let ui = drive(&mut state, Msg::ConfirmApply);
        assert!(state.dialog.is_none());
        let accent = working(&state).roles.get(Role::Accent).to_hex();
        assert_eq!(
            fs::read_to_string(&conf).unwrap(),
            format!("accent = {accent}\n")
        );
        let [Effect::Toast(t)] = ui.as_slice() else {
            panic!("{ui:?}");
        };
        let mut lines = t.text.lines();
        assert_eq!(lines.next(), Some("Applied to 1 app"));
        // The replaced file is named, as a warning.
        assert!(
            lines.next().is_some_and(|l| l.contains("a.conf")),
            "{}",
            t.text
        );
        let Some((label, msg)) = t.action.clone() else {
            panic!("no Undo action");
        };
        assert_eq!(label, "Undo");
        assert!(matches!(msg, Msg::UndoLastApply));

        let ui = drive(&mut state, msg);
        assert!(
            matches!(ui.as_slice(), [Effect::Toast(t)] if t.text.starts_with("Restored apply ")),
            "{ui:?}"
        );
        assert_eq!(fs::read_to_string(&conf).unwrap(), "mine\n");

        // The menu item sends the same Msg; nothing is left to undo.
        state.menu_open = true;
        drive(&mut state, MenuItem::UndoLastApply.msg());
        assert!(!state.menu_open);
        assert_eq!(last_toast(&state), "Nothing to undo");
    }

    #[test]
    fn apply_texts() {
        let report = ApplyReport {
            record_id: 4,
            targets: vec![
                TargetResult {
                    name: "btop".to_string(),
                    status: TargetStatus::Applied(ReloadStatus::Restart(
                        "restart btop to see it".to_string(),
                    )),
                    warnings: vec![],
                },
                TargetResult {
                    name: "cava".to_string(),
                    status: TargetStatus::Applied(ReloadStatus::Failed("exit 1".to_string())),
                    warnings: vec!["a warning".to_string()],
                },
                TargetResult {
                    name: "lsd".to_string(),
                    status: TargetStatus::Skipped(SkipReason::NotInstalled),
                    warnings: vec![],
                },
            ],
        };
        assert_eq!(
            applied_text(&report),
            "Applied to 2 apps\n\
             btop: restart btop to see it\n\
             Could not reload cava (exit 1); restart it to see the colours.\n\
             a warning"
        );
        let write = |rolled_back| ApplyFailure::Write {
            path: PathBuf::from("/x/a.conf"),
            reason: "disk full".to_string(),
            rolled_back,
        };
        assert_eq!(
            write(true).text(),
            "Could not write /x/a.conf: disk full. Nothing was changed."
        );
        assert_eq!(
            write(false).text(),
            "Could not write /x/a.conf: disk full; run Undo last apply to finish."
        );
        let render = ApplyFailure::Render {
            target: "nvim".to_string(),
            message: "undefined value".to_string(),
        };
        assert_eq!(render.text(), "nvim: undefined value. Nothing was changed.");
        assert_eq!(
            ApplyFailure::NothingToApply.text(),
            "Nothing to apply: turn on an installed app."
        );
    }

    #[test]
    fn nothing_to_apply_toasts() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::ShowApply);
        let ui = drive(&mut state, Msg::ConfirmApply);
        assert!(
            matches!(ui.as_slice(), [Effect::Toast(t)]
                if t.text == "Nothing to apply: turn on an installed app." && t.action.is_none()),
            "{ui:?}"
        );
    }

    fn ember_rice() -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../irida-core/tests/fixtures/rices/cosmic-ember-theme"
        ))
    }

    fn slugs(store: &Store) -> Vec<String> {
        store.list().into_iter().map(|e| e.slug).collect()
    }

    fn import(state: &mut State) -> Vec<Effect> {
        drive(state, Msg::Folder(FolderFor::Import, Ok(ember_rice())))
    }

    #[test]
    fn import_collision_keep_both() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["noir"]);
        let mut state = start(&tmp);
        state.menu_open = true;
        let ui = drive(&mut state, MenuItem::Import.msg());
        assert!(
            matches!(ui.as_slice(), [Effect::ChooseFolder(FolderFor::Import)]),
            "{ui:?}"
        );
        assert!(!state.menu_open);
        assert_eq!(FolderFor::Import.title(), "Import a rice repo");
        let ui = import(&mut state);
        assert!(
            matches!(ui.as_slice(), [Effect::Toast(t)] if t.text.starts_with("Imported Ember")),
            "{ui:?}"
        );
        assert_eq!(slugs(&store), ["ember", "noir"]);
        assert_eq!(open_slug(&state), Some("ember"));

        // Again: the slug exists, so it asks.
        assert!(import(&mut state).is_empty());
        assert!(
            matches!(&state.dialog, Some(Dialog::ImportCollision { palette, replace_ok: true, .. })
                if palette.slug == "ember"),
            "{:?}",
            state.dialog
        );
        drive(&mut state, Msg::ImportKeepBoth);
        assert!(state.dialog.is_none());
        assert_eq!(slugs(&store), ["ember", "embertwo", "noir"]);
        assert_eq!(store.load("embertwo").unwrap().name, "Ember Two");
        assert_eq!(store.versions("ember").unwrap().len(), 1);
        assert_eq!(open_slug(&state), Some("embertwo"));

        // The next Keep both takes the next free number.
        import(&mut state);
        drive(&mut state, Msg::ImportKeepBoth);
        assert_eq!(store.load("emberthree").unwrap().name, "Ember Three");

        // Replace instead saves a new version of the existing slug.
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["noir"]);
        let mut state = start(&tmp);
        import(&mut state);
        import(&mut state);
        drive(&mut state, Msg::ImportReplace);
        assert!(state.dialog.is_none());
        assert_eq!(slugs(&store), ["ember", "noir"]);
        assert_eq!(store.versions("ember").unwrap().len(), 2);
        assert_eq!(open_slug(&state), Some("ember"));

        // Replace keeps the notes and pair the repo cannot carry.
        drive(&mut state, Msg::SetNotes("Kept".to_string()));
        drive(&mut state, Msg::Save);
        import(&mut state);
        drive(&mut state, Msg::ImportReplace);
        assert_eq!(store.versions("ember").unwrap().len(), 4);
        assert_eq!(store.load("ember").unwrap().notes.as_deref(), Some("Kept"));
        assert_eq!(working(&state).notes.as_deref(), Some("Kept"));
    }

    #[test]
    fn import_replace_over_dirty_open_palette_asks_first() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "noir"]);
        let mut state = start(&tmp);
        assert_eq!(open_slug(&state), Some("ember"));
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        let before = snapshot(&tmp);

        // Cancel writes nothing and keeps the edits.
        import(&mut state);
        drive(&mut state, Msg::ImportReplace);
        assert!(
            matches!(&state.dialog, Some(Dialog::Unsaved(Leave::Import { palette, .. }))
                if palette.slug == "ember"),
            "{:?}",
            state.dialog
        );
        assert_eq!(snapshot(&tmp), before);
        drive(&mut state, Msg::CloseDialog);
        assert!(state.dialog.is_none());
        assert_eq!(snapshot(&tmp), before);
        assert_eq!(store.versions("ember").unwrap().len(), 1);
        assert!(state.dirty());
        assert_eq!(working(&state).roles.get(Role::Accent), red());

        // Save saves the edits as a version, then the import on top.
        import(&mut state);
        drive(&mut state, Msg::ImportReplace);
        drive(&mut state, Msg::UnsavedSave);
        assert!(state.dialog.is_none());
        assert_eq!(store.versions("ember").unwrap().len(), 3);
        assert_eq!(
            store
                .load_version("ember", 2)
                .unwrap()
                .roles
                .get(Role::Accent),
            red()
        );
        let imported = store.load("ember").unwrap();
        assert_ne!(imported.roles.get(Role::Accent), red());
        assert_eq!(open_slug(&state), Some("ember"));
        assert_eq!(working(&state), &imported);
        assert!(!state.dirty());
        assert_eq!(state.versions, store.versions("ember").unwrap());

        // Discard drops the edits and opens the import.
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        import(&mut state);
        drive(&mut state, Msg::ImportReplace);
        assert!(matches!(
            &state.dialog,
            Some(Dialog::Unsaved(Leave::Import { .. }))
        ));
        drive(&mut state, Msg::UnsavedDiscard);
        assert!(state.dialog.is_none());
        assert_eq!(store.versions("ember").unwrap().len(), 4);
        let imported = store.load("ember").unwrap();
        assert_ne!(imported.roles.get(Role::Accent), red());
        assert_eq!(working(&state), &imported);
        assert!(!state.dirty());
        assert_eq!(state.versions, store.versions("ember").unwrap());
    }

    /// Opens an unsaved draft called `name`, built from the open palette.
    fn open_draft(state: &mut State, name: &str) -> Palette {
        let mut p = working(state).clone();
        p.name = name.to_string();
        p.slug = slug_from_name(name);
        state.go(Leave::Draft(p.clone()));
        assert!(state.dirty());
        p
    }

    #[test]
    fn import_over_open_draft_keeps_both() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["noir"]);
        let mut state = start(&tmp);
        let draft = open_draft(&mut state, "Ember");

        // No stored ember, but the draft has its name: only Keep both.
        assert!(import(&mut state).is_empty());
        assert!(
            matches!(&state.dialog, Some(Dialog::ImportCollision { palette, replace_ok: false, .. })
                if palette.slug == "ember"),
            "{:?}",
            state.dialog
        );
        assert_eq!(slugs(&store), ["noir"]);
        assert!(drive(&mut state, Msg::ImportReplace).is_empty());
        assert!(matches!(state.dialog, Some(Dialog::ImportCollision { .. })));
        assert_eq!(slugs(&store), ["noir"]);

        // Keep both saves it as Ember Two, then asks about the draft.
        drive(&mut state, Msg::ImportKeepBoth);
        assert_eq!(slugs(&store), ["embertwo", "noir"]);
        assert!(
            matches!(&state.dialog, Some(Dialog::Unsaved(Leave::Open(s))) if s == "embertwo"),
            "{:?}",
            state.dialog
        );
        assert_eq!(working(&state), &draft);

        // The draft can still be saved under its own name.
        drive(&mut state, Msg::CloseDialog);
        drive(&mut state, Msg::Save);
        assert!(state.dialog.is_none());
        assert_eq!(slugs(&store), ["ember", "embertwo", "noir"]);
        assert_eq!(open_slug(&state), Some("ember"));
        assert!(!state.dirty());
    }

    #[test]
    fn import_over_open_draft_of_a_stored_name_keeps_both() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "noir"]);
        let mut state = start(&tmp);
        let draft = open_draft(&mut state, "Ember");

        // The stored ember is readable, but Replace is not over the draft.
        import(&mut state);
        assert!(
            matches!(
                &state.dialog,
                Some(Dialog::ImportCollision {
                    replace_ok: false,
                    ..
                })
            ),
            "{:?}",
            state.dialog
        );
        assert!(drive(&mut state, Msg::ImportReplace).is_empty());
        assert_eq!(store.versions("ember").unwrap().len(), 1);

        // Keep both asks about the draft once, and Discard opens the import.
        drive(&mut state, Msg::ImportKeepBoth);
        assert_eq!(slugs(&store), ["ember", "embertwo", "noir"]);
        assert!(
            matches!(&state.dialog, Some(Dialog::Unsaved(Leave::Open(s))) if s == "embertwo"),
            "{:?}",
            state.dialog
        );
        assert_eq!(working(&state), &draft);
        drive(&mut state, Msg::UnsavedDiscard);
        assert!(state.dialog.is_none());
        assert_eq!(open_slug(&state), Some("embertwo"));
        assert!(!state.dirty());
    }

    #[test]
    fn import_replace_discard_clears_typed_hex() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember", "noir"]);
        let mut state = start(&tmp);
        let saved = working(&state).clone();
        drive(
            &mut state,
            Msg::HexInput(Role::Accent, "#00FF00".to_string()),
        );
        drive(
            &mut state,
            Msg::HexInput(Role::Accent, "#00FF0".to_string()),
        );
        assert!(state.dirty());
        assert!(state.field(Field::Hex(Role::Accent)).is_some());

        // Discard reverts the edits, and a failed import leaves no typed
        // text over the reverted colours.
        import(&mut state);
        drive(&mut state, Msg::ImportReplace);
        let effects = update(&mut state, Msg::UnsavedDiscard);
        let [effect @ Effect::SaveImport { .. }] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        let msg = panicked(effect.clone(), &state.ctx, "boom".to_string());
        drive(&mut state, msg);
        assert!(last_toast(&state).starts_with("Not imported"));
        assert_eq!(working(&state), &saved);
        assert!(!state.dirty());
        assert!(state.fields.is_empty());
    }

    #[test]
    fn import_keep_both_skips_open_draft() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["noir"]);
        let mut state = start(&tmp);
        import(&mut state);
        assert_eq!(open_slug(&state), Some("ember"));
        let draft = open_draft(&mut state, "Ember Two");

        import(&mut state);
        drive(&mut state, Msg::ImportKeepBoth);
        assert_eq!(slugs(&store), ["ember", "emberthree", "noir"]);
        assert!(
            matches!(&state.dialog, Some(Dialog::Unsaved(Leave::Open(s))) if s == "emberthree"),
            "{:?}",
            state.dialog
        );
        assert_eq!(working(&state), &draft);
        assert!(state.dirty());
    }

    #[test]
    fn import_never_replaces_unreadable() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["noir"]);
        fs::create_dir_all(palettes_dir(&tmp).join("ember")).unwrap();
        fs::write(palettes_dir(&tmp).join("ember/palette.toml"), "nope").unwrap();
        let mut state = start(&tmp);
        let before = snapshot(&tmp);
        assert!(import(&mut state).is_empty());
        assert!(
            matches!(
                &state.dialog,
                Some(Dialog::ImportCollision {
                    replace_ok: false,
                    ..
                })
            ),
            "{:?}",
            state.dialog
        );
        assert!(drive(&mut state, Msg::ImportReplace).is_empty());
        assert!(matches!(state.dialog, Some(Dialog::ImportCollision { .. })));
        assert_eq!(snapshot(&tmp), before);
        drive(&mut state, Msg::ImportKeepBoth);
        assert_eq!(
            fs::read_to_string(palettes_dir(&tmp).join("ember/palette.toml")).unwrap(),
            "nope"
        );
        assert!(palettes_dir(&tmp).join("embertwo/palette.toml").is_file());
    }

    #[test]
    fn keep_both_names() {
        let taken = ["embertwo", "emberthree"];
        assert_eq!(
            keep_both_name("Ember", |s| taken.contains(&s)).as_deref(),
            Some("Ember Four")
        );
        assert_eq!(keep_both_name("Ember", |_| true), None);
        assert_eq!(
            keep_both_name("Ember Day", |_| false).as_deref(),
            Some("Ember Day Two")
        );
    }

    #[test]
    fn export_summary() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        state.menu_open = true;
        let ui = drive(&mut state, MenuItem::Export.msg());
        assert!(
            matches!(ui.as_slice(), [Effect::ChooseFolder(FolderFor::Export)]),
            "{ui:?}"
        );
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let ui = drive(&mut state, Msg::Folder(FolderFor::Export, Ok(out.clone())));
        assert!(ui.is_empty(), "{ui:?}");
        let Some(Dialog::Exported(summary)) = &state.dialog else {
            panic!("{:?}", state.dialog);
        };
        assert!(
            summary.lines.contains(&ExportLine {
                path: "cosmic/Ember.ron".to_string(),
                status: ExportStatus::Created,
            }),
            "{:?}",
            summary.lines
        );
        assert!(
            summary
                .text
                .lines()
                .any(|l| l.trim() == "created   cosmic/Ember.ron"),
            "{}",
            summary.text
        );
        assert!(summary.text.contains(BY_HAND));
        assert!(summary.text.contains("Contrast: "));
        // The pair is not in the store: exported alone, and said so.
        assert!(summary.text.contains("emberday"), "{}", summary.text);
        assert!(out.join("cosmic-ember-theme/cosmic/Ember.ron").is_file());
        drive(&mut state, Msg::CloseDialog);
        assert!(state.dialog.is_none());
    }

    const VECTORS: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../irida-core/tests/fixtures/draft/vectors.json"
    );

    /// The reference vector `case` written as a 40 by 25 PNG at `path`.
    fn vector_png(case: &str, path: &std::path::Path) {
        use base64::Engine;
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(VECTORS).unwrap()).unwrap();
        let rgba = base64::engine::general_purpose::STANDARD
            .decode(v[case]["rgba"].as_str().unwrap())
            .unwrap();
        image::RgbaImage::from_raw(40, 25, rgba)
            .unwrap()
            .save(path)
            .unwrap();
    }

    /// COSMIC's `all` background key under the temp config, pointing at
    /// `image`, written as COSMIC's own default file is (with `#true`).
    fn cosmic_wallpaper(tmp: &TempDir, image: &std::path::Path) {
        let dir = Dirs::under(tmp.path())
            .config
            .join("cosmic/com.system76.CosmicBackground/v1");
        fs::create_dir_all(&dir).unwrap();
        let entry = format!(
            "(\n    output: \"all\",\n    source: Path({:?}),\n    filter_by_theme: #true,\n    rotation_frequency: 3600,\n    filter_method: Lanczos,\n    scaling_mode: Zoom,\n    sampling_method: Alphanumeric,\n)",
            image.to_str().unwrap()
        );
        fs::write(dir.join("all"), entry).unwrap();
    }

    fn wallpaper_dialog(state: &State) -> &WallpaperDialog {
        match &state.dialog {
            Some(Dialog::Wallpaper(d)) => d,
            other => panic!("no wallpaper dialog: {other:?}"),
        }
    }

    /// A started app on Ember whose COSMIC wallpaper is the two-tone vector,
    /// with New from wallpaper opened.
    fn two_tone_dialog(tmp: &TempDir) -> (State, PathBuf) {
        store_with(tmp, &["ember"]);
        let image = tmp.path().join("pictures/two-tone.png");
        fs::create_dir_all(image.parent().unwrap()).unwrap();
        vector_png("two-tone", &image);
        cosmic_wallpaper(tmp, &image);
        let mut state = start(tmp);
        state.menu_open = true;
        drive(&mut state, MenuItem::NewFromWallpaper.msg());
        (state, image)
    }

    #[test]
    fn opens_with_current_wallpaper() {
        let tmp = TempDir::new().unwrap();
        let (state, image) = two_tone_dialog(&tmp);
        assert!(!state.menu_open);
        let d = wallpaper_dialog(&state);
        assert_eq!(d.image.as_deref(), Some(image.as_path()));
        assert_eq!(d.clusters.len(), 8);
        assert_eq!(d.mode, Mode::Dark);
        let expected = irida_core::draft::draft(&d.clusters, Mode::Dark);
        assert_eq!(d.roles, Some(expected.roles));
        assert_eq!(d.accent_from, expected.accent_from);
        assert!(d.accent_from.is_some(), "two-tone has a vivid colour");
        assert_eq!(d.name, "Two Tone");
        assert_eq!(d.error, None);
        assert_eq!(d.reading, None);
        assert!(state.wallpaper_ready());

        // No wallpaper set: the dialog opens empty, asking for an image.
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        let ui = drive(&mut state, Msg::ShowWallpaper);
        assert!(ui.is_empty(), "{ui:?}");
        let d = wallpaper_dialog(&state);
        assert_eq!(d.image, None);
        assert!(d.clusters.is_empty());
        assert_eq!(d.roles, None);
        assert!(!state.wallpaper_ready());
        // Choose another image asks the file chooser.
        let ui = drive(&mut state, Msg::ChooseImage);
        assert!(matches!(ui.as_slice(), [Effect::ChooseImage]), "{ui:?}");
        assert!(!Effect::ChooseImage.is_io());
    }

    #[test]
    fn switch_mode_redrafts() {
        let tmp = TempDir::new().unwrap();
        let (mut state, _) = two_tone_dialog(&tmp);
        let clusters = wallpaper_dialog(&state).clusters.clone();
        drive(&mut state, Msg::WallpaperMode(Mode::Light));
        let d = wallpaper_dialog(&state);
        assert_eq!(d.mode, Mode::Light);
        let light = irida_core::draft::draft(&clusters, Mode::Light);
        assert_eq!(d.roles, Some(light.roles.clone()));
        assert_eq!(d.accent_from, light.accent_from);
        assert_ne!(
            light.roles,
            irida_core::draft::draft(&clusters, Mode::Dark).roles
        );

        // Created in light mode, the draft keeps those roles.
        drive(&mut state, Msg::WallpaperCreate);
        assert!(state.dialog.is_none());
        let w = working(&state);
        assert_eq!(w.mode, Mode::Light);
        assert_eq!(w.roles, light.roles);
    }

    #[test]
    fn create_opens_unsaved() {
        let tmp = TempDir::new().unwrap();
        let (mut state, _) = two_tone_dialog(&tmp);
        let before = snapshot(&tmp);
        let settings = Dirs::under(tmp.path()).config.join("irida/settings.toml");
        let settings_before = fs::read(&settings).ok();

        // A taken name cannot be created.
        drive(&mut state, Msg::WallpaperName("Ember".to_string()));
        assert!(!state.wallpaper_ready());
        drive(&mut state, Msg::WallpaperCreate);
        assert!(matches!(state.dialog, Some(Dialog::Wallpaper(_))));
        drive(&mut state, Msg::WallpaperName("Bad-Name".to_string()));
        assert!(!state.wallpaper_ready());

        drive(&mut state, Msg::WallpaperName("Two Tone".to_string()));
        assert!(state.wallpaper_ready());
        let clusters = wallpaper_dialog(&state).clusters.clone();
        let ui = drive(&mut state, Msg::WallpaperCreate);
        assert!(ui.is_empty(), "{ui:?}");
        assert!(state.dialog.is_none());
        let Some(Current::Editable {
            slug,
            saved,
            working,
        }) = &state.current
        else {
            panic!("{:?}", state.current);
        };
        assert_eq!(slug, "twotone");
        assert_eq!(saved, &None);
        assert_eq!(
            working,
            &irida_core::draft::draft_palette(&clusters, Mode::Dark, "Two Tone")
        );
        assert!(state.dirty());
        assert_eq!(state.title(), "Two Tone \u{2022}");
        assert_eq!(snapshot(&tmp), before, "nothing written to the store");
        assert_eq!(fs::read(&settings).ok(), settings_before);
        // An unsaved draft cannot be duplicated or deleted by slug.
        assert!(!state.menu_enabled(MenuItem::Duplicate));
        assert!(!state.menu_enabled(MenuItem::Delete));

        // Ctrl+S saves version 1 and remembers it.
        let ui = drive(&mut state, Msg::Save);
        assert!(
            matches!(ui.as_slice(), [Effect::Toast(t)] if t.text == "Saved version 1"),
            "{ui:?}"
        );
        assert!(!state.dirty());
        let store = Store::new(Dirs::under(tmp.path()));
        assert_eq!(store.versions("twotone").unwrap().len(), 1);
        assert!(state.entries.iter().any(|e| e.slug == "twotone"));
        assert_eq!(
            store.settings().unwrap().last_palette.as_deref(),
            Some("twotone")
        );
        assert!(state.menu_enabled(MenuItem::Delete));
    }

    #[test]
    fn create_while_dirty_asks() {
        let tmp = TempDir::new().unwrap();
        let (mut state, _) = two_tone_dialog(&tmp);
        drive(&mut state, Msg::CloseDialog);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::ShowWallpaper);
        assert!(wallpaper_dialog(&state).roles.is_some());
        drive(&mut state, Msg::WallpaperCreate);
        assert!(matches!(
            state.dialog,
            Some(Dialog::Unsaved(Leave::Draft(_)))
        ));
        assert_eq!(open_slug(&state), Some("ember"));
        drive(&mut state, Msg::UnsavedDiscard);
        assert_eq!(open_slug(&state), Some("twotone"));
        assert!(state.dirty());
    }

    #[test]
    fn failed_save_keeps_the_draft_question() {
        let tmp = TempDir::new().unwrap();
        let (mut state, _) = two_tone_dialog(&tmp);
        drive(&mut state, Msg::CloseDialog);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::ShowWallpaper);
        drive(&mut state, Msg::WallpaperCreate);
        let asked = state.dialog.clone();
        assert!(matches!(asked, Some(Dialog::Unsaved(Leave::Draft(_)))));

        // The store refuses the save: the question comes back with the draft.
        let running = update(&mut state, Msg::UnsavedSave);
        assert!(matches!(running.as_slice(), [Effect::SaveVersion { .. }]));
        drive(&mut state, Msg::Saved(Err("disk full".to_string())));
        assert_eq!(last_toast(&state), "Not saved: disk full");
        assert_eq!(state.dialog, asked);
        assert_eq!(open_slug(&state), Some("ember"));

        // So does an edit that cannot be saved.
        if let Some(Current::Editable { working, .. }) = &mut state.current {
            working.name = "Bad-Name".to_string();
        }
        let ui = drive(&mut state, Msg::UnsavedSave);
        assert!(matches!(ui.as_slice(), [Effect::Toast(_)]), "{ui:?}");
        assert_eq!(state.dialog, asked);

        // Discard still opens the draft.
        drive(&mut state, Msg::UnsavedDiscard);
        assert_eq!(open_slug(&state), Some("twotone"));
    }

    #[test]
    fn open_draft_name_is_taken() {
        let tmp = TempDir::new().unwrap();
        let (mut state, _) = two_tone_dialog(&tmp);
        drive(&mut state, Msg::WallpaperCreate);
        assert_eq!(open_slug(&state), Some("twotone"));
        let first = working(&state).clone();

        // The same image again: its name is held by the open draft.
        drive(&mut state, Msg::ShowWallpaper);
        drive(&mut state, Msg::WallpaperMode(Mode::Light));
        assert_eq!(wallpaper_dialog(&state).name, "Two Tone");
        let taken = Err("A palette called twotone is open and not saved yet".to_string());
        assert_eq!(state.name_check("Two Tone"), taken);
        assert!(!state.wallpaper_ready());
        drive(&mut state, Msg::WallpaperCreate);
        assert!(matches!(state.dialog, Some(Dialog::Wallpaper(_))));
        assert_eq!(working(&state), &first);
        // New is refused the same way.
        drive(&mut state, Msg::ShowNew);
        drive(&mut state, Msg::NameInput("Two Tone".to_string()));
        assert!(drive(&mut state, Msg::NameConfirm).is_empty());
        assert!(matches!(state.dialog, Some(Dialog::Name { .. })));
        drive(&mut state, Msg::ShowWallpaper);
        drive(&mut state, Msg::WallpaperMode(Mode::Light));

        // Another name goes through Save, and both drafts save version 1.
        drive(&mut state, Msg::WallpaperName("Two Tone Light".to_string()));
        assert!(state.wallpaper_ready());
        drive(&mut state, Msg::WallpaperCreate);
        assert!(matches!(
            state.dialog,
            Some(Dialog::Unsaved(Leave::Draft(_)))
        ));
        drive(&mut state, Msg::UnsavedSave);
        assert_eq!(open_slug(&state), Some("twotonelight"));
        assert!(state.dirty());
        assert_eq!(working(&state).mode, Mode::Light);
        let ui = drive(&mut state, Msg::Save);
        assert!(
            matches!(ui.as_slice(), [Effect::Toast(t)] if t.text == "Saved version 1"),
            "{ui:?}"
        );
        let store = Store::new(Dirs::under(tmp.path()));
        assert_eq!(store.versions("twotone").unwrap().len(), 1);
        assert_eq!(store.versions("twotonelight").unwrap().len(), 1);
        // Saved, the draft's name is taken by the store instead.
        assert_eq!(
            state.name_check("Two Tone Light"),
            Err("A palette called twotonelight already exists".to_string())
        );
    }

    #[test]
    fn bad_file_message() {
        let tmp = TempDir::new().unwrap();
        let (mut state, image) = two_tone_dialog(&tmp);
        let text = tmp.path().join("notes.txt");
        fs::write(&text, "not an image").unwrap();
        drive(&mut state, Msg::Image(Ok(text)));
        let d = wallpaper_dialog(&state);
        assert_eq!(
            d.error.as_deref(),
            Some("This file could not be read as an image. Try a JPEG, PNG or WebP.")
        );
        // The last good draft stays, so Create still works.
        assert_eq!(d.image.as_deref(), Some(image.as_path()));
        assert_eq!(d.clusters.len(), 8);
        assert_eq!(d.reading, None);
        assert!(state.wallpaper_ready());

        // A missing file says so, and the chooser's own error is shown.
        let gone = tmp.path().join("gone.png");
        drive(&mut state, Msg::Image(Ok(gone)));
        let d = wallpaper_dialog(&state);
        assert!(
            d.error
                .as_deref()
                .unwrap()
                .starts_with("Could not read gone.png"),
            "{:?}",
            d.error
        );
        drive(&mut state, Msg::Image(Err("portal gone".to_string())));
        assert_eq!(
            wallpaper_dialog(&state).error.as_deref(),
            Some("Could not choose an image: portal gone")
        );

        // A good image clears the error and takes its name.
        let other = tmp.path().join("aurora-forest.png");
        vector_png("pastel", &other);
        drive(&mut state, Msg::Image(Ok(other.clone())));
        let d = wallpaper_dialog(&state);
        assert_eq!(d.error, None);
        assert_eq!(d.image.as_deref(), Some(other.as_path()));
        assert_eq!(d.name, "Aurora Forest");
    }

    #[test]
    fn stale_drafts_are_ignored() {
        let tmp = TempDir::new().unwrap();
        let (mut state, image) = two_tone_dialog(&tmp);
        let other = tmp.path().join("pastel.png");
        vector_png("pastel", &other);
        // Pastel is being read when two-tone's late result arrives.
        let effects = update(&mut state, Msg::Image(Ok(other.clone())));
        assert!(matches!(effects.as_slice(), [Effect::DraftImage(p)] if *p == other));
        let late = perform(Effect::DraftImage(image.clone()), &state.ctx);
        let before = wallpaper_dialog(&state).clone();
        for msg in late {
            drive(&mut state, msg);
        }
        assert_eq!(wallpaper_dialog(&state), &before);
        drive_effects(&mut state, effects);
        assert_eq!(
            wallpaper_dialog(&state).image.as_deref(),
            Some(other.as_path())
        );
        // A result after the dialog closed changes nothing.
        drive(&mut state, Msg::CloseDialog);
        for msg in perform(Effect::DraftImage(image), &state.ctx) {
            drive(&mut state, msg);
        }
        assert!(state.dialog.is_none());
    }

    /// Open a new palette called `name` through New.
    fn new_named(state: &mut State, name: &str, mode: Mode) {
        drive(state, Msg::ShowNew);
        drive(state, Msg::NameInput(name.to_string()));
        drive(state, Msg::NameMode(mode));
        drive(state, Msg::NameConfirm);
        assert_eq!(open_slug(state), Some(slug_from_name(name).as_str()));
    }

    fn accent(p: &Option<Palette>) -> Rgb {
        p.as_ref().expect("a palette").roles.get(Role::Accent)
    }

    #[test]
    fn pair_split_needs_pair() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "emberday", "noir", "dossier"]);
        store
            .save_settings(&Settings {
                last_palette: Some("ember".to_string()),
                ..Settings::default()
            })
            .unwrap();
        let mut state = start(&tmp);
        assert_eq!(open_slug(&state), Some("ember"));
        assert!(state.header_enabled(HeaderItem::Pair));
        assert_eq!(state.pair_label(), "Pair: Ember Day");
        assert!(state.menu_enabled(MenuItem::Unpair));
        assert_eq!(state.split, Split::Off);
        assert!(state.split_view().is_none());

        // Ember is paired: the chip shows both halves.
        drive(&mut state, Msg::PairChip);
        assert_eq!(state.split, Split::Pair);
        assert!(state.dialog.is_none());
        let view = state.split_view().expect("a split");
        assert_eq!(view.left.label, "Ember (dark)");
        assert_eq!(view.left.palette, Some(starter("ember")));
        assert_eq!(view.right.label, "Ember Day (light)");
        assert_eq!(view.right.palette, Some(starter("emberday")));
        assert!(view.versions.is_none());
        // The labels follow the working copy.
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        let view = state.split_view().unwrap();
        assert_eq!(accent(&view.left.palette), red());
        // The chip again closes it.
        drive(&mut state, Msg::PairChip);
        assert_eq!(state.split, Split::Off);
        assert!(state.split_view().is_none());

        // A new dark palette lists the light ones, each labelled as paired.
        let ember_accent = starter("ember").roles.get(Role::Accent);
        drive(&mut state, Msg::SetRole(Role::Accent, ember_accent));
        assert!(!state.dirty());
        new_named(&mut state, "Moss", Mode::Dark);
        assert_eq!(state.split, Split::Off, "a new palette closes the split");
        assert_eq!(state.pair_label(), "Pair with...");
        assert!(!state.menu_enabled(MenuItem::Unpair));
        drive(&mut state, Msg::PairChip);
        assert_eq!(state.split, Split::Off);
        let Some(Dialog::PairWith { choices, confirm }) = &state.dialog else {
            panic!("{:?}", state.dialog);
        };
        assert_eq!(confirm, &None);
        let labels: Vec<String> = choices.iter().map(PairChoice::label).collect();
        assert_eq!(
            labels,
            ["Dossier, paired with Noir", "Ember Day, paired with Ember"]
        );
    }

    #[test]
    fn saved_mode_shows_in_the_list() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember", "emberday", "noir"]);
        let mut state = start(&tmp);
        new_named(&mut state, "Moss", Mode::Dark);
        drive(&mut state, Msg::SetMode(Mode::Light));
        drive(&mut state, Msg::Save);
        let moss = state.entries.iter().find(|e| e.slug == "moss").unwrap();
        assert_eq!(moss.palette, Ok(("Moss".to_string(), Mode::Light)));

        // Pair with... offers the dark palettes, not Moss itself.
        drive(&mut state, Msg::PairChip);
        let Some(Dialog::PairWith { choices, .. }) = &state.dialog else {
            panic!("{:?}", state.dialog);
        };
        let slugs: Vec<&str> = choices.iter().map(|c| c.slug.as_str()).collect();
        assert_eq!(slugs, ["ember", "noir"]);
    }

    #[test]
    fn link_survives_save() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "emberday", "noir", "dossier"]);
        let mut state = start(&tmp);
        new_named(&mut state, "Moss", Mode::Dark);

        // Dossier is paired with Noir, so choosing it asks first.
        drive(&mut state, Msg::PairChip);
        drive(&mut state, Msg::PairChoose("dossier".to_string()));
        assert!(matches!(
            &state.dialog,
            Some(Dialog::PairWith { confirm: Some(slug), .. }) if slug == "dossier"
        ));
        assert_eq!(store.load("moss").unwrap().pair, None, "not yet");
        drive(&mut state, Msg::PairConfirm);
        assert!(state.dialog.is_none());
        assert_eq!(store.load("moss").unwrap().pair.as_deref(), Some("dossier"));
        assert_eq!(store.load("dossier").unwrap().pair.as_deref(), Some("moss"));
        assert_eq!(store.load("noir").unwrap().pair, None);
        assert_eq!(working(&state).pair.as_deref(), Some("dossier"));
        assert!(!state.dirty(), "linking is not an unsaved edit");
        assert!(last_toast(&state).starts_with("Paired Moss with Dossier"));
        assert_eq!(state.pair_label(), "Pair: Dossier");

        // An edit and Ctrl+S keep the pair.
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::Save);
        assert!(!state.dirty());
        let moss = store.load("moss").unwrap();
        assert_eq!(moss.pair.as_deref(), Some("dossier"));
        assert_eq!(moss.roles.get(Role::Accent), red());
        assert_eq!(
            store.load_version("moss", 2).unwrap().pair.as_deref(),
            Some("dossier")
        );

        // Unpair from the menu clears both sides, and a save keeps that.
        state.menu_open = true;
        drive(&mut state, MenuItem::Unpair.msg());
        assert!(!state.menu_open);
        assert_eq!(store.load("moss").unwrap().pair, None);
        assert_eq!(store.load("dossier").unwrap().pair, None);
        assert_eq!(working(&state).pair, None);
        assert!(!state.dirty());
        drive(
            &mut state,
            Msg::SetRole(Role::Accent, Rgb::from_hex("#00FF00").unwrap()),
        );
        drive(&mut state, Msg::Save);
        assert_eq!(store.load("moss").unwrap().pair, None);

        // A palette nobody is paired with links without asking.
        drive(&mut state, Msg::PairChip);
        drive(&mut state, Msg::PairChoose("dossier".to_string()));
        assert!(state.dialog.is_none());
        assert_eq!(store.load("dossier").unwrap().pair.as_deref(), Some("moss"));
    }

    #[test]
    fn save_queued_behind_a_link_keeps_the_pair() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "emberday"]);
        let mut state = start(&tmp);
        new_named(&mut state, "Moss", Mode::Dark);

        // Ctrl+S before the link has run (behind a live apply, say): both
        // run in one batch, the save after the link.
        drive(&mut state, Msg::PairChip);
        drive(&mut state, Msg::PairChoose("emberday".to_string()));
        let mut effects = update(&mut state, Msg::PairConfirm);
        assert!(matches!(effects.as_slice(), [Effect::LinkPair { .. }]));
        effects.extend(update(&mut state, Msg::SetRole(Role::Accent, red())));
        effects.extend(update(&mut state, Msg::Save));
        drive_effects(&mut state, effects);
        assert_eq!(
            store.load("moss").unwrap().pair.as_deref(),
            Some("emberday")
        );
        assert_eq!(
            store.load("emberday").unwrap().pair.as_deref(),
            Some("moss")
        );
        assert_eq!(
            store.load_version("moss", 2).unwrap().pair.as_deref(),
            Some("emberday")
        );
        assert_eq!(working(&state).pair.as_deref(), Some("emberday"));
        assert!(!state.dirty());

        // And the same for Unpair.
        let mut effects = update(&mut state, Msg::Unpair);
        effects.extend(update(
            &mut state,
            Msg::SetRole(Role::Accent, Rgb::from_hex("#00FF00").unwrap()),
        ));
        effects.extend(update(&mut state, Msg::Save));
        drive_effects(&mut state, effects);
        assert_eq!(store.load("moss").unwrap().pair, None);
        assert_eq!(store.load("emberday").unwrap().pair, None);
        assert_eq!(store.load_version("moss", 3).unwrap().pair, None);
        assert_eq!(working(&state).pair, None);
        assert!(!state.dirty());
    }

    #[test]
    fn compare_versions() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        new_named(&mut state, "Moss", Mode::Dark);
        let green = Rgb::from_hex("#00FF00").unwrap();
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::Save);
        drive(&mut state, Msg::SetRole(Role::Accent, green));
        assert!(state.header_enabled(HeaderItem::Compare));
        assert_eq!(state.header_tooltip(HeaderItem::Compare), None);

        // Compare shows version 2 next to the working copy.
        drive(&mut state, Msg::Compare);
        assert_eq!(state.split, Split::Saved(VersionRef::Latest));
        let view = state.split_view().expect("a split");
        assert_eq!(view.left.label, "v2");
        assert_eq!(accent(&view.left.palette), red());
        assert_eq!(view.right.label, "Working copy");
        assert_eq!(accent(&view.right.palette), green);
        let choice = view.versions.expect("a version list");
        assert_eq!(choice.labels, ["v2", "v1"]);
        assert_eq!(choice.selected, Some(0));
        assert_eq!(choice.snapshot, None);

        // Choosing version 1 shows it.
        drive(&mut state, Msg::CompareVersion(1));
        assert_eq!(state.split, Split::Saved(VersionRef::Version(1)));
        let view = state.split_view().unwrap();
        assert_eq!(view.left.label, "v1");
        assert_eq!(
            view.left.palette,
            Some(store.load_version("moss", 1).unwrap())
        );
        assert_eq!(
            accent(&view.left.palette),
            starter("ember").roles.get(Role::Accent)
        );
        assert_eq!(view.versions.unwrap().selected, Some(1));

        // Pinning names it.
        drive(&mut state, Msg::ShowPinSnapshot);
        assert!(matches!(
            &state.dialog,
            Some(Dialog::Snapshot { n: 1, name }) if name.is_empty()
        ));
        drive(&mut state, Msg::SnapshotInput("   ".to_string()));
        assert_eq!(state.snapshot_check(), Err(String::new()));
        drive(&mut state, Msg::SnapshotConfirm);
        assert!(state.dialog.is_some(), "an empty name is refused");
        drive(&mut state, Msg::SnapshotInput("x".repeat(41)));
        assert!(state.snapshot_check().is_err());
        drive(&mut state, Msg::SnapshotInput(" Warm accent ".to_string()));
        assert_eq!(state.snapshot_check(), Ok("Warm accent".to_string()));
        drive(&mut state, Msg::SnapshotConfirm);
        assert!(state.dialog.is_none());
        assert_eq!(
            store.versions("moss").unwrap()[1].snapshot.as_deref(),
            Some("Warm accent")
        );
        let view = state.split_view().unwrap();
        assert_eq!(view.left.label, "v1 Warm accent");
        let choice = view.versions.unwrap();
        assert_eq!(choice.labels, ["v2", "v1 Warm accent"]);
        assert_eq!(choice.snapshot.as_deref(), Some("Warm accent"));

        // Removing the name keeps the version.
        drive(&mut state, Msg::UnpinSnapshot);
        assert_eq!(store.versions("moss").unwrap()[1].snapshot, None);
        assert_eq!(state.split_view().unwrap().left.label, "v1");
        assert_eq!(state.split, Split::Saved(VersionRef::Version(1)));

        // Compare again closes it; opened again it follows each save.
        drive(&mut state, Msg::Compare);
        assert_eq!(state.split, Split::Off);
        drive(&mut state, Msg::Compare);
        drive(&mut state, Msg::Save);
        let view = state.split_view().unwrap();
        assert_eq!(view.left.label, "v3");
        assert_eq!(accent(&view.left.palette), green);
        assert_eq!(view.versions.unwrap().labels, ["v3", "v2", "v1"]);
    }

    #[test]
    fn one_split_at_a_time() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember", "emberday"]);
        let mut state = start(&tmp);
        assert_eq!(open_slug(&state), Some("ember"));
        drive(&mut state, Msg::PairChip);
        assert_eq!(state.split, Split::Pair);
        drive(&mut state, Msg::Compare);
        assert_eq!(state.split, Split::Saved(VersionRef::Latest));
        let view = state.split_view().unwrap();
        assert_eq!(view.left.label, "v1");
        assert_eq!(view.right.label, "Working copy");
        // And back: the pair replaces the comparison.
        drive(&mut state, Msg::PairChip);
        assert_eq!(state.split, Split::Pair);
        assert_eq!(state.split_view().unwrap().right.label, "Ember Day (light)");
        // Unpairing closes the pair split.
        drive(&mut state, Msg::Unpair);
        assert_eq!(state.split, Split::Off);
        assert_eq!(store.load("emberday").unwrap().pair, None);
        // Opening another palette closes any split.
        drive(&mut state, Msg::Compare);
        drive(&mut state, Msg::ShowSwitch);
        drive(&mut state, Msg::Choose("emberday".to_string()));
        assert_eq!(open_slug(&state), Some("emberday"));
        assert_eq!(state.split, Split::Off);
    }

    #[test]
    fn compare_disabled_without_versions() {
        let tmp = TempDir::new().unwrap();
        let (mut state, _) = two_tone_dialog(&tmp);
        drive(&mut state, Msg::WallpaperName("Two Tone".to_string()));
        drive(&mut state, Msg::WallpaperCreate);
        assert!(matches!(
            state.current,
            Some(Current::Editable { saved: None, .. })
        ));
        assert!(!state.header_enabled(HeaderItem::Compare));
        assert_eq!(
            state.header_tooltip(HeaderItem::Compare),
            Some("Save a version first (Ctrl+S)")
        );
        assert!(!state.header_enabled(HeaderItem::Pair));
        assert!(update(&mut state, Msg::Compare).is_empty());
        assert!(update(&mut state, Msg::PairChip).is_empty());
        assert_eq!(state.split, Split::Off);
        assert!(state.dialog.is_none());
        // Its first save makes a version to compare with.
        drive(&mut state, Msg::Save);
        assert!(state.header_enabled(HeaderItem::Compare));
        assert!(state.header_enabled(HeaderItem::Pair));

        // A stored palette with no saved version cannot be compared either.
        let tmp = TempDir::new().unwrap();
        Store::new(Dirs::under(tmp.path()))
            .save(&starter("noir"))
            .unwrap();
        let state = start(&tmp);
        assert_eq!(open_slug(&state), Some("noir"));
        assert!(!state.header_enabled(HeaderItem::Compare));
        assert!(state.header_enabled(HeaderItem::Pair));
    }

    // Try it live. cava is found by its config folder in the temp home; the
    // path stays empty, so nothing is signalled.

    /// cava's config in the temp home, holding `x\n`.
    fn put_cava(tmp: &TempDir) -> PathBuf {
        let config = Dirs::under(tmp.path()).config.join("cava/config");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "x\n").unwrap();
        config
    }

    fn live_marker(tmp: &TempDir) -> PathBuf {
        Dirs::under(tmp.path()).state.join("irida/live.toml")
    }

    /// The palettes the live applies among `effects` write.
    fn live_applies(effects: &[Effect]) -> Vec<&Palette> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::LiveApply { palette, .. } => Some(palette),
                _ => None,
            })
            .collect()
    }

    /// The generation of the one timer `effects` start.
    fn timer(effects: &[Effect]) -> u64 {
        match effects {
            [Effect::StartTimer(delay, generation)] => {
                assert_eq!(*delay, LIVE_DELAY);
                *generation
            }
            other => panic!("no timer: {other:?}"),
        }
    }

    /// Ember open with cava installed, try it live on and its first apply
    /// done; cava's config path.
    fn live_on(tmp: &TempDir) -> (State, PathBuf) {
        store_with(tmp, &["ember", "noir"]);
        let config = put_cava(tmp);
        let mut state = start(tmp);
        assert_eq!(open_slug(&state), Some("ember"));
        assert!(state.header_enabled(HeaderItem::TryLive));
        assert_eq!(state.header_tooltip(HeaderItem::TryLive), None);
        drive(&mut state, Msg::LiveToggle(true));
        assert!(state.live_on());
        assert_ne!(
            fs::read_to_string(&config).unwrap(),
            "x\n",
            "turning it on applies the open palette"
        );
        assert!(live_marker(tmp).exists());
        (state, config)
    }

    #[test]
    fn pairing_while_live_applies_again() {
        let tmp = TempDir::new().unwrap();
        let (mut state, _) = live_on(&tmp);
        assert_eq!(working(&state).pair.as_deref(), Some("emberday"));
        Store::new(Dirs::under(tmp.path()))
            .save_version(&starter("emberday"))
            .unwrap();
        drive_effects(&mut state, vec![Effect::ListEntries]);
        // The effects that follow running `effects` once, not run.
        fn after(state: &mut State, effects: Vec<Effect>) -> Vec<Effect> {
            let msgs: Vec<Msg> = effects
                .into_iter()
                .flat_map(|e| perform(e, &state.ctx))
                .collect();
            msgs.into_iter().flat_map(|m| update(state, m)).collect()
        }

        // The pair changes what live writes, so it is applied again.
        let unlink = update(&mut state, Msg::Unpair);
        let effects = after(&mut state, unlink);
        let applies = live_applies(&effects);
        assert_eq!(applies.len(), 1, "{effects:?}");
        assert_eq!(applies[0].pair, None);
        assert!(!state.dirty());
        drive_effects(&mut state, effects);

        drive(&mut state, Msg::PairChip);
        let link = update(&mut state, Msg::PairChoose("emberday".to_string()));
        assert!(live_applies(&link).is_empty());
        let effects = after(&mut state, link);
        let applies = live_applies(&effects);
        assert_eq!(applies.len(), 1, "{effects:?}");
        assert_eq!(applies[0].pair.as_deref(), Some("emberday"));
        assert!(!state.dirty());
    }

    #[test]
    fn debounce_applies_latest_only() {
        let tmp = TempDir::new().unwrap();
        let (mut state, config) = live_on(&tmp);
        let g1 = timer(&update(
            &mut state,
            Msg::HexInput(Role::Accent, "#111111".to_string()),
        ));
        let g2 = timer(&update(
            &mut state,
            Msg::HexInput(Role::Accent, "#222222".to_string()),
        ));
        let g3 = timer(&update(
            &mut state,
            Msg::HexInput(Role::Accent, "#333333".to_string()),
        ));
        assert!(g1 < g2 && g2 < g3);
        assert!(update(&mut state, Msg::LiveTick(g1)).is_empty());
        assert!(update(&mut state, Msg::LiveTick(g2)).is_empty());
        let effects = update(&mut state, Msg::LiveTick(g3));
        let applies = live_applies(&effects);
        assert_eq!(applies.len(), 1, "{effects:?}");
        assert_eq!(applies[0].roles.get(Role::Accent), hex("#333333"));
        drive_effects(&mut state, effects);
        assert!(fs::read_to_string(&config).unwrap().contains("#333333"));

        // Late ticks apply nothing more.
        assert!(update(&mut state, Msg::LiveTick(g3)).is_empty());
        assert!(update(&mut state, Msg::LiveTick(g1)).is_empty());

        // The other text fields and the mode chip wait too.
        timer(&update(
            &mut state,
            Msg::SetName(Role::Accent, "Flame".to_string()),
        ));
        timer(&update(&mut state, Msg::SetNotes("warm".to_string())));
        let bright_red = Slot::Ansi(Tone::Bright, Ansi::Red);
        drive(&mut state, Msg::Pin(bright_red));
        timer(&update(
            &mut state,
            Msg::PinHexInput(bright_red, "#ABCDEF".to_string()),
        ));
        let g = timer(&update(&mut state, Msg::SetMode(Mode::Light)));
        let effects = update(&mut state, Msg::LiveTick(g));
        let applies = live_applies(&effects);
        assert_eq!(applies.len(), 1, "{effects:?}");
        assert_eq!(applies[0].mode, Mode::Light);
        assert_eq!(
            applies[0].pins.get(bright_red),
            Some(PinValue::Color(hex("#ABCDEF")))
        );
        assert_eq!(applies[0].notes.as_deref(), Some("warm"));

        // Without a session nothing is scheduled.
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        put_cava(&tmp);
        let mut state = start(&tmp);
        assert!(
            update(
                &mut state,
                Msg::HexInput(Role::Accent, "#111111".to_string())
            )
            .is_empty()
        );
        assert!(update(&mut state, Msg::SetRole(Role::Accent, red())).is_empty());
    }

    #[test]
    fn picker_release_applies_now() {
        let tmp = TempDir::new().unwrap();
        let (mut state, config) = live_on(&tmp);
        drive(&mut state, Msg::Expand(Some(Role::Accent)));
        let green = Hsv::from_color(Srgb::new(0.0_f32, 1.0, 0.0));
        let effects = update(
            &mut state,
            Msg::Picker(ColorPickerUpdate::ActiveColor(green)),
        );
        assert!(
            live_applies(&effects).is_empty(),
            "a drag waits: {effects:?}"
        );
        assert_eq!(working(&state).roles.get(Role::Accent), hex("#00FF00"));
        let effects = update(&mut state, Msg::Picker(ColorPickerUpdate::ActionFinished));
        let applies = live_applies(&effects);
        assert_eq!(applies.len(), 1, "{effects:?}");
        assert_eq!(applies[0].roles.get(Role::Accent), hex("#00FF00"));
        drive_effects(&mut state, effects);
        assert!(fs::read_to_string(&config).unwrap().contains("#00FF00"));

        // A colour typed in the picker applies when it is applied.
        drive(
            &mut state,
            Msg::Picker(ColorPickerUpdate::Input("#0000FF".to_string())),
        );
        let effects = update(&mut state, Msg::Picker(ColorPickerUpdate::AppliedColor));
        assert_eq!(live_applies(&effects).len(), 1, "{effects:?}");
        drive_effects(&mut state, effects);

        // SetRole applies at once too.
        let effects = update(&mut state, Msg::SetRole(Role::Success, red()));
        assert_eq!(live_applies(&effects).len(), 1, "{effects:?}");
    }

    #[test]
    fn pin_applies_now() {
        let tmp = TempDir::new().unwrap();
        let (mut state, _) = live_on(&tmp);
        let bright_red = Slot::Ansi(Tone::Bright, Ansi::Red);
        let effects = update(&mut state, Msg::Pin(bright_red));
        let applies = live_applies(&effects);
        assert_eq!(applies.len(), 1, "{effects:?}");
        assert!(applies[0].pins.is_pinned(bright_red));
        drive_effects(&mut state, effects);

        let effects = update(&mut state, Msg::Unpin(bright_red));
        assert_eq!(live_applies(&effects).len(), 1, "{effects:?}");
        drive_effects(&mut state, effects);
        let effects = update(&mut state, Msg::NoBackground(true));
        assert_eq!(live_applies(&effects).len(), 1, "{effects:?}");
        drive_effects(&mut state, effects);
        // Pinning what is pinned changes nothing, so nothing is applied.
        let effects = update(&mut state, Msg::NoBackground(true));
        assert!(live_applies(&effects).is_empty(), "{effects:?}");
    }

    #[test]
    fn applies_run_one_at_a_time() {
        let tmp = TempDir::new().unwrap();
        let (mut state, config) = live_on(&tmp);
        let first = update(&mut state, Msg::SetRole(Role::Accent, red()));
        assert_eq!(live_applies(&first).len(), 1);
        // While it runs, edits wait; the newest runs next.
        let blue = hex("#0000FF");
        assert!(update(&mut state, Msg::SetRole(Role::Accent, hex("#00FF00"))).is_empty());
        assert!(update(&mut state, Msg::SetRole(Role::Accent, blue)).is_empty());
        let [effect] = <[Effect; 1]>::try_from(first).unwrap();
        let msgs = perform(effect, &state.ctx);
        let next = update(&mut state, msgs.into_iter().next().unwrap());
        let applies = live_applies(&next);
        assert_eq!(applies.len(), 1, "{next:?}");
        assert_eq!(applies[0].roles.get(Role::Accent), blue);
        drive_effects(&mut state, next);
        assert!(fs::read_to_string(&config).unwrap().contains("#0000FF"));

        // Revert asked while an apply runs waits for it, and drops the
        // edits still waiting.
        let running = update(&mut state, Msg::SetRole(Role::Accent, red()));
        assert_eq!(live_applies(&running).len(), 1);
        assert!(update(&mut state, Msg::SetRole(Role::Accent, hex("#00FF00"))).is_empty());
        drive(&mut state, Msg::LiveToggle(false));
        assert!(update(&mut state, Msg::LiveRevert).is_empty());
        // Closing meanwhile waits for the revert, then asks about the
        // unsaved edits.
        assert!(update(&mut state, Msg::CloseRequested).is_empty());
        assert!(state.dialog.is_none());
        let [effect] = <[Effect; 1]>::try_from(running).unwrap();
        let msgs = perform(effect, &state.ctx);
        let next = update(&mut state, msgs.into_iter().next().unwrap());
        assert!(
            matches!(next.as_slice(), [Effect::RevertLive(_)]),
            "{next:?}"
        );
        drive_effects(&mut state, next);
        assert_eq!(fs::read(&config).unwrap(), b"x\n");
        assert!(!state.live_on());
        assert!(!live_marker(&tmp).exists());
        assert_eq!(state.dialog, Some(Dialog::Unsaved(Leave::Exit)));
    }

    #[test]
    fn off_asks_keep_or_revert() {
        let tmp = TempDir::new().unwrap();
        let (mut state, config) = live_on(&tmp);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::SetRole(Role::Error, hex("#00FF00")));
        assert!(drive(&mut state, Msg::LiveToggle(false)).is_empty());
        let asked = Some(Dialog::KeepOrRevert {
            then: None,
            pending: false,
        });
        assert_eq!(state.dialog, asked);
        assert!(state.live_on(), "on until the user chooses");

        // Cancel keeps it on.
        drive(&mut state, Msg::CloseDialog);
        assert!(state.dialog.is_none());
        assert!(state.live_on());
        assert!(live_marker(&tmp).exists());

        // Undo waits for the session to end.
        drive(&mut state, Msg::UndoLastApply);
        assert_eq!(last_toast(&state), LIVE_BLOCKS_UNDO);

        // Keep leaves the colours, and one undo puts back the files from
        // before the session.
        drive(&mut state, Msg::LiveToggle(false));
        drive(&mut state, Msg::LiveKeep);
        assert!(state.dialog.is_none());
        assert!(!state.live_on());
        assert!(!live_marker(&tmp).exists());
        assert!(fs::read_to_string(&config).unwrap().contains("#FF0000"));
        assert!(last_toast(&state).starts_with("Kept the live colours"));
        drive(&mut state, Msg::UndoLastApply);
        assert_eq!(fs::read(&config).unwrap(), b"x\n");
    }

    #[test]
    fn revert_restores() {
        let tmp = TempDir::new().unwrap();
        let (mut state, config) = live_on(&tmp);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        let g = timer(&update(&mut state, Msg::SetMode(Mode::Light)));
        drive(&mut state, Msg::LiveTick(g));
        assert!(fs::read_to_string(&config).unwrap().contains("#FF0000"));
        drive(&mut state, Msg::LiveToggle(false));
        drive(&mut state, Msg::LiveRevert);
        assert_eq!(fs::read(&config).unwrap(), b"x\n");
        assert!(!state.live_on());
        assert!(state.dialog.is_none());
        assert!(!live_marker(&tmp).exists());
        assert_eq!(last_toast(&state), "Reverted the live colours");
        // The edits stay in the editor, unsaved.
        assert!(state.dirty());
        assert_eq!(working(&state).roles.get(Role::Accent), red());

        // A new session can start after it.
        drive(&mut state, Msg::LiveToggle(true));
        assert!(state.live_on());
        assert!(fs::read_to_string(&config).unwrap().contains("#FF0000"));
    }

    #[test]
    fn pending_offer_at_start() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember", "noir"]);
        let config = put_cava(&tmp);
        // A run that applied live, then stopped without Keep or Revert.
        let ctx = ctx(&tmp);
        let noir = starter("noir");
        let mut session = irida_core::live::LiveSession::start(&ctx, "noir").unwrap();
        let req = irida_core::apply::ApplyRequest {
            palette: &noir,
            pair: None,
            only: None,
            live_only: false,
        };
        session.apply(&ctx, &Settings::default(), &req).unwrap();
        drop(session);
        assert_ne!(fs::read(&config).unwrap(), b"x\n");

        let mut state = start(&tmp);
        let asked = Some(Dialog::KeepOrRevert {
            then: None,
            pending: true,
        });
        assert_eq!(state.dialog, asked);
        assert_eq!(open_slug(&state), Some("ember"));
        assert!(state.live_on());
        // It is not dismissed without a choice, and edits are not applied
        // into the old session.
        drive(&mut state, Msg::CloseDialog);
        assert_eq!(state.dialog, asked);
        let effects = update(&mut state, Msg::SetRole(Role::Accent, red()));
        assert!(live_applies(&effects).is_empty(), "{effects:?}");
        // Closing the window keeps the prompt, and closes after it.
        assert!(drive(&mut state, Msg::CloseRequested).is_empty());
        assert_eq!(
            state.dialog,
            Some(Dialog::KeepOrRevert {
                then: Some(LiveThen::Leave(Leave::Exit)),
                pending: true,
            })
        );

        drive(&mut state, Msg::LiveRevert);
        assert_eq!(fs::read(&config).unwrap(), b"x\n");
        // The edit made meanwhile is asked about next.
        assert_eq!(state.dialog, Some(Dialog::Unsaved(Leave::Exit)));
        assert!(!state.live_on());
        assert!(!live_marker(&tmp).exists());
        drop(state);
        let state = start(&tmp);
        assert!(state.dialog.is_none(), "{:?}", state.dialog);
        assert!(!state.live_on());
    }

    #[test]
    fn switch_and_close_ask_first() {
        let tmp = TempDir::new().unwrap();
        let (mut state, config) = live_on(&tmp);

        // Switch: Keep or Revert first, then the list.
        drive(&mut state, Msg::ShowSwitch);
        assert_eq!(
            state.dialog,
            Some(Dialog::KeepOrRevert {
                then: Some(LiveThen::Switch),
                pending: false,
            })
        );
        drive(&mut state, Msg::LiveKeep);
        assert!(!state.live_on());
        assert_eq!(state.dialog, Some(Dialog::Switch));
        drive(&mut state, Msg::CloseDialog);

        // Close with unsaved edits: Keep or Revert, then the unsaved question.
        // The kept colours are what a later Revert goes back to.
        let kept = fs::read(&config).unwrap();
        assert_ne!(kept, b"x\n");
        drive(&mut state, Msg::LiveToggle(true));
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        assert!(fs::read_to_string(&config).unwrap().contains("#FF0000"));
        assert!(drive(&mut state, Msg::CloseRequested).is_empty());
        assert_eq!(
            state.dialog,
            Some(Dialog::KeepOrRevert {
                then: Some(LiveThen::Leave(Leave::Exit)),
                pending: false,
            })
        );
        // Cancel stays open and on.
        drive(&mut state, Msg::CloseDialog);
        assert!(state.live_on());
        drive(&mut state, Msg::CloseRequested);
        let ui = drive(&mut state, Msg::LiveRevert);
        assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
        assert_eq!(fs::read(&config).unwrap(), kept);
        assert_eq!(state.dialog, Some(Dialog::Unsaved(Leave::Exit)));
        let ui = drive(&mut state, Msg::UnsavedDiscard);
        assert!(matches!(ui.as_slice(), [Effect::Exit]), "{ui:?}");

        // Delete and Apply ask first too.
        drive(&mut state, Msg::LiveToggle(true));
        drive(&mut state, Msg::ShowDelete);
        assert_eq!(
            state.dialog,
            Some(Dialog::KeepOrRevert {
                then: Some(LiveThen::Delete),
                pending: false,
            })
        );
        drive(&mut state, Msg::LiveKeep);
        assert!(matches!(state.dialog, Some(Dialog::Delete { .. })));
        drive(&mut state, Msg::CloseDialog);
        drive(&mut state, Msg::LiveToggle(true));
        drive(&mut state, Msg::ShowApply);
        assert_eq!(
            state.dialog,
            Some(Dialog::KeepOrRevert {
                then: Some(LiveThen::Apply),
                pending: false,
            })
        );
        drive(&mut state, Msg::LiveRevert);
        assert!(matches!(state.dialog, Some(Dialog::Apply(_))));
        drive(&mut state, Msg::CloseDialog);

        // So does anything else that leaves the palette, like New.
        drive(&mut state, Msg::LiveToggle(true));
        drive(&mut state, Msg::ShowNew);
        drive(&mut state, Msg::NameInput("Moss".to_string()));
        drive(&mut state, Msg::NameConfirm);
        assert!(matches!(
            state.dialog,
            Some(Dialog::KeepOrRevert {
                then: Some(LiveThen::Leave(Leave::Create(_))),
                ..
            })
        ));
        drive(&mut state, Msg::LiveRevert);
        assert!(matches!(
            state.dialog,
            Some(Dialog::Unsaved(Leave::Create(_)))
        ));
        drive(&mut state, Msg::UnsavedDiscard);
        assert_eq!(open_slug(&state), Some("moss"));
        assert!(!state.live_on());
    }

    #[test]
    fn failed_save_asks_about_a_live_session_first() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember", "noir"]);
        put_cava(&tmp);
        let mut state = start(&tmp);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::CloseRequested);
        assert_eq!(state.dialog, Some(Dialog::Unsaved(Leave::Exit)));
        let running = update(&mut state, Msg::UnsavedSave);
        assert!(matches!(running.as_slice(), [Effect::SaveVersion { .. }]));

        // Try it live starts while the save runs, then the save fails: Keep
        // or Revert comes first, not a Discard that exits with it still on.
        drive(&mut state, Msg::LiveToggle(true));
        assert!(state.live_on());
        let ui = drive(&mut state, Msg::Saved(Err("disk full".to_string())));
        assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
        assert_eq!(last_toast(&state), "Not saved: disk full");
        assert_eq!(
            state.dialog,
            Some(Dialog::KeepOrRevert {
                then: Some(LiveThen::Leave(Leave::Exit)),
                pending: false,
            })
        );
        drive(&mut state, Msg::LiveRevert);
        assert!(!state.live_on());
        assert_eq!(state.dialog, Some(Dialog::Unsaved(Leave::Exit)));
        let ui = drive(&mut state, Msg::UnsavedDiscard);
        assert!(matches!(ui.as_slice(), [Effect::Exit]), "{ui:?}");
    }

    #[test]
    fn newest_answer_decides_with_try_it_live_on() {
        for ok in [true, false] {
            let tmp = TempDir::new().unwrap();
            store_with(&tmp, &["ember", "noir"]);
            put_cava(&tmp);
            let mut state = start(&tmp);
            drive(&mut state, Msg::SetRole(Role::Accent, red()));
            drive(&mut state, Msg::CloseRequested);
            let running = update(&mut state, Msg::UnsavedSave);
            assert!(matches!(running.as_slice(), [Effect::SaveVersion { .. }]));

            // While the save runs, Try it live goes on and New asks Keep or
            // Revert: that answer replaces Save then exit.
            drive(&mut state, Msg::LiveToggle(true));
            drive(&mut state, Msg::ShowNew);
            drive(&mut state, Msg::NameInput("Moss".to_string()));
            drive(&mut state, Msg::NameConfirm);
            let create = state.dialog.clone();
            assert!(
                matches!(
                    &create,
                    Some(Dialog::KeepOrRevert {
                        then: Some(LiveThen::Leave(Leave::Create(p))),
                        pending: false,
                    }) if p.slug == "moss"
                ),
                "{create:?}"
            );
            let ui = if ok {
                drive_effects(&mut state, running)
            } else {
                drive(&mut state, Msg::Saved(Err("disk full".to_string())))
            };
            assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
            assert_eq!(state.dialog, create);
            let ui = drive(&mut state, Msg::LiveKeep);
            assert!(!ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
            assert!(!state.live_on());
            if ok {
                assert_eq!(open_slug(&state), Some("moss"));
            } else {
                assert!(
                    matches!(&state.dialog, Some(Dialog::Unsaved(Leave::Create(p))) if p.slug == "moss"),
                    "{:?}",
                    state.dialog
                );
            }
        }
    }

    #[test]
    fn finished_import_keeps_save_then_exit() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        drive(&mut state, Msg::CloseRequested);
        let running = update(&mut state, Msg::UnsavedSave);
        assert!(matches!(running.as_slice(), [Effect::SaveVersion { .. }]));

        // An import started earlier finishes while the save runs: it asks to
        // open what it wrote, but the save still exits.
        drive_effects(
            &mut state,
            vec![Effect::SaveImport {
                palette: starter("noir"),
                notes: Vec::new(),
                replace: false,
            }],
        );
        assert!(store.load("noir").is_ok());
        assert_eq!(
            state.dialog,
            Some(Dialog::Unsaved(Leave::Open("noir".to_string())))
        );
        let ui = drive_effects(&mut state, running);
        assert!(ui.iter().any(|e| matches!(e, Effect::Exit)), "{ui:?}");
        assert!(state.dialog.is_none());
    }

    #[test]
    fn second_window_leaves_the_first_windows_session() {
        let tmp = TempDir::new().unwrap();
        let (first, config) = live_on(&tmp);
        let marker = fs::read(live_marker(&tmp)).unwrap();
        let applied = fs::read(&config).unwrap();

        let (mut second, effects) = State::new(
            ctx(&tmp),
            tmp.path().join("system"),
            irida_core::live::lock_app(&Dirs::under(tmp.path())),
        );
        let mut queue: Vec<Msg> = effects
            .into_iter()
            .flat_map(|e| perform(e, &second.ctx))
            .collect();
        let mut asked = Vec::new();
        for msg in queue.drain(..) {
            asked.extend(update(&mut second, msg));
        }
        assert!(
            !asked.iter().any(|e| matches!(e, Effect::FindPendingLive)),
            "{asked:?}"
        );
        drive_effects(&mut second, asked);

        assert_eq!(open_slug(&second), Some("ember"));
        assert!(second.dialog.is_none(), "{:?}", second.dialog);
        assert!(!second.live_on());
        assert!(!second.header_enabled(HeaderItem::TryLive));
        assert_eq!(
            second.header_tooltip(HeaderItem::TryLive),
            Some(LIVE_IN_OTHER_WINDOW)
        );
        assert!(drive(&mut second, Msg::LiveToggle(true)).is_empty());
        assert!(!second.live_on());
        // The first window's session is untouched, and it still works.
        assert_eq!(fs::read(live_marker(&tmp)).unwrap(), marker);
        assert_eq!(fs::read(&config).unwrap(), applied);
        assert!(first.live_on());
        // Editing and saving still work here.
        drive(&mut second, Msg::SetRole(Role::Accent, red()));
        drive(&mut second, Msg::Save);
        assert!(!second.dirty());

        // Once the first window has gone, a new start finds the session.
        drop(first);
        drop(second);
        let state = start(&tmp);
        assert!(
            matches!(
                state.dialog,
                Some(Dialog::KeepOrRevert { pending: true, .. })
            ),
            "{:?}",
            state.dialog
        );
    }

    #[test]
    fn toggle_needs_saved_palette_and_live_target() {
        // No live target installed.
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        put_a(&tmp);
        let mut state = start(&tmp);
        assert!(!state.header_enabled(HeaderItem::TryLive));
        assert_eq!(
            state.header_tooltip(HeaderItem::TryLive),
            Some(LIVE_NEEDS_TARGET)
        );
        assert!(drive(&mut state, Msg::LiveToggle(true)).is_empty());
        assert!(!state.live_on());
        assert!(!live_marker(&tmp).exists());

        // A draft that was never saved.
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        put_cava(&tmp);
        let mut state = start(&tmp);
        assert!(state.header_enabled(HeaderItem::TryLive));
        let mut draft = starter("noir");
        draft.slug = "moss".to_string();
        draft.name = "Moss".to_string();
        state.current = Some(Current::Editable {
            slug: "moss".to_string(),
            saved: None,
            working: draft,
        });
        assert!(!state.header_enabled(HeaderItem::TryLive));
        assert_eq!(
            state.header_tooltip(HeaderItem::TryLive),
            Some(LIVE_NEEDS_SAVE)
        );
        drive(&mut state, Msg::LiveToggle(true));
        assert!(!state.live_on());

        // A palette that cannot be read.
        state.current = Some(Current::Unreadable {
            slug: "broken".to_string(),
            error: "bad".to_string(),
            text: String::new(),
        });
        assert!(!state.header_enabled(HeaderItem::TryLive));
        drive(&mut state, Msg::LiveToggle(true));
        assert!(!state.live_on());
        assert!(!live_marker(&tmp).exists());
    }
    #[test]
    fn failed_save_can_be_saved_again() {
        let tmp = TempDir::new().unwrap();
        let store = store_with(&tmp, &["ember"]);
        let mut state = start(&tmp);
        drive(&mut state, Msg::SetRole(Role::Accent, red()));
        let effects = update(&mut state, Msg::Save);
        let [effect @ Effect::SaveVersion { .. }] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        let msg = panicked(effect.clone(), &state.ctx, "boom".to_string());
        let ui = drive(&mut state, msg);
        assert!(matches!(ui.as_slice(), [Effect::Toast(t)] if t.text.contains("boom")));
        assert!(state.dirty());
        assert_eq!(store.versions("ember").unwrap().len(), 1);

        // Not left saving: Ctrl+S saves.
        drive(&mut state, Msg::Save);
        assert!(!state.dirty());
        assert_eq!(store.versions("ember").unwrap().len(), 2);
    }

    #[test]
    fn failed_live_apply_still_reverts() {
        let tmp = TempDir::new().unwrap();
        store_with(&tmp, &["ember"]);
        let config = put_cava(&tmp);
        let mut state = start(&tmp);
        let effects = update(&mut state, Msg::LiveToggle(true));
        let [start_live @ Effect::StartLive(_)] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        let started = perform(start_live.clone(), &state.ctx);
        let effects: Vec<Effect> = started
            .into_iter()
            .flat_map(|m| update(&mut state, m))
            .collect();
        let [apply @ Effect::LiveApply { .. }] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        // The first apply writes, then its Msg is lost to a panic.
        perform(apply.clone(), &state.ctx);
        assert_ne!(fs::read_to_string(&config).unwrap(), "x\n");
        let msg = panicked(apply.clone(), &state.ctx, "boom".to_string());
        let ui = drive(&mut state, msg);
        assert!(matches!(ui.as_slice(), [Effect::Toast(t)] if t.text.contains("boom")));

        // Not left busy, and Revert knows the record the apply made.
        drive(&mut state, Msg::LiveToggle(false));
        drive(&mut state, Msg::LiveRevert);
        assert!(!state.live_on());
        assert_eq!(fs::read_to_string(&config).unwrap(), "x\n");
        assert!(!live_marker(&tmp).exists());
    }
}
