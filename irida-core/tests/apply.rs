mod common;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use common::{TestEnv, palette};
use hex_color::HexColor;
use irida_core::apply::{
    ApplyError, ApplyReport, ApplyRequest, Ctx, SkipReason, TargetStatus, UndoError, apply, undo,
};
use irida_core::derive::derive;
use irida_core::native::cosmic_term::config_dir;
use irida_core::palette::Palette;
use irida_core::reload::ReloadStatus;
use irida_core::store::Settings;
use irida_core::template::Renderer;
use irida_core::term::ColorScheme;

type Schemes = BTreeMap<u64, ColorScheme>;

const A_TOML: &str = r#"name = "a"
detect = {}

[[file]]
path = "~/.config/a/a.conf"
write = "file"
template = "a.tmpl"
"#;
const A_TMPL: &str = "accent = {{ accent }}\nbackground = {{ background }}\n";

const B_TOML: &str = r##"name = "b"
detect = {}

[[file]]
path = "~/.zshrc"
write = "block"
template = "b.tmpl"
comment = "#"
"##;
const B_TMPL: &str = "export IRIDA_ACCENT={{ accent }}\n";

const C_TOML: &str = r#"name = "c"
detect = {}

[[file]]
path = "~/.config/c/c.conf"
write = "key"
template = "c.tmpl"
key = "theme"
"#;
const C_TMPL: &str = "{{ slug }}\r\n";

/// Write a user target folder with its templates.
fn put_target(env: &TestEnv, name: &str, toml: &str, templates: &[(&str, &str)]) {
    let dir = env.dirs.config.join("irida/targets").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("target.toml"), toml).unwrap();
    for (file, text) in templates {
        fs::write(dir.join(file), text).unwrap();
    }
}

fn put_a(env: &TestEnv) {
    put_target(env, "a", A_TOML, &[("a.tmpl", A_TMPL)]);
}

fn put_b(env: &TestEnv) {
    put_target(env, "b", B_TOML, &[("b.tmpl", B_TMPL)]);
}

fn put_c(env: &TestEnv) {
    put_target(env, "c", C_TOML, &[("c.tmpl", C_TMPL)]);
}

/// A user target with one `file` write of `template` to `path`.
fn file_target(name: &str, path: &str) -> String {
    format!(
        "name = \"{name}\"\ndetect = {{}}\n\n[[file]]\npath = \"{path}\"\nwrite = \"file\"\ntemplate = \"t.tmpl\"\n"
    )
}

fn ctx(env: &TestEnv) -> Ctx {
    Ctx { dirs: env.dirs.clone(), path: vec![] }
}

fn run(env: &TestEnv, p: &Palette) -> Result<ApplyReport, ApplyError> {
    run_with(env, &Settings::default(), p, None, None)
}

fn run_with(
    env: &TestEnv,
    settings: &Settings,
    p: &Palette,
    pair: Option<&Palette>,
    only: Option<&[&str]>,
) -> Result<ApplyReport, ApplyError> {
    let req = ApplyRequest {
        palette: p,
        pair,
        only: only.map(|o| o.iter().map(|s| s.to_string()).collect()),
    };
    apply(&ctx(env), settings, &req)
}

fn status<'a>(report: &'a ApplyReport, name: &str) -> &'a TargetStatus {
    &report.get(name).unwrap_or_else(|| panic!("{name} in report")).status
}

fn applied(report: &ApplyReport, name: &str) {
    assert_eq!(
        status(report, name),
        &TargetStatus::Applied(ReloadStatus::NotNeeded),
        "{name}"
    );
}

fn render(p: &Palette, source: &str) -> String {
    Renderer::new(p).render("t", source).unwrap()
}

fn applies_dir(env: &TestEnv) -> PathBuf {
    env.dirs.state.join("irida/applies")
}

fn record_ids(env: &TestEnv) -> Vec<u32> {
    let mut ids: Vec<u32> = match fs::read_dir(applies_dir(env)) {
        Ok(rd) => rd
            .map(|e| e.unwrap().file_name().into_string().unwrap().parse().unwrap())
            .collect(),
        Err(_) => Vec::new(),
    };
    ids.sort();
    ids
}

fn home(env: &TestEnv, rel: &str) -> PathBuf {
    env.dirs.home.join(rel)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn applies_file_block_and_key() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    put_b(&env);
    put_c(&env);
    env.write(".zshrc", "export X=1\n");
    env.write(".config/c/c.conf", "a = 1\ntheme = old\n");

    let report = run(&env, &ember).unwrap();
    assert_eq!(report.record_id, 1);
    for name in ["a", "b", "c"] {
        applied(&report, name);
    }
    assert_eq!(
        status(&report, "cosmic-term"),
        &TargetStatus::Skipped(SkipReason::NotInstalled)
    );

    assert_eq!(read(&home(&env, ".config/a/a.conf")), render(&ember, A_TMPL));
    assert_eq!(
        read(&home(&env, ".zshrc")),
        format!(
            "export X=1\n# >>> irida >>>\n{}# <<< irida <<<\n",
            render(&ember, B_TMPL)
        )
    );
    assert_eq!(read(&home(&env, ".config/c/c.conf")), "a = 1\ntheme = ember\n");
    assert_eq!(record_ids(&env), [1]);
}

#[test]
fn undo_restores_and_keeps_user_edits() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    put_b(&env);
    put_c(&env);
    env.write(".zshrc", "export X=1\n");
    env.write(".config/c/c.conf", "a = 1\ntheme = old\n");
    run(&env, &ember).unwrap();

    let zshrc = home(&env, ".zshrc");
    let c_conf = home(&env, ".config/c/c.conf");
    fs::write(&zshrc, read(&zshrc) + "alias x=y\n").unwrap();
    fs::write(&c_conf, read(&c_conf) + "mine = 1\n").unwrap();

    let report = undo(&ctx(&env)).unwrap();
    assert_eq!(report.record_id, 1);
    assert_eq!(report.palette, "ember");
    assert!(report.failures.is_empty(), "{:?}", report.failures);

    assert_eq!(read(&zshrc), "export X=1\nalias x=y\n");
    assert!(!home(&env, ".config/a/a.conf").exists());
    assert_eq!(read(&c_conf), "a = 1\ntheme = old\nmine = 1\n");
    assert!(record_ids(&env).is_empty());
}

#[test]
fn symlink_apply_and_undo() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_b(&env);
    let real = env.write("dotfiles/zshrc", "export X=1\n");
    let link = home(&env, ".zshrc");
    symlink("dotfiles/zshrc", &link).unwrap();

    let report = run(&env, &ember).unwrap();
    applied(&report, "b");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert!(read(&real).starts_with("export X=1\n# >>> irida >>>\n"));

    undo(&ctx(&env)).unwrap();
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(read(&real), "export X=1\n");
}

#[test]
fn render_error_writes_nothing() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    put_target(
        &env,
        "r",
        &file_target("r", "~/.config/r/r.conf"),
        &[("t.tmpl", "{{ nope }}\n")],
    );

    match run(&env, &ember) {
        Err(ApplyError::Render { target, .. }) => assert_eq!(target, "r"),
        other => panic!("expected Render, got {other:?}"),
    }
    assert!(!home(&env, ".config/a").exists());
    assert!(!home(&env, ".config/r").exists());
    assert!(record_ids(&env).is_empty());
}

#[test]
fn syntax_error_skips_one_target() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    put_target(
        &env,
        "s",
        &file_target("s", "~/.config/s/s.conf"),
        &[("t.tmpl", "{% for %}\n")],
    );

    let report = run(&env, &ember).unwrap();
    applied(&report, "a");
    assert!(
        matches!(status(&report, "s"), TargetStatus::Skipped(SkipReason::Broken(_))),
        "{:?}",
        status(&report, "s")
    );
    assert!(home(&env, ".config/a/a.conf").exists());
    assert!(!home(&env, ".config/s").exists());
}

#[test]
fn write_failure_rolls_back() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_target(
        &env,
        "a",
        r#"name = "a"
detect = {}

[[file]]
path = "~/.config/a/a.conf"
write = "file"
template = "t.tmpl"

[[file]]
path = "~/.config/a/new.conf"
write = "file"
template = "t.tmpl"
"#,
        &[("t.tmpl", "accent = {{ accent }}\n")],
    );
    put_target(
        &env,
        "z",
        &file_target("z", "~/.config/blocker/b.conf"),
        &[("t.tmpl", "x\n")],
    );
    let a_conf = env.write(".config/a/a.conf", "old");
    env.write(".config/blocker", "not a folder");

    match run(&env, &ember) {
        Err(e @ ApplyError::Write { rolled_back: true, .. }) => {
            assert!(e.to_string().contains("blocker/b.conf"), "{e}");
        }
        other => panic!("expected rolled back Write, got {other:?}"),
    }
    assert_eq!(read(&a_conf), "old");
    assert!(!home(&env, ".config/a/new.conf").exists());
    assert!(record_ids(&env).is_empty());
}

#[test]
fn write_error_display_says_whether_record_was_kept() {
    let err = |rolled_back| ApplyError::Write {
        path: PathBuf::from("/h/.config/x.conf"),
        source: std::io::Error::other("disk full"),
        rolled_back,
    };
    let kept = err(false).to_string();
    assert!(kept.contains("/h/.config/x.conf"), "{kept}");
    assert!(kept.contains("record was kept"), "{kept}");
    assert!(kept.contains("irida undo"), "{kept}");
    let clean = err(true).to_string();
    assert!(clean.contains("/h/.config/x.conf"), "{clean}");
    assert!(!clean.contains("irida undo"), "{clean}");
}

#[test]
fn broken_markers_skip_the_file() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    put_b(&env);
    let broken = "# >>> irida >>>\nexport X=1\n# >>> irida >>>\n";
    let zshrc = env.write(".zshrc", broken);

    let report = run(&env, &ember).unwrap();
    applied(&report, "a");
    match status(&report, "b") {
        TargetStatus::Skipped(SkipReason::Broken(msg)) => {
            assert!(msg.contains(".zshrc"), "{msg}");
            assert!(msg.contains("irida markers are broken"), "{msg}");
        }
        other => panic!("expected Broken, got {other:?}"),
    }
    assert!(home(&env, ".config/a/a.conf").exists());
    assert_eq!(read(&zshrc), broken);
}

#[test]
fn nothing_to_apply_lists_every_target() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_b(&env);
    env.write(".zshrc", "# <<< irida <<<\n");

    match run(&env, &ember) {
        Err(ApplyError::NothingToApply(results)) => {
            let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
            assert_eq!(names, ["b", "cosmic-term"]);
        }
        other => panic!("expected NothingToApply, got {other:?}"),
    }
    assert!(record_ids(&env).is_empty());
}

#[test]
fn selection_rules() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    put_target(&env, "d", &file_target("d", "~/.config/d/d.conf"), &[("t.tmpl", "d\n")]);
    put_target(
        &env,
        "u",
        "name = \"u\"\ndetect = { binary = \"irida-test-absent\" }\n\n[[file]]\npath = \"~/.config/u/u.conf\"\nwrite = \"file\"\ntemplate = \"t.tmpl\"\n",
        &[("t.tmpl", "u\n")],
    );
    put_target(&env, "x", "name = \"x\"\nbogus = 1\n", &[]);
    let settings = Settings { disabled: ["d".to_string()].into(), last_palette: None };

    let report = run_with(&env, &settings, &ember, None, None).unwrap();
    applied(&report, "a");
    assert_eq!(status(&report, "u"), &TargetStatus::Skipped(SkipReason::NotInstalled));
    assert_eq!(status(&report, "d"), &TargetStatus::Skipped(SkipReason::Disabled));
    assert!(
        matches!(status(&report, "x"), TargetStatus::Skipped(SkipReason::Broken(_))),
        "{:?}",
        status(&report, "x")
    );
    assert!(!home(&env, ".config/d").exists());
    assert!(!home(&env, ".config/u").exists());

    let report = run_with(&env, &settings, &ember, None, Some(&["d"])).unwrap();
    applied(&report, "d");
    assert!(report.get("a").is_none());
    assert!(report.get("cosmic-term").is_none());
    assert_eq!(report.targets.len(), 1);
    assert_eq!(read(&home(&env, ".config/d/d.conf")), "d\n");

    match run_with(&env, &settings, &ember, None, Some(&["a", "nope"])) {
        Err(ApplyError::UnknownTarget(name)) => assert_eq!(name, "nope"),
        other => panic!("expected UnknownTarget, got {other:?}"),
    }
}

fn ron_text<T: serde::Serialize>(value: &T) -> String {
    ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new()).unwrap()
}

fn gruvbox() -> ColorScheme {
    ColorScheme {
        name: "Gruvbox".into(),
        foreground: Some(HexColor::rgb(0x11, 0x11, 0x11)),
        ..Default::default()
    }
}

/// Put a COSMIC Terminal key file, which also makes the target detected.
fn put_term_key(env: &TestEnv, key: &str, contents: &str) -> PathBuf {
    let dir = config_dir(&env.dirs);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(key);
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn cosmic_term_apply_and_undo() {
    let env = TestEnv::new();
    let (ember, emberday) = (palette("ember"), palette("emberday"));
    let dark_text = ron_text(&Schemes::from([(0, gruvbox())]));
    let dark = put_term_key(&env, "color_schemes_dark", &dark_text);
    let theme = put_term_key(&env, "syntax_theme_dark", "\"Gruvbox\"");
    let dir = config_dir(&env.dirs);

    let report = run_with(&env, &Settings::default(), &ember, Some(&emberday), None).unwrap();
    applied(&report, "cosmic-term");
    let dark_map: Schemes = ron::from_str(&read(&dark)).unwrap();
    assert_eq!(
        dark_map,
        Schemes::from([(0, gruvbox()), (1, derive(&ember).term.to_scheme("Ember"))])
    );
    let light_map: Schemes = ron::from_str(&read(&dir.join("color_schemes_light"))).unwrap();
    assert_eq!(
        light_map,
        Schemes::from([(0, derive(&emberday).term.to_scheme("Ember Day"))])
    );
    assert_eq!(read(&theme), "\"Ember\"");
    assert_eq!(read(&dir.join("syntax_theme_light")), "\"Ember Day\"");

    let report = undo(&ctx(&env)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(fs::read(&dark).unwrap(), dark_text.as_bytes());
    assert_eq!(fs::read(&theme).unwrap(), b"\"Gruvbox\"");
    assert!(!dir.join("color_schemes_light").exists());
    assert!(!dir.join("syntax_theme_light").exists());
}

#[test]
fn cosmic_term_bad_config_skips_only_that_target() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    let text = ron_text(&Schemes::from([(0, gruvbox())]))
        .replace("name: \"Gruvbox\",", "name: \"Gruvbox\",\n        extra: 1,");
    assert!(text.contains("extra: 1"));
    let dark = put_term_key(&env, "color_schemes_dark", &text);

    let report = run(&env, &ember).unwrap();
    applied(&report, "a");
    assert!(
        matches!(
            status(&report, "cosmic-term"),
            TargetStatus::Skipped(SkipReason::Broken(_))
        ),
        "{:?}",
        status(&report, "cosmic-term")
    );
    assert_eq!(read(&dark), text);
    assert!(!config_dir(&env.dirs).join("syntax_theme_dark").exists());
}

#[test]
fn records_are_pruned() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    for n in 1..=11 {
        assert_eq!(run(&env, &ember).unwrap().record_id, n);
    }
    assert_eq!(record_ids(&env), (2..=11).collect::<Vec<u32>>());
}

#[test]
fn capture_failure_leaves_no_record() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    let a_conf = env.write(".config/a/a.conf", "old");
    fs::set_permissions(&a_conf, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&a_conf).is_ok() {
        // Running with privileges that ignore file modes; nothing to test.
        return;
    }

    let result = run(&env, &ember);
    fs::set_permissions(&a_conf, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(result, Err(ApplyError::Record(_))), "{result:?}");
    assert_eq!(read(&a_conf), "old");
    assert!(record_ids(&env).is_empty());
}

#[test]
fn undo_with_nothing() {
    let env = TestEnv::new();
    assert!(matches!(undo(&ctx(&env)), Err(UndoError::NothingToUndo)));
}
