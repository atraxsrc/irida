mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use common::{TestEnv, palette};
use cosmic_config::ConfigSet;
use hex_color::HexColor;
use irida_core::derive::derive;
use irida_core::native::cosmic_term::{CONFIG_ID, VERSION, config_dir};
use irida_core::native::{NativePlan, plan};
use irida_core::palette::Palette;
use irida_core::target::{NativeKind, TargetError};
use irida_core::term::ColorScheme;

type Schemes = BTreeMap<u64, ColorScheme>;

fn ron_text<T: serde::Serialize>(value: &T) -> String {
    ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new()).unwrap()
}

/// Write an existing COSMIC Terminal key file.
fn put(env: &TestEnv, key: &str, contents: &str) {
    let dir = config_dir(&env.dirs);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(key), contents).unwrap();
}

fn run(env: &TestEnv, palettes: &[&Palette]) -> Result<NativePlan, TargetError> {
    plan(NativeKind::CosmicTerm, palettes, &env.dirs)
}

fn keys(env: &TestEnv, plan: &NativePlan) -> Vec<String> {
    let dir = config_dir(&env.dirs);
    let mut keys: Vec<String> = plan
        .writes
        .iter()
        .map(|w| {
            assert_eq!(w.path.parent(), Some(dir.as_path()), "{}", w.path.display());
            w.path.file_name().unwrap().to_string_lossy().into_owned()
        })
        .collect();
    keys.sort();
    keys
}

fn contents<'a>(env: &TestEnv, plan: &'a NativePlan, key: &str) -> &'a str {
    let path = config_dir(&env.dirs).join(key);
    let found: Vec<_> = plan.writes.iter().filter(|w| w.path == path).collect();
    assert_eq!(found.len(), 1, "{key} written once");
    &found[0].contents
}

fn schemes(env: &TestEnv, plan: &NativePlan, key: &str) -> Schemes {
    ron::from_str(contents(env, plan, key)).unwrap()
}

fn scheme_of(p: &Palette) -> ColorScheme {
    derive(p).term.to_scheme(&p.name)
}

fn named(name: &str, fg: u8) -> ColorScheme {
    ColorScheme {
        name: name.into(),
        foreground: Some(HexColor::rgb(fg, fg, fg)),
        ..Default::default()
    }
}

#[test]
fn config_dir_is_cosmic_layout() {
    let env = TestEnv::new();
    assert_eq!(
        config_dir(&env.dirs),
        env.dirs.config.join("cosmic/com.system76.CosmicTerm/v1")
    );
    assert_eq!(CONFIG_ID, "com.system76.CosmicTerm");
    assert_eq!(VERSION, 1);
}

#[test]
fn fresh_config() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let plan = run(&env, &[&ember]).unwrap();
    assert_eq!(
        keys(&env, &plan),
        ["color_schemes_dark", "syntax_theme_dark"]
    );
    assert_eq!(
        schemes(&env, &plan, "color_schemes_dark"),
        BTreeMap::from([(0, scheme_of(&ember))])
    );
    assert_eq!(contents(&env, &plan, "syntax_theme_dark"), "\"Ember\"");
    assert!(plan.warnings.is_empty());
}

#[test]
fn keeps_other_schemes() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let gruvbox = named("Gruvbox", 0x11);
    let old: Schemes = BTreeMap::from([(0, gruvbox.clone()), (4, named("Ember", 0x22))]);
    put(&env, "color_schemes_dark", &ron_text(&old));
    let plan = run(&env, &[&ember]).unwrap();
    assert_eq!(
        schemes(&env, &plan, "color_schemes_dark"),
        BTreeMap::from([(0, gruvbox), (4, scheme_of(&ember))])
    );
}

#[test]
fn duplicate_names_replace_lowest_id() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let later = named("Ember", 0x55);
    let old: Schemes = BTreeMap::from([(2, named("Ember", 0x22)), (5, later.clone())]);
    put(&env, "color_schemes_dark", &ron_text(&old));
    let plan = run(&env, &[&ember]).unwrap();
    assert_eq!(
        schemes(&env, &plan, "color_schemes_dark"),
        BTreeMap::from([(2, scheme_of(&ember)), (5, later)])
    );
}

#[test]
fn appends_after_highest_id() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let (a, b) = (named("A", 0x11), named("B", 0x22));
    let old: Schemes = BTreeMap::from([(0, a.clone()), (7, b.clone())]);
    put(&env, "color_schemes_dark", &ron_text(&old));
    let plan = run(&env, &[&ember]).unwrap();
    assert_eq!(
        schemes(&env, &plan, "color_schemes_dark"),
        BTreeMap::from([(0, a), (7, b), (8, scheme_of(&ember))])
    );
}

/// Profiles in the layout COSMIC Terminal writes (`Profile` field order).
const PROFILES: &str = r#"{
    0: (
        name: "Other",
        command: "",
        syntax_theme_dark: "Solarized Dark",
        syntax_theme_light: "Solarized Light",
        tab_title: "",
        working_directory: "",
        drain_on_exit: false,
    ),
    1: (
        name: "Main",
        command: "/bin/zsh",
        syntax_theme_dark: "COSMIC Dark",
        syntax_theme_light: "COSMIC Light",
        tab_title: "shell",
        working_directory: "/srv/work",
        drain_on_exit: true,
    ),
}"#;

#[test]
fn pair_fills_both_modes_and_profile_once() {
    let env = TestEnv::new();
    let (ember, emberday) = (palette("ember"), palette("emberday"));
    put(&env, "default_profile", "Some(1)");
    put(&env, "profiles", PROFILES);
    let plan = run(&env, &[&ember, &emberday]).unwrap();
    assert_eq!(
        keys(&env, &plan),
        [
            "color_schemes_dark",
            "color_schemes_light",
            "profiles",
            "syntax_theme_dark",
            "syntax_theme_light",
        ]
    );
    assert_eq!(
        schemes(&env, &plan, "color_schemes_dark"),
        BTreeMap::from([(0, scheme_of(&ember))])
    );
    assert_eq!(
        schemes(&env, &plan, "color_schemes_light"),
        BTreeMap::from([(0, scheme_of(&emberday))])
    );
    assert_eq!(contents(&env, &plan, "syntax_theme_dark"), "\"Ember\"");
    assert_eq!(contents(&env, &plan, "syntax_theme_light"), "\"Ember Day\"");
    let expected = PROFILES
        .replace("\"COSMIC Dark\"", "\"Ember\"")
        .replace("\"COSMIC Light\"", "\"Ember Day\"");
    assert_eq!(contents(&env, &plan, "profiles"), expected);
}

#[test]
fn profile_untouched_without_default() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put(&env, "profiles", PROFILES);
    let plan = run(&env, &[&ember]).unwrap();
    assert_eq!(
        keys(&env, &plan),
        ["color_schemes_dark", "syntax_theme_dark"]
    );
    put(&env, "default_profile", "Some(9)");
    let plan = run(&env, &[&ember]).unwrap();
    assert_eq!(
        keys(&env, &plan),
        ["color_schemes_dark", "syntax_theme_dark"]
    );
}

#[test]
fn same_mode_pair_is_invalid() {
    let env = TestEnv::new();
    let (ember, noir) = (palette("ember"), palette("noir"));
    assert!(matches!(
        run(&env, &[&ember, &noir]),
        Err(TargetError::Invalid(_))
    ));
}

#[test]
fn app_theme_warning() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put(&env, "app_theme", "Dark");
    let plan = run(&env, &[&ember]).unwrap();
    assert_eq!(
        plan.warnings,
        ["COSMIC Terminal is set to Dark; it will not follow the desktop mode"]
    );
    put(&env, "app_theme", "System");
    let plan = run(&env, &[&ember]).unwrap();
    assert!(plan.warnings.is_empty());
}

#[test]
fn unknown_field_fails_cleanly() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let old: Schemes = BTreeMap::from([(0, named("Gruvbox", 0x11))]);
    let text = ron_text(&old).replace(
        "name: \"Gruvbox\",",
        "name: \"Gruvbox\",\n        extra: 1,",
    );
    assert!(text.contains("extra: 1"));
    put(&env, "color_schemes_dark", &text);
    match run(&env, &[&ember]) {
        Err(TargetError::Invalid(msg)) => {
            assert!(msg.contains("color_schemes_dark"), "{msg}");
            assert!(msg.ends_with("nothing written"), "{msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn matches_cosmic_config_format() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let plan = run(&env, &[&ember]).unwrap();
    let planned = contents(&env, &plan, "color_schemes_dark");

    let custom = env.tmp.path().join("cosmic-config");
    let config =
        cosmic_config::Config::with_custom_path(CONFIG_ID, VERSION, custom.clone()).unwrap();
    let value: Schemes = BTreeMap::from([(0, scheme_of(&ember))]);
    config.set("color_schemes_dark", value).unwrap();
    let written: PathBuf = custom.join("cosmic/com.system76.CosmicTerm/v1/color_schemes_dark");
    assert_eq!(std::fs::read_to_string(written).unwrap(), planned);
}
