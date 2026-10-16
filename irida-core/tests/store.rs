mod common;

use std::path::PathBuf;

use common::{TestEnv, fixture, palette};
use irida_core::dirs::Dirs;
use irida_core::store::{Settings, Store, StoreError};

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
    assert!(matches!(store.resolve("nope"), Err(StoreError::NotFound(_))));
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
