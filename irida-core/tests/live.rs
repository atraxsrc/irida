mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use common::{TestEnv, palette};
use hex_color::HexColor;
use irida_core::apply::{self, ApplyError, ApplyReport, ApplyRequest, Ctx, UndoError};
use irida_core::live::{self, LiveSession};
use irida_core::native::cosmic_term::config_dir;
use irida_core::palette::{Mode, Palette};
use irida_core::record;
use irida_core::store::Settings;
use irida_core::term::ColorScheme;

type Schemes = BTreeMap<u64, ColorScheme>;

fn ctx(env: &TestEnv) -> Ctx {
    Ctx {
        dirs: env.dirs.clone(),
        path: vec![],
    }
}

fn marker(env: &TestEnv) -> PathBuf {
    env.dirs.state.join("irida/live.toml")
}

fn put_marker(env: &TestEnv, text: &str) {
    fs::create_dir_all(env.dirs.state.join("irida")).unwrap();
    fs::write(marker(env), text).unwrap();
}

fn req(p: &Palette) -> ApplyRequest<'_> {
    ApplyRequest {
        palette: p,
        pair: None,
        only: None,
        live_only: false,
    }
}

fn session_apply(s: &mut LiveSession, env: &TestEnv, p: &Palette) -> ApplyReport {
    s.apply(&ctx(env), &Settings::default(), &req(p))
        .unwrap_or_else(|e| panic!("session apply: {e}"))
}

/// A normal apply of the user target `a` only.
fn apply_a(env: &TestEnv, p: &Palette) -> ApplyReport {
    let req = ApplyRequest {
        only: Some(vec!["a".to_string()]),
        ..req(p)
    };
    apply::apply(&ctx(env), &Settings::default(), &req).unwrap()
}

fn applies_dir(env: &TestEnv) -> PathBuf {
    env.dirs.state.join("irida/applies")
}

/// Record folder numbers, ascending; empty when there is no applies folder.
fn record_ids(env: &TestEnv) -> Vec<u32> {
    let mut ids: Vec<u32> = match fs::read_dir(applies_dir(env)) {
        Ok(rd) => rd
            .map(|e| {
                e.unwrap()
                    .file_name()
                    .into_string()
                    .unwrap()
                    .parse()
                    .unwrap()
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    ids.sort();
    ids
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

/// cava's config folder, which makes cava (a live target) detected.
fn put_cava(env: &TestEnv) -> PathBuf {
    env.write(".config/cava/config", "x\n")
}

/// Write a user target folder with its templates.
fn put_target(env: &TestEnv, name: &str, toml: &str, templates: &[(&str, &str)]) {
    let dir = env.dirs.config.join("irida/targets").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("target.toml"), toml).unwrap();
    for (file, text) in templates {
        fs::write(dir.join(file), text).unwrap();
    }
}

/// A user target with one `file` write of `t.tmpl` to `path`.
fn file_target(name: &str, path: &str, live: bool) -> String {
    format!(
        "name = \"{name}\"\ndetect = {{}}\nlive = {live}\n\n[[file]]\npath = \"{path}\"\nwrite = \"file\"\ntemplate = \"t.tmpl\"\n"
    )
}

/// Plan 1's user target `a`, which does not reload live.
fn put_a(env: &TestEnv) {
    put_target(
        env,
        "a",
        &file_target("a", "~/.config/a/a.conf", false),
        &[("t.tmpl", "accent = {{ accent }}\n")],
    );
}

fn ron_text<T: serde::Serialize>(value: &T) -> String {
    ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new()).unwrap()
}

/// Profiles in the layout COSMIC Terminal writes (`Profile` field order).
const PROFILES: &str = r#"{
    1: (
        name: "Main",
        command: "",
        syntax_theme_dark: "COSMIC Dark",
        syntax_theme_light: "COSMIC Light",
        tab_title: "",
        working_directory: "",
        drain_on_exit: false,
    ),
}"#;

const TERM_KEYS: [&str; 5] = [
    "color_schemes_dark",
    "color_schemes_light",
    "profiles",
    "syntax_theme_dark",
    "syntax_theme_light",
];

/// A COSMIC Terminal config with a default profile and one user scheme;
/// `color_schemes_light` does not exist.
fn put_term(env: &TestEnv) {
    let dir = config_dir(&env.dirs);
    fs::create_dir_all(&dir).unwrap();
    let gruvbox = ColorScheme {
        name: "Gruvbox".into(),
        foreground: Some(HexColor::rgb(0x11, 0x11, 0x11)),
        ..Default::default()
    };
    for (key, text) in [
        (
            "color_schemes_dark",
            ron_text(&Schemes::from([(0, gruvbox)])),
        ),
        ("default_profile", "Some(1)".to_string()),
        ("profiles", PROFILES.to_string()),
        ("syntax_theme_dark", "\"COSMIC Dark\"".to_string()),
        ("syntax_theme_light", "\"COSMIC Light\"".to_string()),
    ] {
        fs::write(dir.join(key), text).unwrap();
    }
}

/// The bytes of every COSMIC Terminal key the tests compare; `None` when absent.
fn term_bytes(env: &TestEnv) -> Vec<(&'static str, Option<Vec<u8>>)> {
    let dir = config_dir(&env.dirs);
    TERM_KEYS
        .iter()
        .map(|k| (*k, fs::read(dir.join(k)).ok()))
        .collect()
}

#[test]
fn session_captures_once() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    assert!(marker(&env).is_file());
    assert_eq!(s.palette(), "ember");
    assert_eq!(s.record_id(), None);
    assert!(record_ids(&env).is_empty(), "no record before an apply");

    for slug in ["ember", "noir", "emberday"] {
        let report = session_apply(&mut s, &env, &palette(slug));
        assert!(report.get("cava").is_some(), "{report:?}");
    }
    let id = s.record_id().expect("record after an apply");
    assert_eq!(record_ids(&env), vec![id]);
    assert_ne!(read(&cava), "x\n");
    let rec = record::load(&env.dirs, id).unwrap();
    assert_eq!(rec.entries.len(), 1, "{rec:?}");
    let blobs = fs::read_dir(applies_dir(&env).join(id.to_string()).join("files"))
        .unwrap()
        .count();
    assert_eq!(blobs, 1);

    let report = s.revert(&ctx(&env)).unwrap().expect("a report");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&cava), "x\n");
    assert!(!marker(&env).exists());
    assert!(record_ids(&env).is_empty());
}

#[test]
fn keep_leaves_one_undo() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &palette("ember"));
    session_apply(&mut s, &env, &palette("noir"));
    s.keep(&ctx(&env)).unwrap();
    assert!(!marker(&env).exists());
    assert_eq!(record_ids(&env).len(), 1);

    let report = apply::undo(&ctx(&env)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&cava), "x\n");
    assert!(matches!(
        apply::undo(&ctx(&env)),
        Err(UndoError::NothingToUndo)
    ));
}

/// Start a session, apply Ember (dark), then Ember switched to light.
fn mode_switch_session(env: &TestEnv) -> LiveSession {
    let ember = palette("ember");
    let mut light = ember.clone();
    light.mode = Mode::Light;
    let mut s = LiveSession::start(&ctx(env), "ember").unwrap();
    session_apply(&mut s, env, &ember);
    session_apply(&mut s, env, &light);
    s
}

#[test]
fn session_mode_switch_reverts() {
    // Revert.
    let env = TestEnv::new();
    put_term(&env);
    let before = term_bytes(&env);
    let s = mode_switch_session(&env);
    assert_ne!(term_bytes(&env), before);
    let report = s.revert(&ctx(&env)).unwrap().expect("a report");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(term_bytes(&env), before);

    // Keep, then core undo.
    let env = TestEnv::new();
    put_term(&env);
    let before = term_bytes(&env);
    let s = mode_switch_session(&env);
    s.keep(&ctx(&env)).unwrap();
    let report = apply::undo(&ctx(&env)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(term_bytes(&env), before);
}

#[test]
fn crash_then_pending() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &palette("ember"));
    let id = s.record_id();
    drop(s);

    let p = live::pending(&env.dirs).expect("an open session");
    assert_eq!(p.record_id(), id);
    assert_eq!(p.palette(), "ember");
    let report = p.revert(&ctx(&env)).unwrap().expect("a report");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&cava), "x\n");
    assert!(!marker(&env).exists());
    assert!(live::pending(&env.dirs).is_none());
}

#[test]
fn stale_marker_cleared() {
    let env = TestEnv::new();
    for text in [
        "palette = \"ember\"\nrecord = 99\n",
        "palette = \"ember\"\n",
        "not a marker",
    ] {
        put_marker(&env, text);
        assert!(live::pending(&env.dirs).is_none(), "{text}");
        assert!(!marker(&env).exists(), "{text}");
    }
}

#[test]
fn odd_marker_still_offers_revert() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &palette("ember"));
    let id = s.record_id().unwrap();
    // A marker from a newer Irida, with a key this one does not know, and
    // one that lost its palette name.
    for text in [
        format!("palette = \"ember\"\nrecord = {id}\nstarted = 5\n"),
        format!("record = {id}\n"),
    ] {
        put_marker(&env, &text);
        let p = live::pending(&env.dirs).expect(&text);
        assert_eq!(p.record_id(), Some(id), "{text}");
        assert_eq!(p.palette(), "ember", "{text}");
        assert!(marker(&env).exists(), "{text}");
    }
    live::pending(&env.dirs)
        .unwrap()
        .revert(&ctx(&env))
        .unwrap();
    assert_eq!(read(&cava), "x\n");
}

#[test]
fn refreshed_names_the_markers_record() {
    let env = TestEnv::new();
    put_cava(&env);
    let s = LiveSession::start(&ctx(&env), "ember").unwrap();
    let mut applied = s.clone();
    session_apply(&mut applied, &env, &palette("ember"));
    let id = applied.record_id().unwrap();
    // `s` never saw its first apply finish.
    assert_eq!(s.record_id(), None);
    assert_eq!(s.clone().refreshed(&env.dirs).record_id(), Some(id));

    // A record the apply made but stopped before writing to is not taken.
    put_marker(&env, &format!("palette = \"ember\"\nrecord = {}\n", id + 1));
    fs::create_dir_all(applies_dir(&env).join((id + 1).to_string())).unwrap();
    assert_eq!(s.refreshed(&env.dirs).record_id(), None);
}

#[test]
fn unfinished_first_apply_is_dropped() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    let ember = palette("ember");
    // A first apply that made its record, then stopped before its first
    // write: the marker names a folder without record.toml.
    let unfinished = |s: &LiveSession| {
        let id = record_ids(&env).last().copied().unwrap_or(0) + 1;
        fs::create_dir_all(applies_dir(&env).join(id.to_string())).unwrap();
        put_marker(&env, &format!("palette = \"ember\"\nrecord = {id}\n"));
        (id, s.clone().refreshed(&env.dirs))
    };

    // The next apply starts again and the folder goes.
    let s = LiveSession::start(&ctx(&env), "ember").unwrap();
    let (id, mut s) = unfinished(&s);
    assert_eq!(s.record_id(), None);
    session_apply(&mut s, &env, &ember);
    assert!(!applies_dir(&env).join(id.to_string()).exists());
    assert_eq!(record_ids(&env), vec![s.record_id().unwrap()]);
    s.revert(&ctx(&env)).unwrap();
    assert_eq!(read(&cava), "x\n");

    // Revert and Keep of a session that never wrote drop it too.
    for keep in [false, true] {
        let s = LiveSession::start(&ctx(&env), "ember").unwrap();
        let (id, s) = unfinished(&s);
        if keep {
            s.keep(&ctx(&env)).unwrap();
        } else {
            assert!(s.revert(&ctx(&env)).unwrap().is_none());
        }
        assert!(
            !applies_dir(&env).join(id.to_string()).exists(),
            "keep {keep}"
        );
        assert!(!marker(&env).exists());
    }
    assert!(record_ids(&env).is_empty());
}

#[test]
fn prune_spares_open_session() {
    let env = TestEnv::new();
    put_cava(&env);
    put_a(&env);
    let ember = palette("ember");
    let mut last = 0;
    for _ in 0..3 {
        last = apply_a(&env, &ember).record_id;
    }
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &ember);
    let id = s.record_id().unwrap();

    // A normal apply cannot run during the session, so prune directly.
    record::prune(&env.dirs, 1).unwrap();
    assert_eq!(record_ids(&env), vec![last, id]);
    assert_eq!(record::latest(&env.dirs).unwrap().unwrap().id, last);
}

#[test]
fn apply_refuses_during_session() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    put_a(&env);
    let a = env.dirs.home.join(".config/a/a.conf");
    let ember = palette("ember");
    // More older records than Keep's prune keeps: the session's must survive.
    for _ in 0..12 {
        apply_a(&env, &ember);
    }
    let before = record_ids(&env);
    let a_before = read(&a);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &ember);
    let id = s.record_id().unwrap();
    let applied = read(&cava);

    let req = ApplyRequest {
        only: Some(vec!["a".to_string()]),
        ..req(&ember)
    };
    match apply::apply(&ctx(&env), &Settings::default(), &req) {
        Err(e @ ApplyError::LiveSessionOpen) => {
            assert!(e.to_string().contains("live session"), "{e}");
        }
        other => panic!("expected LiveSessionOpen, got {other:?}"),
    }
    assert_eq!(read(&cava), applied);
    assert_eq!(read(&a), a_before, "nothing written");
    let mut with_session = before.clone();
    with_session.push(id);
    assert_eq!(record_ids(&env), with_session, "no record made");

    // Keep, then one undo still puts back what was there before the session.
    s.keep(&ctx(&env)).unwrap();
    apply_a(&env, &ember);
    apply::undo(&ctx(&env)).unwrap();
    apply::undo(&ctx(&env)).unwrap();
    assert_eq!(read(&cava), "x\n");
}

#[test]
fn stale_marker_never_names_a_new_record() {
    let env = TestEnv::new();
    put_cava(&env);
    put_a(&env);
    let ember = palette("ember");
    let first = apply_a(&env, &ember).record_id;
    // A revert that died between deleting its record and its marker.
    put_marker(
        &env,
        &format!("palette = \"ember\"\nrecord = {}\n", first + 1),
    );

    let next = apply_a(&env, &ember).record_id;
    assert_eq!(next, first + 2, "the marker's id is skipped");
    apply_a(&env, &ember);
    apply::undo(&ctx(&env)).unwrap();
    apply::undo(&ctx(&env)).unwrap();
}

#[test]
fn latest_skips_open_session() {
    let env = TestEnv::new();
    put_cava(&env);
    put_a(&env);
    let ember = palette("ember");
    let normal = apply_a(&env, &ember).record_id;
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &ember);
    assert!(s.record_id().unwrap() > normal);
    assert_eq!(record::latest(&env.dirs).unwrap().unwrap().id, normal);
}

#[test]
fn undo_refuses_during_session() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &palette("ember"));
    let applied = read(&cava);

    match apply::undo(&ctx(&env)) {
        Err(e @ UndoError::LiveSessionOpen) => {
            assert!(e.to_string().contains("live session"), "{e}");
        }
        other => panic!("expected LiveSessionOpen, got {other:?}"),
    }
    assert_eq!(read(&cava), applied);
    assert!(s.record_id().is_some());
}

#[test]
fn non_live_targets_untouched() {
    let env = TestEnv::new();
    put_cava(&env);
    put_a(&env);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    let report = session_apply(&mut s, &env, &palette("ember"));
    assert!(report.get("a").is_none(), "{report:?}");
    assert!(report.get("cava").is_some(), "{report:?}");
    assert!(!env.dirs.home.join(".config/a").exists());
}

#[test]
fn failed_first_apply_leaves_nothing() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    put_target(
        &env,
        "zz",
        &file_target("zz", "~/.config/zz/zz.conf", true),
        &[("t.tmpl", "{{ nope }}\n")],
    );
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    match s.apply(&ctx(&env), &Settings::default(), &req(&palette("ember"))) {
        Err(ApplyError::Render { target, .. }) => assert_eq!(target, "zz"),
        other => panic!("expected Render, got {other:?}"),
    }
    assert!(record_ids(&env).is_empty());
    assert_eq!(read(&cava), "x\n");
    assert_eq!(s.record_id(), None);
    assert!(live::pending(&env.dirs).is_none());
    assert!(s.revert(&ctx(&env)).unwrap().is_none());
    assert!(!marker(&env).exists());
}

#[test]
fn failed_later_apply_restores_only_its_writes() {
    let env = TestEnv::new();
    let cava = put_cava(&env);
    // `zz` sorts after cava, so cava is written before zz fails.
    put_target(
        &env,
        "zz",
        &file_target("zz", "~/.config/blocker/b.conf", true),
        &[("t.tmpl", "x\n")],
    );
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &palette("ember"));
    let id = s.record_id().unwrap();
    let after_first = read(&cava);
    let entries = record::load(&env.dirs, id).unwrap().entries;
    fs::remove_dir_all(env.dirs.home.join(".config/blocker")).unwrap();
    env.write(".config/blocker", "not a folder");

    match s.apply(&ctx(&env), &Settings::default(), &req(&palette("noir"))) {
        Err(ApplyError::Write {
            rolled_back: true, ..
        }) => {}
        other => panic!("expected rolled back Write, got {other:?}"),
    }
    assert_eq!(read(&cava), after_first, "this apply's writes are undone");
    assert_eq!(record::load(&env.dirs, id).unwrap().entries, entries);
    assert_eq!(s.record_id(), Some(id));
    assert!(live::pending(&env.dirs).is_some(), "the session stays open");

    let report = s.revert(&ctx(&env)).unwrap().expect("a report");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&cava), "x\n");
    assert!(record_ids(&env).is_empty());
}

#[test]
fn start_refuses_while_a_session_is_open() {
    let env = TestEnv::new();
    put_cava(&env);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &palette("ember"));
    let err = LiveSession::start(&ctx(&env), "noir").expect_err("refused");
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(live::pending(&env.dirs).unwrap().record_id(), s.record_id());
}

#[test]
fn session_open_is_false_without_a_marker() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert!(!live::session_open(&irida_core::dirs::Dirs::under(
        tmp.path()
    )));
}

#[test]
fn keep_then_undo_restores_a_key_line_rewritten_in_the_session() {
    let env = TestEnv::new();
    put_cava(&env);
    put_target(
        &env,
        "c",
        "name = \"c\"\ndetect = {}\nlive = true\n\n[[file]]\npath = \"~/.config/c/c.conf\"\nwrite = \"key\"\nkey = \"theme\"\ntemplate = \"t.tmpl\"\n",
        &[("t.tmpl", "{{ slug }}\n")],
    );
    let c_conf = env.write(".config/c/c.conf", "a = 1\ntheme = old\n");
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &palette("ember"));
    session_apply(&mut s, &env, &palette("noir"));
    assert_eq!(read(&c_conf), "a = 1\ntheme = noir\n");
    s.keep(&ctx(&env)).unwrap();

    let report = apply::undo(&ctx(&env)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&c_conf), "a = 1\ntheme = old\n");
}

#[test]
fn keep_then_undo_with_two_keys_in_one_file() {
    let env = TestEnv::new();
    put_cava(&env);
    put_target(
        &env,
        "y",
        "name = \"y\"\ndetect = {}\nlive = true\n\n[[file]]\npath = \"~/.config/y/theme.toml\"\nwrite = \"key\"\nkey = \"dark\"\nsection = \"flavor\"\ntemplate = \"t.tmpl\"\n\n[[file]]\npath = \"~/.config/y/theme.toml\"\nwrite = \"key\"\nkey = \"light\"\nsection = \"flavor\"\ntemplate = \"t.tmpl\"\n",
        &[("t.tmpl", "{{ slug }}\n")],
    );
    let start = "[flavor]\ndark = old\n";
    let conf = env.write(".config/y/theme.toml", start);
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    session_apply(&mut s, &env, &palette("ember"));
    session_apply(&mut s, &env, &palette("noir"));
    assert_eq!(read(&conf), "[flavor]\ndark = noir\nlight = noir\n");
    s.keep(&ctx(&env)).unwrap();

    let report = apply::undo(&ctx(&env)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&conf), start);
}

/// A live target with a key and a block in one file, where the block first
/// lands on the session's second apply (the first finds the markers broken).
/// Kept, then undone, both come out.
#[test]
fn later_session_edit_to_a_held_file_is_undone() {
    let env = TestEnv::new();
    put_target(
        &env,
        "m",
        "name = \"m\"\ndetect = {}\nlive = true\n\n\
         [[file]]\npath = \"~/.config/m/m.conf\"\nwrite = \"key\"\nkey = \"k\"\ntemplate = \"k.tmpl\"\n\n\
         [[file]]\npath = \"~/.config/m/m.conf\"\nwrite = \"block\"\ncomment = \"#\"\ntemplate = \"b.tmpl\"\n",
        &[
            ("k.tmpl", "{{ slug }}\n"),
            ("b.tmpl", "accent {{ accent }}\n"),
        ],
    );
    let conf = env.write(".config/m/m.conf", "a\n# >>> irida >>>\n");
    let mut s = LiveSession::start(&ctx(&env), "ember").unwrap();
    let report = session_apply(&mut s, &env, &palette("ember"));
    let w = &report.get("m").unwrap().warnings;
    assert!(w.iter().any(|m| m.contains("markers are broken")), "{w:?}");
    assert!(!read(&conf).contains("accent"));

    // The user mends the stray marker; the next apply adds the block.
    fs::write(&conf, read(&conf).replace("# >>> irida >>>\n", "")).unwrap();
    session_apply(&mut s, &env, &palette("noir"));
    assert!(read(&conf).contains("accent"), "{}", read(&conf));
    s.keep(&ctx(&env)).unwrap();

    let report = apply::undo(&ctx(&env)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&conf), "a\n");
}
