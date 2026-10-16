mod output;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use irida_core::apply::{self, ApplyError, ApplyRequest, Ctx};
use irida_core::contrast;
use irida_core::dirs::Dirs;
use irida_core::palette::Palette;
use irida_core::store::Store;

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
        /// Palette slug or path to a palette file
        palette: String,
    },
    /// List stored palettes
    List,
    /// Apply a palette to every installed target
    Apply {
        /// Palette slug or path to a palette file
        palette: String,
        /// Apply only these targets (comma separated), even when turned off
        #[arg(long, value_delimiter = ',')]
        only: Option<Vec<String>>,
    },
    /// Undo the most recent apply
    Undo,
}

fn store() -> Result<Store, String> {
    Dirs::from_env().map(Store::new).map_err(|e| e.to_string())
}

/// The palette's pair, when it names one that exists with the opposite mode.
/// Otherwise a warning goes to stderr and the palette is applied alone.
fn load_pair(store: &Store, p: &Palette) -> Option<Palette> {
    let slug = p.pair.as_deref()?;
    match store.load(slug) {
        Ok(pair) if pair.mode != p.mode => Some(pair),
        Ok(_) => {
            eprintln!(
                "irida: warning: pair {slug} has the same mode as {}; applying alone",
                p.slug
            );
            None
        }
        Err(e) => {
            eprintln!("irida: warning: pair {slug} not available ({e}); applying alone");
            None
        }
    }
}

fn run_apply(store: Store, palette: &str, only: Option<Vec<String>>) -> Result<ExitCode, String> {
    let ctx = Ctx::from_env().map_err(|e| e.to_string())?;
    let p = store.resolve(palette).map_err(|e| e.to_string())?;
    let pair = load_pair(&store, &p);
    let settings = store.settings().map_err(|e| e.to_string())?;
    let req = ApplyRequest {
        palette: &p,
        pair: pair.as_ref(),
        only,
    };
    match apply::apply(&ctx, &settings, &req) {
        Ok(report) => {
            let mut settings = settings;
            settings.last_palette = Some(p.slug.clone());
            print!("{}", output::format_apply(&report));
            if let Err(e) = store.save_settings(&settings) {
                eprintln!("irida: warning: could not save settings: {e}");
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

fn run(cli: Cli) -> Result<ExitCode, String> {
    let store = store()?;
    match cli.command {
        Command::Apply { palette, only } => run_apply(store, &palette, only),
        Command::Undo => run_undo(),
        Command::Check { palette } => {
            let p = store.resolve(&palette).map_err(|e| e.to_string())?;
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
