mod app;
mod output;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use irida_core::apply::{self, ApplyError, ApplyRequest, Ctx};
use irida_core::contrast;
use irida_core::dirs::Dirs;
use irida_core::draft;
use irida_core::export::{ExportRequest, export};
use irida_core::import;
use irida_core::palette::{Mode, Palette, slug_from_name, validate_name};
use irida_core::store::{PairLookup, Store, is_saved_slug};
use irida_core::wallpaper;

#[derive(Parser)]
#[command(name = "irida", version, about = "Colour palettes for COSMIC")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ModeArg {
    Dark,
    Light,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Mode {
        match m {
            ModeArg::Dark => Mode::Dark,
            ModeArg::Light => Mode::Light,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Check a palette's contrast against its background and pane
    Check {
        /// Palette slug, palette file or rice folder
        palette: String,
    },
    /// List stored palettes
    List,
    /// Apply a palette to every installed target
    Apply {
        /// Palette slug, palette file or rice folder
        palette: String,
        /// Apply only these targets by id (comma separated), even when turned
        /// off: cosmic-desktop, cosmic-term, cava, btop, nvim, fastfetch, lsd,
        /// zsh, ghostty, delta, fish, eza, kitty, alacritty, foot, helix, bat, or
        /// a folder of your own under
        /// ~/.config/irida/targets/
        #[arg(long, value_delimiter = ',')]
        only: Option<Vec<String>>,
    },
    /// Undo the most recent apply
    Undo,
    /// Export a palette as a cosmic-<slug>-theme rice repo
    Export {
        /// Palette slug, palette file or rice folder
        palette: String,
        /// The repo itself, or the folder to create it in
        folder: PathBuf,
    },
    /// Draft a palette from an image and save it
    Draft {
        /// A JPEG, PNG or WebP image
        image: PathBuf,
        /// The palette's mode (default: from the image's lightness)
        #[arg(long, value_enum)]
        mode: Option<ModeArg>,
        /// The palette's name (default: from the file name)
        #[arg(long)]
        name: Option<String>,
    },
}

/// The palette `arg` names, as `Store::resolve` finds it. A rice folder
/// without a `palette.toml` is imported here instead, so its notes (such as
/// a dropped alpha) reach stderr.
fn resolve(store: &Store, arg: &str) -> Result<Palette, String> {
    let path = Path::new(arg);
    if path.is_dir() && !path.join("palette.toml").is_file() && import::is_rice_folder(path) {
        let (p, notes) = import::import_rice_with_notes(path).map_err(|e| e.to_string())?;
        for note in notes {
            eprintln!("irida: note: {note}");
        }
        return Ok(p);
    }
    store.resolve(arg).map_err(|e| e.to_string())
}

/// The palette's pair, when it names one that exists with the opposite mode.
/// Otherwise a warning goes to stderr and the palette is used alone;
/// `doing` names what for ("applying", "exporting").
fn load_pair(store: &Store, p: &Palette, doing: &str) -> Option<Palette> {
    match store.pair_of(p) {
        PairLookup::Found(pair) => Some(pair),
        PairLookup::None => None,
        PairLookup::SameMode(slug) => {
            eprintln!(
                "irida: warning: pair {slug} has the same mode as {}; {doing} alone",
                p.slug
            );
            None
        }
        PairLookup::Missing { slug, reason } => {
            eprintln!("irida: warning: pair {slug} not available ({reason}); {doing} alone");
            None
        }
    }
}

fn run_apply(store: Store, palette: &str, only: Option<Vec<String>>) -> Result<ExitCode, String> {
    let ctx = Ctx::from_env().map_err(|e| e.to_string())?;
    let p = resolve(&store, palette)?;
    let pair = load_pair(&store, &p, "applying");
    let settings = store.settings().map_err(|e| e.to_string())?;
    let req = ApplyRequest {
        palette: &p,
        pair: pair.as_ref(),
        only,
        live_only: false,
    };
    match apply::apply(&ctx, &settings, &req) {
        Ok(report) => {
            print!("{}", output::format_apply(&report));
            if is_saved_slug(palette) {
                let mut settings = settings;
                settings.last_palette = Some(p.slug.clone());
                if let Err(e) = store.save_settings(&settings) {
                    eprintln!("irida: warning: could not save settings: {e}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(ApplyError::NothingToApply(results)) => {
            print!("{}", output::format_targets(&results));
            Err(ApplyError::NothingToApply(results).to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn run_undo() -> Result<ExitCode, String> {
    let ctx = Ctx::from_env().map_err(|e| e.to_string())?;
    let report = apply::undo(&ctx).map_err(|e| e.to_string())?;
    print!("{}", output::format_undo(&report));
    Ok(if report.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn run_export(dirs: &Dirs, store: Store, palette: &str, folder: &Path) -> Result<ExitCode, String> {
    let p = resolve(&store, palette)?;
    let pair = load_pair(&store, &p, "exporting");
    let req = ExportRequest {
        palette: &p,
        pair: pair.as_ref(),
        folder,
    };
    let report = export(dirs, &req).map_err(|e| e.to_string())?;
    print!("{}", output::format_export(&report, &p));
    Ok(ExitCode::SUCCESS)
}

fn run_draft(
    store: &Store,
    image: &Path,
    mode: Option<ModeArg>,
    name: Option<String>,
) -> Result<ExitCode, String> {
    let name = name.unwrap_or_else(|| wallpaper::name_from_file(image));
    validate_name(&name).map_err(|e| e.to_string())?;
    let slug = slug_from_name(&name);
    if slug.is_empty() {
        return Err(format!(
            "the name {name:?} has no letters to make a slug from"
        ));
    }
    if store.exists(&slug) {
        return Err(format!(
            "a palette called {slug} already exists; pass --name"
        ));
    }
    let pixels = wallpaper::load_pixels(image).map_err(|e| e.to_string())?;
    let clusters = draft::clusters(&pixels);
    let mode = mode.map_or_else(|| draft::mode_of(&clusters), Mode::from);
    let p = draft::draft_palette(&clusters, mode, &name);
    store.save_version(&p).map_err(|e| e.to_string())?;
    println!("saved {} ({}, {})", p.slug, p.name, p.mode);
    print!("{}", output::format_check(&p, &contrast::check(&p)));
    Ok(ExitCode::SUCCESS)
}

/// Whether a Wayland or X display is there to open the app on. An empty
/// value counts as unset.
fn has_display() -> bool {
    ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()))
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    let Some(command) = cli.command else {
        if !has_display() {
            return Err("no display found; run irida --help for the command line".to_string());
        }
        return app::run().map(|()| ExitCode::SUCCESS);
    };
    let dirs = Dirs::from_env().map_err(|e| e.to_string())?;
    let store = Store::new(dirs.clone());
    match command {
        Command::Draft { image, mode, name } => run_draft(&store, &image, mode, name),
        Command::Apply { palette, only } => run_apply(store, &palette, only),
        Command::Undo => run_undo(),
        Command::Export { palette, folder } => run_export(&dirs, store, &palette, &folder),
        Command::Check { palette } => {
            let p = resolve(&store, &palette)?;
            let report = contrast::check(&p);
            print!("{}", output::format_check(&p, &report));
            Ok(if report.failures().is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        Command::List => {
            for entry in store.list() {
                match entry.result {
                    Ok(p) => println!("{}  {} ({})", entry.slug, p.name, p.mode),
                    Err(e) => println!("{}  unreadable: {e}", entry.slug),
                }
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("irida: {message}");
            ExitCode::from(2)
        }
    }
}
