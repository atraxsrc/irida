mod output;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use irida_core::apply::{self, ApplyError, ApplyRequest, Ctx};
use irida_core::contrast;
use irida_core::dirs::Dirs;
use irida_core::export::{ExportRequest, export};
use irida_core::import;
use irida_core::palette::Palette;
use irida_core::store::{PairLookup, Store, is_saved_slug};

#[derive(Parser)]
#[command(name = "irida", version, about = "Colour palettes for COSMIC")]
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
        /// Apply only these targets (comma separated), even when turned off
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

fn run(cli: Cli) -> Result<ExitCode, String> {
    let dirs = Dirs::from_env().map_err(|e| e.to_string())?;
    let store = Store::new(dirs.clone());
    match cli.command {
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
