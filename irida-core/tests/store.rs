mod common;

use std::path::PathBuf;

use common::{TestEnv, fixture, palette};
use irida_core::dirs::{Dirs, DirsError};
use irida_core::palette::Mode;
use irida_core::store::{PairLookup, Settings, Store, StoreError, VersionInfo, is_saved_slug};

#[test]
fn expand_paths() {
    let d = Dirs {
        home: "/h".into(),
        config: "/x/cfg".into(),
        data: "/x/data".into(),
        state: "/x/state".into(),
    };
    assert_eq!(
        d.expand("~/.config/btop/btop.conf"),
        PathBuf::from("/x/cfg/btop/btop.conf")
    );
    assert_eq!(d.expand("~/.zshrc"), PathBuf::from("/h/.zshrc"));
    assert_eq!(
        d.expand("$XDG_CONFIG_HOME/cava/config"),
        PathBuf::from("/x/cfg/cava/config")
    );
    assert_eq!(d.expand("/etc/x"), PathBuf::from("/etc/x"));
}

#[test]
fn save_load_list() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    store.save(&palette("ember")).unwrap();
    store.save(&palette("emberday")).unwrap();
    let slugs: Vec<String> = store.list().into_iter().map(|e| e.slug).collect();
    assert_eq!(slugs, ["ember", "emberday"]);
    assert_eq!(store.load("ember").unwrap(), palette("ember"));
}

#[test]
fn versions_count_up() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    assert_eq!(store.save_version(&palette("ember")).unwrap(), 1);
    let mut second = palette("ember");
    second.name = "Ember Two".into();
    assert_eq!(store.save_version(&second).unwrap(), 2);
    let (n, p) = store.latest_version("ember").unwrap().unwrap();
    assert_eq!(n, 2);
    assert_eq!(p, second);
    assert_eq!(store.load("ember").unwrap(), second);
}

#[test]
fn unreadable_palette_is_reported_and_untouched() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    let path = env.dirs.data.join("irida/palettes/broken/palette.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not toml [").unwrap();
    let entries = store.list();
    let broken = entries.iter().find(|e| e.slug == "broken").expect("listed");
    assert!(matches!(broken.result, Err(StoreError::Unreadable { .. })));
    assert_eq!(std::fs::read(&path).unwrap(), b"not toml [");
}

#[test]
fn resolve_forms() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    store.save(&palette("ember")).unwrap();
    let ember = palette("ember");
    assert_eq!(store.resolve("ember").unwrap(), ember);
    let file = fixture("palettes/ember.toml");
    assert_eq!(store.resolve(file.to_str().unwrap()).unwrap(), ember);
    let dir = env.dirs.data.join("irida/palettes/ember");
    assert_eq!(store.resolve(dir.to_str().unwrap()).unwrap(), ember);
    assert!(matches!(
        store.resolve("nope"),
        Err(StoreError::NotFound(_))
    ));
    let empty = env.tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(matches!(
        store.resolve(empty.to_str().unwrap()),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn settings_round_trip() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    assert_eq!(store.settings().unwrap(), Settings::default());
    let mut s = Settings::default();
    s.disabled.insert("cava".into());
    store.save_settings(&s).unwrap();
    assert_eq!(store.settings().unwrap(), s);
}

#[test]
fn save_rejects_bad_slug() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    let mut p = palette("ember");
    p.slug = "../x".into();
    assert!(matches!(store.save(&p), Err(StoreError::BadSlug(_))));
    assert!(matches!(
        store.save_version(&p),
        Err(StoreError::BadSlug(_))
    ));
    assert!(!env.dirs.data.join("irida/x").exists());
    assert!(!env.dirs.data.join("irida/palettes/../x").exists());
    assert!(!env.dirs.data.join("irida/palettes").exists());
}

#[test]
fn save_refuses_invalid() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    store.save(&palette("ember")).unwrap();
    let dir = env.dirs.data.join("irida/palettes/ember");
    let path = dir.join("palette.toml");
    let before = std::fs::read(&path).unwrap();

    let mut p = palette("ember");
    p.name = "Bad-Name".into();
    let err = store.save(&p).unwrap_err();
    assert!(matches!(err, StoreError::Invalid(_)), "{err:?}");
    assert!(err.to_string().contains("Bad-Name"), "{err}");
    assert!(matches!(
        store.save_version(&p),
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!dir.join("versions").exists());
}

#[test]
fn dirs_from_vars() {
    use std::ffi::OsString;
    let o = |s: &str| Some(OsString::from(s));
    assert_eq!(
        Dirs::from_vars(o("rel/home"), None, None, None).unwrap_err(),
        DirsError::NoHome
    );
    assert_eq!(
        Dirs::from_vars(o(""), None, None, None).unwrap_err(),
        DirsError::NoHome
    );
    assert_eq!(
        Dirs::from_vars(None, None, None, None).unwrap_err(),
        DirsError::NoHome
    );

    let d = Dirs::from_vars(o("/h"), o("rel/cfg"), o("rel/data"), o("rel/state")).unwrap();
    assert_eq!(d.config, PathBuf::from("/h/.config"));
    assert_eq!(d.data, PathBuf::from("/h/.local/share"));
    assert_eq!(d.state, PathBuf::from("/h/.local/state"));

    let d = Dirs::from_vars(o("/h"), o("/x/cfg"), o("/x/data"), o("/x/state")).unwrap();
    assert_eq!(d.home, PathBuf::from("/h"));
    assert_eq!(d.config, PathBuf::from("/x/cfg"));
    assert_eq!(d.data, PathBuf::from("/x/data"));
    assert_eq!(d.state, PathBuf::from("/x/state"));
}

fn store_with(env: &TestEnv, slugs: &[&str]) -> Store {
    let store = Store::new(env.dirs.clone());
    for slug in slugs {
        store.save(&palette(slug)).unwrap();
    }
    store
}

fn palettes_dir(env: &TestEnv) -> PathBuf {
    env.dirs.data.join("irida/palettes")
}

#[test]
fn delete_unlinks_pair() {
    let env = TestEnv::new();
    let store = store_with(&env, &["ember", "emberday"]);
    assert!(store.exists("emberday"));
    store.delete("emberday").unwrap();
    assert!(!palettes_dir(&env).join("emberday").exists());
    assert!(!store.exists("emberday"));
    assert_eq!(store.load("ember").unwrap().pair, None);
    assert!(matches!(
        store.delete("emberday"),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn delete_refuses_bad_slug() {
    let env = TestEnv::new();
    let store = store_with(&env, &["ember"]);
    let outside = env.dirs.data.join("irida/x");
    std::fs::create_dir_all(&outside).unwrap();
    assert!(!store.exists("../x"));
    assert!(matches!(store.delete("../x"), Err(StoreError::NotFound(_))));
    assert!(matches!(store.delete(""), Err(StoreError::NotFound(_))));
    assert!(outside.is_dir());
    assert!(palettes_dir(&env).join("ember/palette.toml").is_file());
}

#[test]
fn delete_unreadable_palette() {
    let env = TestEnv::new();
    let store = store_with(&env, &["ember"]);
    let dir = palettes_dir(&env).join("broken");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("palette.toml"), "not toml").unwrap();
    store.delete("broken").unwrap();
    assert!(!dir.exists());
    assert_eq!(store.load("ember").unwrap(), palette("ember"));
}

#[test]
fn duplicate_rules() {
    let env = TestEnv::new();
    let store = store_with(&env, &["ember", "emberday"]);
    let copy = store.duplicate("ember", "Ember Two").unwrap();
    assert_eq!(copy.slug, "embertwo");
    assert_eq!(copy.name, "Ember Two");
    assert_eq!(copy.pair, None);
    assert_eq!(copy.roles, palette("ember").roles);
    assert_eq!(store.load("embertwo").unwrap(), copy);
    assert_eq!(
        store.versions("embertwo").unwrap(),
        [VersionInfo {
            n: 1,
            snapshot: None
        }]
    );
    assert_eq!(store.load("ember").unwrap(), palette("ember"));
    assert_eq!(store.load("emberday").unwrap(), palette("emberday"));

    let err = store.duplicate("ember", "Ember Two").unwrap_err();
    assert!(
        matches!(err, StoreError::Exists(ref s) if s == "embertwo"),
        "{err:?}"
    );
    assert!(err.to_string().contains("embertwo"), "{err}");
    assert!(matches!(
        store.duplicate("ember", "9"),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.duplicate("ember", "Ember-Two"),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.duplicate("nope", "Fresh"),
        Err(StoreError::NotFound(_))
    ));
    assert!(!palettes_dir(&env).join("fresh").exists());
}

#[test]
fn versions_and_snapshots() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    let mut saved = Vec::new();
    for name in ["Ember", "Ember Two", "Ember Three"] {
        let mut p = palette("ember");
        p.name = name.into();
        store.save_version(&p).unwrap();
        saved.push(p);
    }
    store.pin_snapshot("ember", 2, "  Warm  ").unwrap();
    let v = |n, s: Option<&str>| VersionInfo {
        n,
        snapshot: s.map(String::from),
    };
    assert_eq!(
        store.versions("ember").unwrap(),
        [v(3, None), v(2, Some("Warm")), v(1, None)]
    );
    assert_eq!(store.load_version("ember", 2).unwrap(), saved[1]);
    assert_eq!(store.latest_version("ember").unwrap().unwrap().0, 3);

    let snapshots = env
        .dirs
        .data
        .join("irida/palettes/ember/versions/snapshots.toml");
    let text = std::fs::read_to_string(&snapshots).unwrap();
    let table: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(table["snapshots"]["2"].as_str(), Some("Warm"), "{text}");
    store.pin_snapshot("ember", 2, "Warmer").unwrap();
    assert_eq!(store.versions("ember").unwrap()[1], v(2, Some("Warmer")));
    store.unpin_snapshot("ember", 2).unwrap();
    assert_eq!(
        store.versions("ember").unwrap(),
        [v(3, None), v(2, None), v(1, None)]
    );
    assert!(!snapshots.exists());
    store.unpin_snapshot("ember", 2).unwrap();

    assert!(matches!(
        store.load_version("ember", 9),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.pin_snapshot("ember", 9, "Ghost"),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.unpin_snapshot("ember", 9),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.pin_snapshot("ember", 1, "   "),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.pin_snapshot("ember", 1, &"x".repeat(41)),
        Err(StoreError::Invalid(_))
    ));
    store.pin_snapshot("ember", 1, &"x".repeat(40)).unwrap();
    assert!(matches!(
        store.versions("../x"),
        Err(StoreError::NotFound(_))
    ));
    assert_eq!(store.versions("moss").unwrap(), []);
}

#[test]
fn unreadable_snapshots_are_never_overwritten() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    store.save_version(&palette("ember")).unwrap();
    let path = palettes_dir(&env).join("ember/versions/snapshots.toml");
    std::fs::write(&path, "not toml [").unwrap();
    assert!(matches!(
        store.versions("ember"),
        Err(StoreError::Unreadable { .. })
    ));
    assert!(matches!(
        store.pin_snapshot("ember", 1, "Warm"),
        Err(StoreError::Unreadable { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"not toml [");
}

#[test]
fn pair_lookup() {
    let env = TestEnv::new();
    let store = store_with(&env, &["ember", "emberday", "noir"]);
    let ember = palette("ember");
    assert_eq!(
        store.pair_of(&ember),
        PairLookup::Found(palette("emberday"))
    );

    let mut alone = ember.clone();
    alone.pair = None;
    assert_eq!(store.pair_of(&alone), PairLookup::None);

    let mut to_noir = ember.clone();
    to_noir.pair = Some("noir".into());
    assert_eq!(store.pair_of(&to_noir), PairLookup::SameMode("noir".into()));

    store.delete("emberday").unwrap();
    match store.pair_of(&ember) {
        PairLookup::Missing { slug, reason } => {
            assert_eq!(slug, "emberday");
            assert!(reason.contains("emberday"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn link_and_unlink() {
    let env = TestEnv::new();
    let store = store_with(&env, &["ember", "emberday", "noir", "dossier"]);
    assert!(matches!(
        store.link_pair("ember", "noir"),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.link_pair("ember", "ember"),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.link_pair("ember", "moss"),
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(store.load("ember").unwrap(), palette("ember"));
    assert_eq!(store.load("noir").unwrap(), palette("noir"));

    store.link_pair("noir", "emberday").unwrap();
    let pair = |slug: &str| store.load(slug).unwrap().pair;
    assert_eq!(pair("noir").as_deref(), Some("emberday"));
    assert_eq!(pair("emberday").as_deref(), Some("noir"));
    assert_eq!(pair("ember"), None);
    assert_eq!(pair("dossier"), None);
    assert_eq!(store.load("noir").unwrap().mode, Mode::Dark);

    store.unlink_pair("noir").unwrap();
    assert_eq!(pair("noir"), None);
    assert_eq!(pair("emberday"), None);
    store.unlink_pair("noir").unwrap();
}

#[test]
fn seed_once() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    assert_eq!(store.seed_starters().unwrap(), 4);
    let slugs: Vec<String> = store.list().into_iter().map(|e| e.slug).collect();
    assert_eq!(slugs, ["dossier", "ember", "emberday", "noir"]);
    for slug in &slugs {
        assert_eq!(store.load(slug).unwrap(), palette(slug));
        assert_eq!(
            store.versions(slug).unwrap(),
            [VersionInfo {
                n: 1,
                snapshot: None
            }]
        );
    }
    assert_eq!(store.seed_starters().unwrap(), 0);
    assert_eq!(store.versions("ember").unwrap().len(), 1);

    let env = TestEnv::new();
    let store = store_with(&env, &["noir"]);
    assert_eq!(store.seed_starters().unwrap(), 0);
    assert_eq!(store.list().len(), 1);

    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    std::fs::create_dir_all(palettes_dir(&env)).unwrap();
    assert_eq!(store.seed_starters().unwrap(), 4);
}

#[test]
fn unreadable_entry_untouched() {
    let env = TestEnv::new();
    let store = Store::new(env.dirs.clone());
    let path = palettes_dir(&env).join("broken/palette.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not toml").unwrap();
    let entries = store.list();
    assert_eq!(entries.len(), 1);
    assert!(matches!(
        entries[0].result,
        Err(StoreError::Unreadable { .. })
    ));
    assert_eq!(store.seed_starters().unwrap(), 0);
    assert_eq!(store.raw_text("broken").unwrap(), "not toml");
    assert!(matches!(
        store.raw_text("nope"),
        Err(StoreError::NotFound(_))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"not toml");
    assert_eq!(store.list().len(), 1);
}

#[test]
fn saved_slug_test() {
    let env = TestEnv::new();
    assert!(is_saved_slug("ember"));
    assert!(!is_saved_slug(env.tmp.path().to_str().unwrap()));
    let file = fixture("palettes/ember.toml");
    assert!(!is_saved_slug(file.to_str().unwrap()));
}
