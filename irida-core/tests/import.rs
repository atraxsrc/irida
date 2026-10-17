mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{TestEnv, fixture, fixture_text, palette};
use irida_core::derive::derive;
use irida_core::export::{ExportRequest, ExportStatus, export};
use irida_core::import::{import_rice, import_rice_with_notes, is_rice_folder};
use irida_core::native::cosmic_desktop::export_ron;
use irida_core::role::Role;
use irida_core::store::{Store, StoreError};

const RICES: [(&str, &str); 4] = [
    ("ember", "Ember"),
    ("emberday", "EmberDay"),
    ("noir", "Noir"),
    ("dossier", "Dossier"),
];

fn repo(slug: &str) -> PathBuf {
    fixture(&format!("rices/cosmic-{slug}-theme"))
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A copy of a fixture repo inside the test's temporary folder.
fn copy_repo(env: &TestEnv, slug: &str) -> PathBuf {
    let to = env.tmp.path().join(format!("cosmic-{slug}-theme"));
    copy_dir(&repo(slug), &to);
    to
}

#[test]
fn imports_all_four() {
    for (slug, _) in RICES {
        let got = import_rice(&repo(slug)).unwrap_or_else(|e| panic!("{e}"));
        let mut want = palette(slug);
        // A name that is the role's own, such as Pane, is the default.
        for role in [Role::Pane, Role::Muted, Role::Comment] {
            want.names.remove(&role);
        }
        assert_eq!(got.name, want.name, "{slug}");
        assert_eq!(got.slug, want.slug, "{slug}");
        assert_eq!(got.mode, want.mode, "{slug}");
        assert_eq!(got.roles, want.roles, "{slug}");
        assert_eq!(got.names, want.names, "{slug}");
        assert_eq!(got.pins, want.pins, "{slug}");
        assert_eq!(got.pair, None, "{slug}");
        assert_eq!(got.notes, None, "{slug}");
    }
}

#[test]
fn terminal_round_trips() {
    for (slug, stem) in RICES {
        let p = import_rice(&repo(slug)).unwrap();
        assert_eq!(
            derive(&p).term.to_scheme(&p.name).to_ron() + "\n",
            fixture_text(&format!(
                "rices/cosmic-{slug}-theme/cosmic-term/{stem}-term.ron"
            )),
            "{slug}"
        );
    }
}

#[test]
fn desktop_round_trips_for_ember() {
    for (slug, stem) in [("ember", "Ember"), ("emberday", "EmberDay")] {
        let p = import_rice(&repo(slug)).unwrap();
        assert_eq!(
            export_ron(&p),
            fixture_text(&format!("rices/cosmic-{slug}-theme/cosmic/{stem}.ron")),
            "{slug}"
        );
    }
}

const NOIR_THEME: &str = "cosmic/config/com.system76.CosmicTerm/v1/syntax_theme_dark";

#[test]
fn noir_picks_named_scheme() {
    assert_eq!(import_rice(&repo("noir")).unwrap().name, "Noir");

    let env = TestEnv::new();
    let copy = copy_repo(&env, "noir");
    fs::write(copy.join(NOIR_THEME), "\"Noir Smoke\"").unwrap();
    let e = import_rice(&copy).unwrap_err().to_string();
    assert!(e.contains("rename the files"), "{e}");

    fs::write(copy.join(NOIR_THEME), "\"Nope\"").unwrap();
    let e = import_rice(&copy).unwrap_err().to_string();
    assert!(e.contains("Noir-term.ron"), "{e}");
    assert!(e.contains("Noir-term-smoke.ron"), "{e}");
}

#[test]
fn errors_name_the_problem() {
    let env = TestEnv::new();
    let copy = copy_repo(&env, "ember");
    fs::copy(copy.join("cosmic/Ember.ron"), copy.join("cosmic/Other.ron")).unwrap();
    let e = import_rice(&copy).unwrap_err().to_string();
    assert!(e.contains("Ember.ron") && e.contains("Other.ron"), "{e}");

    let plain = env.tmp.path().join("my-theme");
    copy_dir(&repo("ember"), &plain);
    assert!(!is_rice_folder(&plain));
    assert!(import_rice(&plain).is_err());

    let noir = copy_repo(&env, "noir");
    fs::remove_dir_all(noir.join("nvim")).unwrap();
    let e = import_rice(&noir).unwrap_err().to_string();
    assert!(e.contains("cmt"), "{e}");
}

#[test]
fn store_resolves_rice_folders() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    let ember = repo("ember");
    let p = store.resolve(ember.to_str().unwrap()).unwrap();
    assert_eq!(p.name, "Ember");
    assert_eq!(p.pins, palette("ember").pins);

    let dotted = format!("{}/.", ember.display());
    assert_eq!(store.resolve(&dotted).unwrap().name, "Ember");

    let empty = env.tmp.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    match store.resolve(empty.to_str().unwrap()) {
        Err(StoreError::NotFound(msg)) => {
            assert!(
                msg.ends_with(
                    "no palette.toml, and not a rice repo (the folder must be named cosmic-<slug>-theme)"
                ),
                "{msg}"
            );
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn export_then_import_keeps_names() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let emberday = palette("emberday");
    let parent = env.tmp.path().join("out");
    let report = export(
        &env.dirs,
        &ExportRequest {
            palette: &ember,
            pair: Some(&emberday),
            folder: &parent,
        },
    )
    .unwrap();
    assert!(!report.repo.join("scripts/contrast.py").exists());

    let mut want = ember.names.clone();
    for role in [Role::Pane, Role::Muted, Role::Comment] {
        want.remove(&role);
    }
    let (got, notes) = import_rice_with_notes(&report.repo).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(got.names, want);
    assert_eq!(got.roles, ember.roles);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(import_rice(&report.repo).unwrap().names, want);
}

#[test]
fn export_into_rice_with_contrast_py_imports_back() {
    let env = TestEnv::new();
    let copy = copy_repo(&env, "ember");
    let mut ember = palette("ember");
    ember.roles.set(
        Role::Accent,
        irida_core::color::Rgb::from_hex("#E0602A").unwrap(),
    );
    ember.names.insert(Role::Text, "Ink".into());
    ember.names.insert(Role::Container, "Soot / Ash".into());
    ember
        .names
        .insert(Role::Magenta, r#"Heather "wild" \ moor"#.into());
    let report = export(
        &env.dirs,
        &ExportRequest {
            palette: &ember,
            pair: None,
            folder: &copy,
        },
    )
    .unwrap();
    let line = |path: &str| {
        report
            .lines
            .iter()
            .find(|l| l.path == path)
            .map(|l| l.status.clone())
    };
    assert_eq!(line("scripts/contrast.py"), Some(ExportStatus::Updated));
    assert_eq!(
        line("README.md"),
        Some(ExportStatus::Skipped("no irida markers".into()))
    );
    let script = fs::read_to_string(copy.join("scripts/contrast.py")).unwrap();
    assert!(script.contains("def lum(h):"), "{script}");
    assert!(
        script.contains(r##"("Text", "Ink", "#D6D9E0", True),"##),
        "{script}"
    );
    assert!(
        script.contains(r##"("Containers", "Soot \x2f Ash / Pane", ["#22262E", "#1B1E24"]),"##),
        "{script}"
    );

    let mut want = ember.names.clone();
    for role in [Role::Pane, Role::Muted, Role::Comment] {
        want.remove(&role);
    }
    let got = import_rice(&copy).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(got.roles, ember.roles);
    assert_eq!(got.names, want);
}

#[test]
fn partial_names_round_trip_through_contrast_py() {
    let env = TestEnv::new();
    let copy = copy_repo(&env, "ember");
    let mut ember = palette("ember");
    ember.names.clear();
    ember.names.insert(Role::Text, "Ink".into());
    ember.names.insert(Role::Container, "Soot / Ash".into());
    export(
        &env.dirs,
        &ExportRequest {
            palette: &ember,
            pair: None,
            folder: &copy,
        },
    )
    .unwrap();
    let script = fs::read_to_string(copy.join("scripts/contrast.py")).unwrap();
    assert!(
        script.contains(r##"("Accent", "Accent", "#E8713A", True),"##),
        "{script}"
    );

    let got = import_rice(&copy).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(got.names, ember.names);
}

#[test]
fn control_and_blank_names_through_contrast_py() {
    let env = TestEnv::new();
    let copy = copy_repo(&env, "ember");
    let mut ember = palette("ember");
    ember.names.insert(Role::Text, "Ink\nDeep".into());
    export(
        &env.dirs,
        &ExportRequest {
            palette: &ember,
            pair: None,
            folder: &copy,
        },
    )
    .unwrap();
    let path = copy.join("scripts/contrast.py");
    let script = fs::read_to_string(&path).unwrap();
    // The newline is escaped, so the line stays one valid Python string.
    assert!(script.contains(r#"("Text", "Ink\x0aDeep", "#), "{script}");
    let got = import_rice(&copy).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        got.names.get(&Role::Text).map(String::as_str),
        Some("Ink\nDeep")
    );

    // A blank name in a hand-made script is no name, as in the README.
    let blank = script.replace(r#"("Text", "Ink\x0aDeep", "#, r#"("Text", " ", "#);
    assert_ne!(blank, script);
    fs::write(&path, blank).unwrap();
    let got = import_rice(&copy).unwrap_or_else(|e| panic!("{e}"));
    assert!(!got.names.contains_key(&Role::Text), "{:?}", got.names);
    got.validate().unwrap();
}

#[test]
fn contrast_py_of_another_shape_is_flagged() {
    let env = TestEnv::new();
    let copy = copy_repo(&env, "ember");
    let script = copy.join("scripts/contrast.py");
    fs::write(&script, "print('mine')\n").unwrap();
    let report = export(
        &env.dirs,
        &ExportRequest {
            palette: &palette("ember"),
            pair: None,
            folder: &copy,
        },
    )
    .unwrap();
    let line = report
        .lines
        .iter()
        .find(|l| l.path == "scripts/contrast.py")
        .map(|l| l.status.clone());
    assert_eq!(line, Some(ExportStatus::Stale));
    assert_eq!(fs::read_to_string(&script).unwrap(), "print('mine')\n");
}

#[test]
fn alpha_is_reported() {
    let env = TestEnv::new();
    let copy = copy_repo(&env, "noir");
    let scheme = copy.join("cosmic-term/Noir-term.ron");
    let text = fs::read_to_string(&scheme).unwrap();
    assert_eq!(text.matches("foreground: \"#CCD0CF\"").count(), 1);
    fs::write(
        &scheme,
        text.replace("foreground: \"#CCD0CF\"", "foreground: \"#CCD0CF80\""),
    )
    .unwrap();

    let (got, notes) = import_rice_with_notes(&copy).unwrap_or_else(|e| panic!("{e}"));
    let want = import_rice(&repo("noir")).unwrap();
    assert_eq!(got.roles.get(Role::Text).to_hex(), "#CCD0CF");
    assert_eq!(got.roles, want.roles);
    assert_eq!(got.pins, want.pins);
    assert_eq!(
        notes,
        ["foreground: alpha 128 in the terminal scheme was dropped; Irida colours are opaque"]
    );
    assert_eq!(import_rice(&copy).unwrap(), got);
}
