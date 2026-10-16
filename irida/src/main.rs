mod output;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use irida_core::contrast;
use irida_core::dirs::Dirs;
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
}

fn store() -> Result<Store, String> {
    Dirs::from_env().map(Store::new).map_err(|e| e.to_string())
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    let store = store()?;
    match cli.command {
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
