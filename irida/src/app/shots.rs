//! README screenshots, drawn headless with the software renderer in COSMIC's
//! stock dark theme. Run with
//! `IRIDA_SHOTS=<folder> cargo test -p irida shots -- --ignored`.

use std::path::{Path, PathBuf};

use cosmic::iced::{Font, Length, Size};
use cosmic::widget::{self, popover};
use cosmic::{Element, Theme};
use iced_test::simulator::Simulator;
use irida_core::apply::Ctx;
use irida_core::dirs::Dirs;
use irida_core::store::{Settings, Store, starters};
use tempfile::TempDir;

use super::effects::{drive, drive_effects};
use super::state::{Msg, State};
use super::{content, header};

/// A started app on the starter palettes, opened on `slug`. Targets are
/// detected from the real `$PATH`, so the Apply sheet lists real apps.
fn start(tmp: &TempDir, slug: &str) -> State {
    let dirs = Dirs::under(tmp.path());
    let store = Store::new(dirs.clone());
    for p in starters() {
        store.save_version(&p).unwrap();
    }
    store
        .save_settings(&Settings {
            last_palette: Some(slug.to_string()),
            ..Settings::default()
        })
        .unwrap();
    let path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let lock = irida_core::live::lock_app(&dirs);
    let (mut state, effects) = State::new(Ctx { dirs, path }, tmp.path().join("system"), lock);
    drive_effects(&mut state, effects);
    state
}

/// The window as libcosmic draws it: header bar, content, and any dialog on top.
fn window(state: &State) -> Element<'_, Msg> {
    let mut bar = widget::header_bar()
        .title("Irida")
        .focused(true)
        .on_close(Msg::CloseRequested)
        .on_minimize(Msg::CloseRequested)
        .on_maximize(Msg::CloseRequested);
    for e in header::start(state) {
        bar = bar.start(e);
    }
    for e in header::end(state) {
        bar = bar.end(e);
    }
    let body = widget::Column::new().push(bar).push(
        widget::container(content(state))
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill),
    );
    let body = widget::container(body)
        .class(cosmic::theme::Container::Background)
        .width(Length::Fill)
        .height(Length::Fill);
    let mut layered = popover(body).modal(true);
    if let Some(dialog) = header::dialog(state) {
        layered = layered.popup(dialog);
    }
    layered.into()
}

fn shoot(state: &State, folder: &Path, name: &str) {
    let settings = iced_test::core::Settings {
        default_font: Font::with_name("Open Sans"),
        ..iced_test::core::Settings::default()
    };
    let mut sim = Simulator::with_size(settings, Size::new(1200., 780.), window(state));
    let snapshot = sim.snapshot(&Theme::dark()).expect("a snapshot");
    let path = folder.join(name);
    let _ = std::fs::remove_file(path.with_extension("png"));
    assert!(snapshot.matches_image(&path).expect("the PNG is written"));
}

#[test]
#[ignore = "writes README screenshots; set IRIDA_SHOTS"]
fn shots() {
    let folder = PathBuf::from(std::env::var_os("IRIDA_SHOTS").expect("IRIDA_SHOTS is set"));
    std::fs::create_dir_all(&folder).unwrap();

    let tmp = TempDir::new().unwrap();
    let mut state = start(&tmp, "ember");
    shoot(&state, &folder, "editor-dark");
    drive(&mut state, Msg::ShowApply);
    shoot(&state, &folder, "apply");

    let tmp = TempDir::new().unwrap();
    let state = start(&tmp, "emberday");
    shoot(&state, &folder, "editor-light");
}
