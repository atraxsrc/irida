mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use common::{TestEnv, fixture_text, palette};
use irida_core::apply::{ApplyRequest, Ctx, TargetStatus, apply, undo};
use irida_core::native::cosmic_desktop::{COLOR_KEYS, export_ron};
use irida_core::native::{NativePlan, export, plan};
use irida_core::palette::Palette;
use irida_core::store::Settings;
use irida_core::target::{NativeKind, TargetError};

const DARK_BUILDER: &str = "com.system76.CosmicTheme.Dark.Builder";
const DARK: &str = "com.system76.CosmicTheme.Dark";
const LIGHT_BUILDER: &str = "com.system76.CosmicTheme.Light.Builder";
const LIGHT: &str = "com.system76.CosmicTheme.Light";
const MODE: &str = "com.system76.CosmicTheme.Mode";

fn run(env: &TestEnv, palettes: &[&Palette]) -> Result<NativePlan, TargetError> {
    plan(NativeKind::CosmicDesktop, palettes, &env.dirs)
}

fn key_dir(env: &TestEnv, id: &str, version: u32) -> PathBuf {
    env.dirs
        .config
        .join("cosmic")
        .join(id)
        .join(format!("v{version}"))
}

/// Put a key file into the user's COSMIC config.
fn put(env: &TestEnv, id: &str, version: u32, key: &str, contents: &str) {
    let dir = key_dir(env, id, version);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(key), contents).unwrap();
}

fn under<'a>(env: &TestEnv, plan: &'a NativePlan, id: &str) -> BTreeMap<String, &'a str> {
    let dir = key_dir(env, id, if id == MODE { 1 } else { 2 });
    plan.writes
        .iter()
        .filter(|w| w.path.parent() == Some(dir.as_path()))
        .map(|w| {
            (
                w.path.file_name().unwrap().to_string_lossy().into_owned(),
                w.contents.as_str(),
            )
        })
        .collect()
}

#[test]
fn export_matches_rice() {
    assert_eq!(
        export_ron(&palette("ember")),
        fixture_text("rices/cosmic-ember-theme/cosmic/Ember.ron")
    );
    assert_eq!(
        export_ron(&palette("emberday")),
        fixture_text("rices/cosmic-emberday-theme/cosmic/EmberDay.ron")
    );
    let files = export(NativeKind::CosmicDesktop, &palette("emberday"));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "cosmic/EmberDay.ron");
}

#[test]
fn pins_are_needed() {
    let mut ember = palette("ember");
    ember.pins.clear();
    let out = export_ron(&ember);
    assert!(out.contains("gray_2: \"#2B2F37FF\""), "{out}");
    assert!(out.contains("neutral_tint: Some(\"#76787EFF\")"), "{out}");
    let out = export_ron(&palette("emberday"));
    assert!(out.contains("neutral_5: \"#868686FF\""), "{out}");
}

#[test]
fn plan_single_dark() {
    let env = TestEnv::new();
    let plan = run(&env, &[&palette("ember")]).unwrap();
    assert_eq!(plan.writes.len(), 51);
    let builder = under(&env, &plan, DARK_BUILDER);
    let theme = under(&env, &plan, DARK);
    let mode = under(&env, &plan, MODE);
    assert_eq!(builder.len(), 11);
    assert_eq!(builder.keys().map(String::as_str).collect::<Vec<_>>(), {
        let mut keys = COLOR_KEYS.to_vec();
        keys.sort();
        keys
    });
    assert_eq!(theme.len(), 39);
    assert_eq!(mode, BTreeMap::from([("is_dark".to_string(), "true")]));
    for (key, value) in [
        ("accent", "Some(\"#E8713AFF\")"),
        ("neutral_tint", "Some(\"#7E8085FF\")"),
        ("bg_color", "Some(\"#15171CFF\")"),
        ("primary_container_bg", "Some(\"#22262EFF\")"),
        ("secondary_container_bg", "Some(\"#1B1E24FF\")"),
        ("text_tint", "Some(\"#D6D9E0FF\")"),
        ("success", "Some(\"#8FB58AFF\")"),
        ("warning", "Some(\"#D9A441FF\")"),
        ("destructive", "Some(\"#D85A5AFF\")"),
        ("window_hint", "Some(\"#E8713AFF\")"),
    ] {
        assert_eq!(builder[key], value, "{key}");
    }
    assert_eq!(theme["name"], "\"Ember\"");
    assert_eq!(theme["is_dark"], "true");
    assert_eq!(theme["gaps"], "(0, 8)");
    assert!(!env.dirs.config.join("cosmic").exists());
}

#[test]
fn plan_single_light() {
    let env = TestEnv::new();
    let plan = run(&env, &[&palette("emberday")]).unwrap();
    assert_eq!(plan.writes.len(), 51);
    let builder = under(&env, &plan, LIGHT_BUILDER);
    let theme = under(&env, &plan, LIGHT);
    assert_eq!(builder.len(), 11);
    assert_eq!(theme.len(), 39);
    assert!(
        builder["palette"].starts_with("Light((\n    name: \"Ember-Day\""),
        "{}",
        builder["palette"]
    );
    assert_eq!(theme["is_dark"], "false");
    assert_eq!(
        under(&env, &plan, MODE),
        BTreeMap::from([("is_dark".to_string(), "false")])
    );
}

#[test]
fn plan_pair() {
    let env = TestEnv::new();
    let plan = run(&env, &[&palette("ember"), &palette("emberday")]).unwrap();
    assert_eq!(plan.writes.len(), 100);
    assert!(under(&env, &plan, MODE).is_empty());
    assert!(matches!(
        run(&env, &[&palette("ember"), &palette("noir")]),
        Err(TargetError::Invalid(_))
    ));
}

#[test]
fn user_settings_survive() {
    let env = TestEnv::new();
    put(&env, DARK_BUILDER, 2, "gaps", "(3, 9)");
    let plan = run(&env, &[&palette("ember")]).unwrap();
    assert!(!under(&env, &plan, DARK_BUILDER).contains_key("gaps"));
    assert_eq!(under(&env, &plan, DARK)["gaps"], "(3, 9)");
}

#[test]
fn symlinked_user_settings_survive() {
    let env = TestEnv::new();
    let dotfiles = env.tmp.path().join("dotfiles");
    fs::create_dir_all(&dotfiles).unwrap();
    fs::write(dotfiles.join("gaps"), "(3, 9)").unwrap();
    let dir = key_dir(&env, DARK_BUILDER, 2);
    fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(dotfiles.join("gaps"), dir.join("gaps")).unwrap();
    let plan = run(&env, &[&palette("ember")]).unwrap();
    assert!(!under(&env, &plan, DARK_BUILDER).contains_key("gaps"));
    assert_eq!(under(&env, &plan, DARK)["gaps"], "(3, 9)");
}

#[test]
fn bad_key_warns() {
    let env = TestEnv::new();
    put(&env, DARK_BUILDER, 2, "frosted", "NotAVariant");
    let plan = run(&env, &[&palette("ember")]).unwrap();
    assert_eq!(plan.warnings.len(), 1, "{:?}", plan.warnings);
}

#[test]
fn auto_switch_warns() {
    let env = TestEnv::new();
    let plan = run(&env, &[&palette("ember")]).unwrap();
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    put(&env, MODE, 1, "auto_switch", "true");
    let plan = run(&env, &[&palette("ember")]).unwrap();
    assert_eq!(plan.warnings.len(), 1, "{:?}", plan.warnings);
    assert!(plan.warnings[0].contains("Ember"), "{}", plan.warnings[0]);
}

#[test]
fn deterministic() {
    let env = TestEnv::new();
    let p = palette("ember");
    assert_eq!(run(&env, &[&p]).unwrap(), run(&env, &[&p]).unwrap());
}

/// Every file below `root` with its bytes.
fn listing(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, &mut out);
    out
}

#[test]
fn apply_and_undo() {
    let env = TestEnv::new();
    fs::create_dir_all(env.dirs.config.join("cosmic")).unwrap();
    put(&env, DARK_BUILDER, 2, "gaps", "(3, 9)");
    put(&env, DARK_BUILDER, 2, "accent", "None");
    let before = listing(&env.dirs.config);

    let ctx = Ctx {
        dirs: env.dirs.clone(),
        path: vec![],
    };
    let ember = palette("ember");
    let report = apply(
        &ctx,
        &Settings::default(),
        &ApplyRequest {
            palette: &ember,
            pair: None,
            only: Some(vec!["cosmic-desktop".into()]),
            live_only: false,
        },
    )
    .unwrap();
    assert!(
        matches!(
            report.get("cosmic-desktop").unwrap().status,
            TargetStatus::Applied(_)
        ),
        "{:?}",
        report.get("cosmic-desktop")
    );
    assert!(key_dir(&env, DARK, 2).join("accent").is_file());

    undo(&ctx).unwrap();
    assert_eq!(listing(&env.dirs.config), before);
}

#[test]
fn current_mode_reads_dark() {
    let env = TestEnv::new();
    put(&env, MODE, 1, "is_dark", "true");
    assert_eq!(
        irida_core::native::cosmic_desktop::current_mode(&env.dirs),
        Some(irida_core::palette::Mode::Dark)
    );
}

#[test]
fn current_mode_reads_light() {
    let env = TestEnv::new();
    put(&env, MODE, 1, "is_dark", "false");
    assert_eq!(
        irida_core::native::cosmic_desktop::current_mode(&env.dirs),
        Some(irida_core::palette::Mode::Light)
    );
}

#[test]
fn current_mode_is_none_when_missing_or_garbage() {
    let env = TestEnv::new();
    assert_eq!(
        irida_core::native::cosmic_desktop::current_mode(&env.dirs),
        None
    );
    put(&env, MODE, 1, "is_dark", "maybe");
    assert_eq!(
        irida_core::native::cosmic_desktop::current_mode(&env.dirs),
        None
    );
}
