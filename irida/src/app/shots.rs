//! README screenshots, drawn headless with the software renderer in COSMIC's
//! stock dark theme. Run with
//! `IRIDA_SHOTS=<folder> cargo test -p irida shots -- --ignored`.

use std::fs;
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
use irida_core::role::Role;

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
    open(tmp, dirs)
}

/// Like `start`, with `image` stored on `slug` so the preview draws over it.
fn start_with_image(tmp: &TempDir, slug: &str, image: &Path) -> State {
    let dirs = Dirs::under(tmp.path());
    let store = Store::new(dirs.clone());
    for mut p in starters() {
        if p.slug == slug {
            p.image = Some(image.to_str().expect("the image path is utf-8").to_string());
        }
        store.save_version(&p).unwrap();
    }
    store
        .save_settings(&Settings {
            last_palette: Some(slug.to_string()),
            ..Settings::default()
        })
        .unwrap();
    open(tmp, dirs)
}

/// First run: no settings file, COSMIC wallpaper set to the two-tone vector.
fn first_run(tmp: &TempDir) -> State {
    let dirs = Dirs::under(tmp.path());
    let store = Store::new(dirs.clone());
    for p in starters() {
        store.save_version(&p).unwrap();
    }
    let image = tmp.path().join("pictures/two-tone.png");
    fs::create_dir_all(image.parent().unwrap()).unwrap();
    vector_png("two-tone", &image);
    cosmic_wallpaper(tmp, &image);
    open(tmp, dirs)
}

fn open(tmp: &TempDir, dirs: Dirs) -> State {
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
    drive(&mut state, Msg::Select(Role::Warning));
    shoot(&state, &folder, "editor-role");
    drive(&mut state, Msg::CloseEditor);
    drive(&mut state, Msg::ShowApply);
    shoot(&state, &folder, "apply");

    let tmp = TempDir::new().unwrap();
    let state = start(&tmp, "emberday");
    shoot(&state, &folder, "editor-light");

    let tmp = TempDir::new().unwrap();
    let state = first_run(&tmp);
    shoot(&state, &folder, "first-run");

    let tmp = TempDir::new().unwrap();
    let image = tmp.path().join("pictures/two-tone.png");
    fs::create_dir_all(image.parent().unwrap()).unwrap();
    vector_png("two-tone", &image);
    let state = start_with_image(&tmp, "ember", &image);
    shoot(&state, &folder, "editor-image");
}

const VECTORS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../irida-core/tests/fixtures/draft/vectors.json"
);

/// The reference vector `case` written as a 40 by 25 PNG at `path`.
fn vector_png(case: &str, path: &Path) {
    use base64::Engine;
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(VECTORS).unwrap()).unwrap();
    let rgba = base64::engine::general_purpose::STANDARD
        .decode(v[case]["rgba"].as_str().unwrap())
        .unwrap();
    image::RgbaImage::from_raw(40, 25, rgba)
        .unwrap()
        .save(path)
        .unwrap();
}

/// COSMIC's `all` background key under the temp config, pointing at `image`.
fn cosmic_wallpaper(tmp: &TempDir, image: &Path) {
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
