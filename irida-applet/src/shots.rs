//! A README screenshot of the popup, drawn headless with the software
//! renderer in COSMIC's stock dark theme, as the window's shots are. Run with
//! `IRIDA_SHOTS=<folder> cargo test -p irida-applet shots -- --ignored`.

use std::fs;
use std::path::PathBuf;

use cosmic::Theme;
use cosmic::iced::{Font, Size};
use iced_test::simulator::Simulator;
use irida_core::apply::Ctx;
use irida_core::dirs::Dirs;
use irida_core::store::{Settings, Store, starters};
use tempfile::TempDir;

use crate::actions;
use crate::menu::Kind;
use crate::window::popup_body;

/// The four starters, Ember paired with Ember Day as shipped and the other
/// pair cut so every section shows, with Ember applied last.
fn loaded(tmp: &TempDir) -> actions::Loaded {
    let dirs = Dirs::under(tmp.path());
    let store = Store::new(dirs.clone());
    for mut p in starters() {
        if p.slug == "noir" || p.slug == "dossier" {
            p.pair = None;
        }
        store.save_version(&p).unwrap();
    }
    fs::create_dir_all(dirs.config.join("cosmic")).unwrap();
    let ctx = Ctx { dirs, path: vec![] };
    let ember = Kind::Single(Box::new(store.load("ember").unwrap()));
    actions::apply_row(&ctx, &Settings::default(), &ember, None).unwrap();
    actions::load(&ctx)
}

#[test]
#[ignore = "writes a README screenshot; set IRIDA_SHOTS"]
fn shots() {
    let folder = PathBuf::from(std::env::var_os("IRIDA_SHOTS").expect("IRIDA_SHOTS is set"));
    fs::create_dir_all(&folder).unwrap();
    let tmp = TempDir::new().unwrap();
    let l = loaded(&tmp);
    assert_eq!(l.menu.pairs.len(), 1);
    assert_eq!((l.menu.dark.len(), l.menu.light.len()), (1, 1));
    assert!(l.menu.pairs[0].ticked);

    let body = popup_body(Some(&l), None, Some("Applied Ember \u{b7} Ember Day"));
    let settings = iced_test::core::Settings {
        default_font: Font::with_name("Open Sans"),
        ..iced_test::core::Settings::default()
    };
    let mut sim = Simulator::with_size(settings, Size::new(320., 372.), body);
    let snapshot = sim.snapshot(&Theme::dark()).expect("a snapshot");
    let path = folder.join("applet");
    // The renderer adds its own suffix; an older image there would be compared
    // against instead of replaced.
    let _ = fs::remove_file(folder.join("applet-tiny-skia.png"));
    assert!(snapshot.matches_image(&path).expect("the PNG is written"));
}
