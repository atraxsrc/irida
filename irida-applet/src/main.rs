//! The Irida panel applet: saved palettes in the COSMIC panel, applied with a
//! click. See `docs/superpowers/specs/2026-10-17-panel-applet-design.md`.

mod actions;
mod copy;
mod menu;
mod panel;
#[cfg(test)]
mod shots;
mod window;

use std::process::ExitCode;

use irida_core::apply::Ctx;

fn main() -> ExitCode {
    let ctx = match Ctx::for_app() {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("irida-applet: {e}");
            return ExitCode::from(2);
        }
    };
    match cosmic::applet::run::<window::Window>(ctx) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("irida-applet: {e}");
            ExitCode::from(1)
        }
    }
}
