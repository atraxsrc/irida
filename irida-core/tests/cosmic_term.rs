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

#[test]
fn export_term() {
    use irida_core::native::export;
    for (slug, rice, name) in [
        ("ember", "Ember", "Ember"),
        ("emberday", "EmberDay", "Ember Day"),
    ] {
        let p = palette(slug);
        let files = export(NativeKind::CosmicTerm, &p);
        let get = |path: &str| {
            files
                .iter()
                .find(|f| f.path == path)
                .unwrap_or_else(|| panic!("{path} exported"))
                .contents
                .clone()
        };
        assert_eq!(files.len(), 2);
        assert_eq!(
            get(&format!("cosmic-term/{rice}-term.ron")),
            common::fixture_text(&format!("rices/{rice}-term.ron"))
        );
        assert_eq!(
            get(&format!(
                "cosmic/config/com.system76.CosmicTerm/v1/syntax_theme_{}",
                p.mode
            )),
            format!("\"{name}\"")
        );
    }
}

// Undo through apply records: only Irida's scheme and profile fields go back.

fn term_ctx(env: &TestEnv) -> irida_core::apply::Ctx {
    irida_core::apply::Ctx {
        dirs: env.dirs.clone(),
        path: vec![],
    }
}

/// Apply `p` (and `pair`) to the named targets only.
fn apply_term(
    env: &TestEnv,
    p: &Palette,
    pair: Option<&Palette>,
    only: &[&str],
) -> Result<irida_core::apply::ApplyReport, irida_core::apply::ApplyError> {
    irida_core::apply::apply(
        &term_ctx(env),
        &irida_core::store::Settings::default(),
        &irida_core::apply::ApplyRequest {
            palette: p,
            pair,
            only: Some(only.iter().map(|s| s.to_string()).collect()),
            live_only: false,
        },
    )
}

fn undo_term(env: &TestEnv) {
    let report = irida_core::apply::undo(&term_ctx(env)).expect("undo");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
}

fn key_path(env: &TestEnv, key: &str) -> PathBuf {
    config_dir(&env.dirs).join(key)
}

fn read_key(env: &TestEnv, key: &str) -> String {
    std::fs::read_to_string(key_path(env, key)).unwrap()
}

/// The fields of one profile in the `profiles` key, as RON text values.
fn profile_field(env: &TestEnv, profile: u64, field: &str) -> String {
    let map: BTreeMap<u64, ron::Value> = ron::from_str(&read_key(env, "profiles")).unwrap();
    let ron::Value::Map(fields) = &map[&profile] else {
        panic!("profile {profile} is not a struct");
    };
    let value = fields
        .iter()
        .find(|(k, _)| **k == ron::Value::String(field.into()))
        .unwrap_or_else(|| panic!("profile {profile} has no {field}"))
        .1;
    match value {
        ron::Value::String(s) => s.clone(),
        other => panic!("{field} is not a string: {other:?}"),
    }
}

fn record_toml(env: &TestEnv, id: u32) -> String {
    std::fs::read_to_string(
        env.dirs
            .state
            .join(format!("irida/applies/{id}/record.toml")),
    )
    .unwrap()
}

#[test]
fn plan_lists_undo_items() {
    use irida_core::native::NativeUndo;
    use irida_core::palette::Mode;
    let env = TestEnv::new();
    let (ember, emberday) = (palette("ember"), palette("emberday"));
    let gruvbox = named("Gruvbox", 0x11);
    let old_ember = named("Ember", 0x22);
    put(
        &env,
        "color_schemes_dark",
        &ron_text(&Schemes::from([(0, gruvbox), (3, old_ember.clone())])),
    );
    put(&env, "default_profile", "Some(1)");
    put(&env, "profiles", PROFILES);
    let plan = run(&env, &[&ember, &emberday]).unwrap();
    let undo_of = |key: &str| {
        let path = config_dir(&env.dirs).join(key);
        plan.writes
            .iter()
            .find(|w| w.path == path)
            .unwrap()
            .undo
            .clone()
    };
    assert_eq!(
        undo_of("color_schemes_dark"),
        [NativeUndo::TermScheme {
            mode: Mode::Dark,
            id: 3,
            name: "Ember".into(),
            before: Some(ron_text(&old_ember)),
        }]
    );
    assert_eq!(
        undo_of("color_schemes_light"),
        [NativeUndo::TermScheme {
            mode: Mode::Light,
            id: 0,
            name: "Ember Day".into(),
            before: None,
        }]
    );
    assert_eq!(
        undo_of("profiles"),
        [
            NativeUndo::TermProfileTheme {
                profile: 1,
                field: "syntax_theme_dark".into(),
                before: Some("\"COSMIC Dark\"".into()),
            },
            NativeUndo::TermProfileTheme {
                profile: 1,
                field: "syntax_theme_light".into(),
                before: Some("\"COSMIC Light\"".into()),
            },
        ]
    );
    assert!(undo_of("syntax_theme_dark").is_empty());
    assert!(undo_of("syntax_theme_light").is_empty());
}

#[test]
fn undo_keeps_user_scheme() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let gruvbox = named("Gruvbox", 0x11);
    put(
        &env,
        "color_schemes_dark",
        &ron_text(&Schemes::from([(0, gruvbox.clone())])),
    );
    put(&env, "syntax_theme_dark", "\"Gruvbox\"");
    put(&env, "default_profile", "Some(1)");
    put(&env, "profiles", PROFILES);

    apply_term(&env, &ember, None, &["cosmic-term"]).unwrap();
    assert_eq!(profile_field(&env, 1, "syntax_theme_dark"), "Ember");

    // The user adds a scheme of their own and renames the profile.
    let mut dark: Schemes = ron::from_str(&read_key(&env, "color_schemes_dark")).unwrap();
    assert_eq!(dark[&1], scheme_of(&ember));
    let mine = named("Mine", 0x33);
    dark.insert(2, mine.clone());
    put(&env, "color_schemes_dark", &ron_text(&dark));
    put(
        &env,
        "profiles",
        &read_key(&env, "profiles").replace("name: \"Main\"", "name: \"Work\""),
    );

    undo_term(&env);
    let dark: Schemes = ron::from_str(&read_key(&env, "color_schemes_dark")).unwrap();
    assert_eq!(dark, Schemes::from([(0, gruvbox), (2, mine)]));
    assert_eq!(profile_field(&env, 1, "name"), "Work");
    assert_eq!(profile_field(&env, 1, "syntax_theme_dark"), "COSMIC Dark");
    assert_eq!(profile_field(&env, 1, "syntax_theme_light"), "COSMIC Light");
    assert_eq!(profile_field(&env, 0, "name"), "Other");
    assert_eq!(read_key(&env, "syntax_theme_dark"), "\"Gruvbox\"");
}

#[test]
fn undo_keeps_user_scheme_at_reused_id() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let gruvbox = named("Gruvbox", 0x11);
    put(
        &env,
        "color_schemes_dark",
        &ron_text(&Schemes::from([(0, gruvbox.clone())])),
    );

    apply_term(&env, &ember, None, &["cosmic-term"]).unwrap();
    let mut dark: Schemes = ron::from_str(&read_key(&env, "color_schemes_dark")).unwrap();
    assert_eq!(dark[&1], scheme_of(&ember));

    // The user deletes Ember in Terminal settings and imports their own
    // scheme, which COSMIC Terminal puts at the id Ember had.
    let mine = named("Mine", 0x33);
    dark.insert(1, mine.clone());
    put(&env, "color_schemes_dark", &ron_text(&dark));

    undo_term(&env);
    let dark: Schemes = ron::from_str(&read_key(&env, "color_schemes_dark")).unwrap();
    assert_eq!(dark, Schemes::from([(0, gruvbox), (1, mine)]));
}

#[test]
fn undo_keeps_user_scheme_over_replaced_one() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let old_ember = named("Ember", 0x22);
    put(
        &env,
        "color_schemes_dark",
        &ron_text(&Schemes::from([(0, old_ember)])),
    );

    apply_term(&env, &ember, None, &["cosmic-term"]).unwrap();
    let mine = named("Mine", 0x33);
    put(
        &env,
        "color_schemes_dark",
        &ron_text(&Schemes::from([(0, mine.clone())])),
    );

    undo_term(&env);
    let dark: Schemes = ron::from_str(&read_key(&env, "color_schemes_dark")).unwrap();
    assert_eq!(dark, Schemes::from([(0, mine)]));
}

#[test]
fn undo_restores_replaced_scheme_the_user_deleted() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let gruvbox = named("Gruvbox", 0x11);
    let old_ember = named("Ember", 0x22);
    put(
        &env,
        "color_schemes_dark",
        &ron_text(&Schemes::from([
            (0, gruvbox.clone()),
            (1, old_ember.clone()),
        ])),
    );

    apply_term(&env, &ember, None, &["cosmic-term"]).unwrap();
    // The user deletes Irida's Ember; undo still puts the old one back.
    put(
        &env,
        "color_schemes_dark",
        &ron_text(&Schemes::from([(0, gruvbox.clone())])),
    );

    undo_term(&env);
    let dark: Schemes = ron::from_str(&read_key(&env, "color_schemes_dark")).unwrap();
    assert_eq!(dark, Schemes::from([(0, gruvbox), (1, old_ember)]));
}

#[test]
fn undo_pair_restores_both_fields() {
    let env = TestEnv::new();
    let (ember, emberday) = (palette("ember"), palette("emberday"));
    put(&env, "default_profile", "Some(1)");
    put(&env, "profiles", PROFILES);

    apply_term(&env, &ember, Some(&emberday), &["cosmic-term"]).unwrap();
    assert_eq!(profile_field(&env, 1, "syntax_theme_dark"), "Ember");
    assert_eq!(profile_field(&env, 1, "syntax_theme_light"), "Ember Day");

    undo_term(&env);
    assert_eq!(profile_field(&env, 1, "syntax_theme_dark"), "COSMIC Dark");
    assert_eq!(profile_field(&env, 1, "syntax_theme_light"), "COSMIC Light");
    // Nothing else changed, so the file is back as it was.
    assert_eq!(read_key(&env, "profiles"), PROFILES);
    for key in [
        "color_schemes_dark",
        "color_schemes_light",
        "syntax_theme_dark",
        "syntax_theme_light",
    ] {
        assert!(!key_path(&env, key).exists(), "{key}");
    }
}

#[test]
fn undo_removes_created_keys() {
    let env = TestEnv::new();
    let (ember, emberday) = (palette("ember"), palette("emberday"));
    let dir = config_dir(&env.dirs);
    std::fs::create_dir_all(&dir).unwrap();

    apply_term(&env, &ember, Some(&emberday), &["cosmic-term"]).unwrap();
    assert!(key_path(&env, "color_schemes_dark").exists());
    assert!(key_path(&env, "color_schemes_light").exists());

    undo_term(&env);
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn rollback_is_byte_exact() {
    let env = TestEnv::new();
    let (ember, emberday) = (palette("ember"), palette("emberday"));
    // Compact RON, which Irida's writer would never produce: a rewrite shows.
    let dark_text = ron::to_string(&Schemes::from([(0, named("Gruvbox", 0x11))])).unwrap();
    put(&env, "color_schemes_dark", &dark_text);
    put(&env, "default_profile", "Some(1)");
    let profiles_text = PROFILES.replace("    ", "\t");
    put(&env, "profiles", &profiles_text);
    // Target y writes after cosmic-term; z's write then fails, its folder a file.
    for (name, path) in [
        ("y", "~/.config/y/y.conf"),
        ("z", "~/.config/blocker/b.conf"),
    ] {
        let dir = env.dirs.config.join("irida/targets").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("target.toml"),
            format!(
                "name = \"{name}\"\ndetect = {{}}\n\n[[file]]\npath = \"{path}\"\nwrite = \"file\"\ntemplate = \"t.tmpl\"\n"
            ),
        )
        .unwrap();
        std::fs::write(dir.join("t.tmpl"), "x\n").unwrap();
    }
    let y_conf = env.write(".config/y/y.conf", "old");
    env.write(".config/blocker", "not a folder");

    match apply_term(&env, &ember, Some(&emberday), &["cosmic-term", "y", "z"]) {
        Err(irida_core::apply::ApplyError::Write {
            rolled_back: true, ..
        }) => {}
        other => panic!("expected a rolled back write, got {other:?}"),
    }
    assert_eq!(read_key(&env, "color_schemes_dark"), dark_text);
    assert_eq!(read_key(&env, "profiles"), profiles_text);
    assert_eq!(std::fs::read_to_string(&y_conf).unwrap(), "old");
    for key in [
        "color_schemes_light",
        "syntax_theme_dark",
        "syntax_theme_light",
    ] {
        assert!(!key_path(&env, key).exists(), "{key}");
    }
    assert!(!env.dirs.state.join("irida/applies/1").exists());
}

#[test]
fn old_record_parses() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let dark_old = ron_text(&Schemes::from([(0, named("Gruvbox", 0x11))]));
    put(&env, "color_schemes_dark", "irida wrote this");
    put(&env, "syntax_theme_dark", "\"Ember\"");
    let conf = env.write(".config/c/c.conf", "a = 1\ntheme = ember\n");

    // A record exactly as plan 2 wrote it: one kind per whole file, block or key.
    let rec = env.dirs.state.join("irida/applies/1");
    std::fs::create_dir_all(rec.join("files")).unwrap();
    std::fs::write(rec.join("files/0"), &dark_old).unwrap();
    let text = format!(
        "id = 1\npalette = \"ember\"\n\n\
         [[entries]]\nkind = \"file\"\npath = {dark:?}\nblob = \"files/0\"\n\n\
         [[entries]]\nkind = \"file\"\npath = {theme:?}\n\n\
         [[entries]]\nkind = \"key\"\npath = {conf:?}\nkey = \"theme\"\nbefore = \"theme = old\"\nappended_newline = false\n",
        dark = key_path(&env, "color_schemes_dark").to_str().unwrap(),
        theme = key_path(&env, "syntax_theme_dark").to_str().unwrap(),
        conf = conf.to_str().unwrap(),
    );
    std::fs::write(rec.join("record.toml"), text).unwrap();

    undo_term(&env);
    assert_eq!(read_key(&env, "color_schemes_dark"), dark_old);
    assert!(!key_path(&env, "syntax_theme_dark").exists());
    assert_eq!(
        std::fs::read_to_string(&conf).unwrap(),
        "a = 1\ntheme = old\n"
    );
    assert!(!rec.exists());

    // A new record names the scheme entry by its own kind.
    let report = apply_term(&env, &ember, None, &["cosmic-term"]).unwrap();
    let new = record_toml(&env, report.record_id);
    assert!(new.contains("kind = \"term_scheme\""), "{new}");
    assert!(new.contains("kind = \"file\""), "{new}");
}

#[test]
fn undo_of_unparsable_key_uses_recorded_copy() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let dark_text = ron::to_string(&Schemes::from([(0, named("Gruvbox", 0x11))])).unwrap();
    put(&env, "color_schemes_dark", &dark_text);

    apply_term(&env, &ember, None, &["cosmic-term"]).unwrap();
    put(&env, "color_schemes_dark", "not ron (");

    undo_term(&env);
    assert_eq!(read_key(&env, "color_schemes_dark"), dark_text);
}
