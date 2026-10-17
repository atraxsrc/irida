mod common;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use common::{TestEnv, apply_only, palette};
use hex_color::HexColor;
use irida_core::apply::{
    ApplyError, ApplyReport, ApplyRequest, Ctx, SkipReason, TargetInfo, TargetStatus, UndoError,
    app_path, apply, target_status, undo,
};
use irida_core::derive::derive;
use irida_core::native::cosmic_term::config_dir;
use irida_core::palette::Palette;
use irida_core::reload::ReloadStatus;
use irida_core::store::Settings;
use irida_core::target::load_targets;
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
    Ctx {
        dirs: env.dirs.clone(),
        path: vec![],
    }
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
        live_only: false,
    };
    apply(&ctx(env), settings, &req)
}

fn status<'a>(report: &'a ApplyReport, name: &str) -> &'a TargetStatus {
    &report
        .get(name)
        .unwrap_or_else(|| panic!("{name} in report"))
        .status
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

    assert_eq!(
        read(&home(&env, ".config/a/a.conf")),
        render(&ember, A_TMPL)
    );
    assert_eq!(
        read(&home(&env, ".zshrc")),
        format!(
            "export X=1\n# >>> irida >>>\n{}# <<< irida <<<\n",
            render(&ember, B_TMPL)
        )
    );
    assert_eq!(
        read(&home(&env, ".config/c/c.conf")),
        "a = 1\ntheme = ember\n"
    );
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
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(read(&real).starts_with("export X=1\n# >>> irida >>>\n"));

    undo(&ctx(&env)).unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
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
        matches!(
            status(&report, "s"),
            TargetStatus::Skipped(SkipReason::Broken(_))
        ),
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
        Err(
            e @ ApplyError::Write {
                rolled_back: true, ..
            },
        ) => {
            assert!(e.to_string().contains("blocker/b.conf"), "{e}");
        }
        other => panic!("expected rolled back Write, got {other:?}"),
    }
    assert_eq!(read(&a_conf), "old");
    assert!(!home(&env, ".config/a/new.conf").exists());
    assert!(record_ids(&env).is_empty());
}

/// Restores a folder's mode so the TempDir can be removed.
struct RestoreMode(PathBuf);

impl Drop for RestoreMode {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn rollback_skips_unattempted_writes() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_target(
        &env,
        "a",
        &file_target("a", "~/.config/a/a.conf"),
        &[("t.tmpl", "x\n")],
    );
    put_target(
        &env,
        "m",
        &file_target("m", "~/.config/blocker/b.conf"),
        &[("t.tmpl", "x\n")],
    );
    put_target(
        &env,
        "z",
        &file_target("z", "~/.config/ro/c.conf"),
        &[("t.tmpl", "x\n")],
    );
    let a_conf = env.write(".config/a/a.conf", "old");
    env.write(".config/blocker", "not a folder");
    let c_conf = env.write(".config/ro/c.conf", "keep");
    let ro = c_conf.parent().unwrap().to_path_buf();
    let _restore = RestoreMode(ro.clone());
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(ro.join("probe"), b"").is_ok() {
        // Running with privileges that ignore folder modes; nothing to test.
        return;
    }

    match run(&env, &ember) {
        Err(
            e @ ApplyError::Write {
                rolled_back: true, ..
            },
        ) => {
            assert!(e.to_string().contains("blocker/b.conf"), "{e}");
        }
        other => panic!("expected rolled back Write, got {other:?}"),
    }
    assert_eq!(read(&a_conf), "old");
    assert_eq!(read(&c_conf), "keep");
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
            let loaded = load_targets(&env.dirs);
            let mut expected: Vec<&str> = loaded.iter().map(|t| t.name.as_str()).collect();
            expected.sort();
            assert!(expected.contains(&"b"));
            assert_eq!(names, expected);
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
    put_target(
        &env,
        "d",
        &file_target("d", "~/.config/d/d.conf"),
        &[("t.tmpl", "d\n")],
    );
    put_target(
        &env,
        "u",
        "name = \"u\"\ndetect = { binary = \"irida-test-absent\" }\n\n[[file]]\npath = \"~/.config/u/u.conf\"\nwrite = \"file\"\ntemplate = \"t.tmpl\"\n",
        &[("t.tmpl", "u\n")],
    );
    put_target(&env, "x", "name = \"x\"\nbogus = 1\n", &[]);
    let settings = Settings {
        disabled: ["d".to_string()].into(),
        last_palette: None,
    };

    let report = run_with(&env, &settings, &ember, None, None).unwrap();
    applied(&report, "a");
    assert_eq!(
        status(&report, "u"),
        &TargetStatus::Skipped(SkipReason::NotInstalled)
    );
    assert_eq!(
        status(&report, "d"),
        &TargetStatus::Skipped(SkipReason::Disabled)
    );
    assert!(
        matches!(
            status(&report, "x"),
            TargetStatus::Skipped(SkipReason::Broken(_))
        ),
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
    let text = ron_text(&Schemes::from([(0, gruvbox())])).replace(
        "name: \"Gruvbox\",",
        "name: \"Gruvbox\",\n        extra: 1,",
    );
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

fn warnings<'a>(report: &'a ApplyReport, name: &str) -> &'a [String] {
    &report
        .get(name)
        .unwrap_or_else(|| panic!("{name} in report"))
        .warnings
}

fn broken(status: &TargetStatus) -> &str {
    match status {
        TargetStatus::Skipped(SkipReason::Broken(msg)) => msg,
        other => panic!("expected Broken, got {other:?}"),
    }
}

#[test]
fn signal_needs_binary_on_path() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_target(
        &env,
        "f",
        r#"name = "f"
detect = { binary = "iridafake", config = "~/.config/f" }

[[file]]
path = "~/.config/f/f.conf"
write = "file"
template = "t.tmpl"

[reload]
signal = { process = "iridafake", signal = "TERM" }
"#,
        &[("t.tmpl", "x\n")],
    );
    fs::create_dir_all(home(&env, ".config/f")).unwrap();
    let report = run(&env, &ember).unwrap();
    assert_eq!(
        status(&report, "f"),
        &TargetStatus::Applied(ReloadStatus::NotRunning)
    );
}

#[test]
fn signal_needs_a_binary() {
    let env = TestEnv::new();
    let ember = palette("ember");
    // A process with a name nothing else uses, started by this test.
    let bin = env.tmp.path().join("iridanobin");
    fs::copy("/bin/sleep", &bin).unwrap();
    let mut child = std::process::Command::new(&bin).arg("30").spawn().unwrap();
    put_target(
        &env,
        "g",
        r#"name = "g"
detect = { config = "~/.config/g" }

[[file]]
path = "~/.config/g/g.conf"
write = "file"
template = "t.tmpl"

[reload]
signal = { process = "iridanobin", signal = "TERM" }
"#,
        &[("t.tmpl", "x\n")],
    );
    fs::create_dir_all(home(&env, ".config/g")).unwrap();
    let report = run(&env, &ember).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let alive = child.try_wait().unwrap().is_none();
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(
        status(&report, "g"),
        &TargetStatus::Applied(ReloadStatus::NotRunning)
    );
    assert!(alive, "a target with no binary signalled a process");
}

#[test]
fn same_path_skip_gives_its_reason() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_target(
        &env,
        "a",
        &file_target("a", "~/.config/x/f.conf"),
        &[("t.tmpl", "from a\n")],
    );
    put_target(
        &env,
        "b",
        &file_target("b", "~/.config/x/f.conf"),
        &[("t.tmpl", "from b\n")],
    );
    let conf = env.write(".config/x/f.conf", "mine\n");

    let report = run(&env, &ember).unwrap();
    applied(&report, "a");
    assert!(
        warnings(&report, "a")
            .iter()
            .any(|w| w.contains("replaced your file"))
    );
    let msg = broken(status(&report, "b"));
    assert!(msg.contains("already written by a"), "{msg}");
    assert!(
        warnings(&report, "b")
            .iter()
            .all(|w| !w.contains("replaced your file")),
        "{:?}",
        warnings(&report, "b")
    );
    assert_eq!(read(&conf), "from a\n");
}

#[test]
fn same_path_twice_in_one_target_keeps_note() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let twice = format!(
        "{}\n[[file]]\npath = \"~/.config/x/f.conf\"\nwrite = \"file\"\ntemplate = \"t.tmpl\"\n",
        file_target("d", "~/.config/x/f.conf")
    );
    put_target(&env, "d", &twice, &[("t.tmpl", "from d\n")]);
    let conf = env.write(".config/x/f.conf", "mine\n");

    let report = run(&env, &ember).unwrap();
    applied(&report, "d");
    let notes = warnings(&report, "d")
        .iter()
        .filter(|w| w.contains("replaced your file"))
        .count();
    assert_eq!(notes, 1, "{:?}", warnings(&report, "d"));
    assert_eq!(read(&conf), "from d\n");
}

fn block_target(name: &str, comment: &str) -> String {
    format!(
        "name = \"{name}\"\ndetect = {{}}\n\n[[file]]\npath = \"~/.zshrc\"\nwrite = \"block\"\ntemplate = \"t.tmpl\"\ncomment = \"{comment}\"\n"
    )
}

#[test]
fn same_path_second_target_skipped() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_target(&env, "a", &block_target("a", "#"), &[("t.tmpl", "a\n")]);
    put_target(&env, "b", &block_target("b", "##"), &[("t.tmpl", "b\n")]);
    let zshrc = env.write(".zshrc", "export X=1\n");

    let report = run(&env, &ember).unwrap();
    applied(&report, "a");
    let msg = broken(status(&report, "b"));
    assert!(msg.contains("already written by a"), "{msg}");
    assert_eq!(
        read(&zshrc),
        "export X=1\n# >>> irida >>>\na\n# <<< irida <<<\n"
    );

    undo(&ctx(&env)).unwrap();
    assert_eq!(read(&zshrc), "export X=1\n");
}

#[test]
fn same_path_within_one_target() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_target(
        &env,
        "x",
        r##"name = "x"
detect = {}

[[file]]
path = "~/.config/x/app.conf"
write = "key"
key = "theme"
template = "k.tmpl"

[[file]]
path = "~/.config/x/app.conf"
write = "block"
comment = "#"
template = "b.tmpl"
"##,
        &[("k.tmpl", "irida\n"), ("b.tmpl", "colour = 1\n")],
    );

    let report = run(&env, &ember).unwrap();
    applied(&report, "x");
    assert_eq!(read(&home(&env, ".config/x/app.conf")), "theme = irida\n");
    let w = warnings(&report, "x");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("already written by x"), "{}", w[0]);
}

const CO_TOML: &str = r#"name = "co"
detect = {}

[[file]]
path = "~/.config/c/conf.yaml"
write = "file"
template = "t.tmpl"
create_only = true
"#;
const CO_TMPL: &str = "color:\n  theme: custom\n";

#[test]
fn create_only_cases() {
    let ember = palette("ember");
    let setup = |existing: Option<&str>| {
        let env = TestEnv::new();
        put_a(&env);
        put_target(&env, "co", CO_TOML, &[("t.tmpl", CO_TMPL)]);
        let path = home(&env, ".config/c/conf.yaml");
        if let Some(text) = existing {
            env.write(".config/c/conf.yaml", text);
        }
        (env, path)
    };

    let (env, path) = setup(None);
    let report = run(&env, &ember).unwrap();
    applied(&report, "co");
    assert_eq!(read(&path), CO_TMPL);
    undo(&ctx(&env)).unwrap();
    assert!(!path.exists());

    let (env, path) = setup(Some("color:\n  when: auto\n"));
    let report = run(&env, &ember).unwrap();
    let msg = broken(status(&report, "co"));
    assert!(msg.ends_with("add `theme: custom` under `color:`"), "{msg}");
    assert_eq!(read(&path), "color:\n  when: auto\n");

    let (env, _) = setup(Some("color:\n  theme: custom\n"));
    let report = run(&env, &ember).unwrap();
    assert_eq!(broken(status(&report, "co")), "nothing to write");

    let (env, path) = setup(Some("color:\n  when: auto\n"));
    put_target(
        &env,
        "co2",
        &format!(
            "{}\n[[file]]\npath = \"~/.config/c/other.conf\"\nwrite = \"file\"\ntemplate = \"t.tmpl\"\n",
            CO_TOML.replace("name = \"co\"", "name = \"co2\"")
        ),
        &[("t.tmpl", CO_TMPL)],
    );
    let report = run_with(
        &env,
        &Settings::default(),
        &ember,
        None,
        Some(&["a", "co2"]),
    )
    .unwrap();
    applied(&report, "co2");
    assert!(
        warnings(&report, "co2")
            .iter()
            .any(|w| w.ends_with("add `theme: custom` under `color:`")),
        "{:?}",
        warnings(&report, "co2")
    );
    assert_eq!(read(&path), "color:\n  when: auto\n");
    assert_eq!(read(&home(&env, ".config/c/other.conf")), CO_TMPL);
}

#[test]
fn replaced_file_note() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_target(
        &env,
        "r",
        &file_target("r", "~/.config/r/r.conf"),
        &[("t.tmpl", "irida\n")],
    );
    env.write(".config/r/r.conf", "mine");

    let report = run(&env, &ember).unwrap();
    applied(&report, "r");
    assert!(
        warnings(&report, "r")
            .iter()
            .any(|w| w.contains("replaced your file")),
        "{:?}",
        warnings(&report, "r")
    );
    fs::write(home(&env, ".config/r/r.conf"), "changed by hand\n").unwrap();
    let report = run(&env, &ember).unwrap();
    assert!(
        warnings(&report, "r").is_empty(),
        "{:?}",
        warnings(&report, "r")
    );
}

#[test]
fn hint_reaches_report() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_target(
        &env,
        "h",
        &format!(
            "{}\n[reload]\nhint = \"x\"\n",
            file_target("h", "~/.config/h/h.conf")
        ),
        &[("t.tmpl", "x\n")],
    );
    let report = run(&env, &ember).unwrap();
    assert_eq!(
        status(&report, "h"),
        &TargetStatus::Applied(ReloadStatus::Restart("x".into()))
    );
}

/// Every path below `root` with its bytes (`None` for a folder), sorted.
fn listing(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                todo.push(path.clone());
                out.push((path, None));
            } else {
                let bytes = fs::read(&path).unwrap();
                out.push((path, Some(bytes)));
            }
        }
    }
    out.sort();
    out
}

const BUILTINS: [&str; 17] = [
    "alacritty",
    "bat",
    "btop",
    "cava",
    "cosmic-desktop",
    "cosmic-term",
    "delta",
    "eza",
    "fastfetch",
    "fish",
    "foot",
    "ghostty",
    "helix",
    "kitty",
    "lsd",
    "nvim",
    "zsh",
];
const LIVE: [&str; 3] = ["cava", "cosmic-desktop", "cosmic-term"];

/// The display name each built-in target sets.
fn builtin_display(name: &str) -> &'static str {
    match name {
        "alacritty" => "Alacritty",
        "bat" => "bat",
        "cosmic-desktop" => "COSMIC desktop",
        "cosmic-term" => "COSMIC Terminal",
        "delta" => "delta",
        "eza" => "eza",
        "nvim" => "Neovim",
        "btop" => "btop",
        "cava" => "cava",
        "fastfetch" => "fastfetch",
        "fish" => "fish",
        "foot" => "foot",
        "ghostty" => "Ghostty",
        "helix" => "Helix",
        "kitty" => "kitty",
        "lsd" => "lsd",
        "zsh" => "zsh",
        other => panic!("no display for {other}"),
    }
}

fn info<'a>(status: &'a [TargetInfo], name: &str) -> &'a TargetInfo {
    status
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("{name} in status"))
}

#[test]
fn status_lists_without_writing() {
    let env = TestEnv::new();
    put_a(&env);
    put_target(&env, "bad", "name = \"bad\"\nbogus = 1\n", &[]);
    let settings = Settings {
        disabled: ["a".to_string()].into(),
        last_palette: None,
    };
    let before = listing(env.tmp.path());

    let status = target_status(&ctx(&env), &settings);
    assert_eq!(listing(env.tmp.path()), before);

    let names: Vec<&str> = status.iter().map(|t| t.name.as_str()).collect();
    let mut expected: Vec<&str> = BUILTINS.iter().copied().chain(["a", "bad"]).collect();
    expected.sort();
    assert_eq!(names, expected);

    assert_eq!(
        info(&status, "a"),
        &TargetInfo {
            name: "a".into(),
            display: "a".into(),
            builtin: false,
            live: false,
            detected: true,
            enabled: false,
            broken: None,
        }
    );
    let bad = info(&status, "bad");
    assert!(
        bad.broken.as_deref().is_some_and(|m| m.contains("bogus")),
        "{bad:?}"
    );
    assert!(!bad.builtin && !bad.live && bad.enabled, "{bad:?}");
    for name in BUILTINS {
        assert_eq!(
            info(&status, name),
            &TargetInfo {
                name: name.into(),
                display: builtin_display(name).into(),
                builtin: true,
                live: LIVE.contains(&name),
                detected: false,
                enabled: true,
                broken: None,
            }
        );
    }
}

#[test]
fn status_reports_template_and_config_errors() {
    let env = TestEnv::new();
    put_target(
        &env,
        "s",
        &file_target("s", "~/.config/s/s.conf"),
        &[("t.tmpl", "{% for %}\n")],
    );
    put_target(
        &env,
        "nvim",
        &file_target("nvim", "~/.config/nvim/x.lua"),
        &[("t.tmpl", "x\n")],
    );
    let text = ron_text(&Schemes::from([(0, gruvbox())])).replace(
        "name: \"Gruvbox\",",
        "name: \"Gruvbox\",\n        extra: 1,",
    );
    let dark = put_term_key(&env, "color_schemes_dark", &text);
    let before = listing(env.tmp.path());

    let status = target_status(&ctx(&env), &Settings::default());
    assert_eq!(listing(env.tmp.path()), before);
    assert_eq!(read(&dark), text);

    let s = info(&status, "s");
    assert!(s.detected && s.broken.is_some(), "{s:?}");
    let term = info(&status, "cosmic-term");
    assert!(term.detected && term.live && term.builtin, "{term:?}");
    assert!(
        term.broken
            .as_deref()
            .is_some_and(|m| m.contains("color_schemes_dark")),
        "{term:?}"
    );
    // A user target that replaces a built-in is the user's.
    let nvim = info(&status, "nvim");
    assert!(
        !nvim.builtin && !nvim.live && nvim.broken.is_none(),
        "{nvim:?}"
    );
}

#[test]
fn app_path_adds_standard_dirs() {
    let env = TestEnv::new();
    let home = &env.dirs.home;
    let std_dirs = || {
        vec![
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            home.join(".local/bin"),
            home.join(".cargo/bin"),
        ]
    };

    let mut expected = vec![PathBuf::from("/opt/x")];
    expected.extend(std_dirs());
    assert_eq!(app_path(&env.dirs, vec!["/opt/x".into()]), expected);

    let path = app_path(&env.dirs, vec!["/usr/bin".into(), "/opt/x".into()]);
    let mut expected = vec![PathBuf::from("/usr/bin"), PathBuf::from("/opt/x")];
    expected.extend(
        std_dirs()
            .into_iter()
            .filter(|p| p != Path::new("/usr/bin")),
    );
    assert_eq!(path, expected);
    assert_eq!(
        path.iter().filter(|p| *p == Path::new("/usr/bin")).count(),
        1
    );

    assert_eq!(app_path(&env.dirs, vec![]), std_dirs());
    assert_eq!(app_path(&env.dirs, std_dirs()), std_dirs());
}

#[test]
fn app_path_detects_home_binaries() {
    let env = TestEnv::new();
    put_target(
        &env,
        "tool",
        "name = \"tool\"\ndetect = { binary = \"irida-test-tool\" }\n\n[[file]]\npath = \"~/.config/tool/t.conf\"\nwrite = \"file\"\ntemplate = \"t.tmpl\"\n",
        &[("t.tmpl", "x\n")],
    );
    let bin = env.write(".local/bin/irida-test-tool", "#!/bin/sh\n");
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

    let short = target_status(&ctx(&env), &Settings::default());
    assert!(!info(&short, "tool").detected);

    let app = Ctx {
        dirs: env.dirs.clone(),
        path: app_path(&env.dirs, vec![]),
    };
    assert!(info(&target_status(&app, &Settings::default()), "tool").detected);
}

#[test]
fn live_only_selects_live_targets() {
    let env = TestEnv::new();
    let ember = palette("ember");
    put_a(&env);
    put_target(&env, "bad", "name = \"bad\"\nbogus = 1\n", &[]);
    let cava = env.write(".config/cava/config", "x\n");

    let req = ApplyRequest {
        palette: &ember,
        pair: None,
        only: None,
        live_only: true,
    };
    let report = apply(&ctx(&env), &Settings::default(), &req).unwrap();

    assert_eq!(
        status(&report, "cava"),
        &TargetStatus::Applied(ReloadStatus::NotRunning)
    );
    let names: Vec<&str> = report.targets.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, LIVE);
    assert!(report.get("a").is_none());
    assert!(report.get("bad").is_none());
    assert!(!home(&env, ".config/a").exists());
    let text = read(&cava);
    assert!(text.starts_with("x\n") && text.len() > 2, "{text}");

    common::undo(&env);
    assert_eq!(read(&cava), "x\n");
}

#[test]
fn status_and_report_carry_display() {
    let env = TestEnv::new();
    let ctx = Ctx {
        dirs: env.dirs.clone(),
        path: vec![],
    };
    let infos = target_status(&ctx, &Settings::default());
    let btop = infos.iter().find(|t| t.name == "btop").unwrap();
    assert_eq!(btop.display, "btop");
    let term = infos.iter().find(|t| t.name == "cosmic-term").unwrap();
    assert_eq!(term.display, "COSMIC Terminal");
    env.write(".config/btop/btop.conf", "color_theme = \"Default\"\n");
    let report = common::apply_only(&env, &palette("ember"), "btop").unwrap();
    assert_eq!(report.get("btop").unwrap().display, "btop");
}

const D_TOML: &str = r#"name = "d"
detect = {}

[[file]]
path = "~/.config/d/d.conf"
write = "key"
template = "d.tmpl"
key = "include"
separator = " "
"#;

#[test]
fn key_with_space_separator_round_trips() {
    let env = TestEnv::new();
    put_target(&env, "d", D_TOML, &[("d.tmpl", "{{ slug }}.conf\n")]);
    let conf = env.write(".config/d/d.conf", "font 12\ninclude mine.conf\n");
    apply_only(&env, &palette("ember"), "d").unwrap();
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "font 12\ninclude ember.conf\n"
    );
    common::undo(&env);
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "font 12\ninclude mine.conf\n"
    );
}

const E_TOML: &str = r#"name = "e"
detect = {}

[[file]]
path = "~/.config/e/e.toml"
write = "key"
template = "e.tmpl"
key = "import"
section = "general"
"#;

const F_TOML: &str = r#"name = "f"
detect = {}

[[file]]
path = "~/.config/f/config.toml"
write = "key"
template = "f.tmpl"
key = "theme"
place = "top"
"#;

#[test]
fn key_in_section_round_trips() {
    let env = TestEnv::new();
    put_target(&env, "e", E_TOML, &[("e.tmpl", "[\"{{ slug }}.toml\"]\n")]);
    let text =
        "[font]\nsize = 12\n\n[general]\nlive_config_reload = true\n\n[window]\nimport = [\"x\"]\n";
    let conf = env.write(".config/e/e.toml", text);
    apply_only(&env, &palette("ember"), "e").unwrap();
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[font]\nsize = 12\n\n[general]\nlive_config_reload = true\nimport = [\"ember.toml\"]\n\n[window]\nimport = [\"x\"]\n"
    );
    common::undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), text);
}

#[test]
fn key_at_top_round_trips_without_trailing_newline() {
    let env = TestEnv::new();
    put_target(&env, "f", F_TOML, &[("f.tmpl", "\"{{ slug }}\"\n")]);
    let conf = env.write(
        ".config/f/config.toml",
        "[editor]\nline-number = \"relative\"",
    );
    apply_only(&env, &palette("ember"), "f").unwrap();
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "theme = \"ember\"\n[editor]\nline-number = \"relative\""
    );
    common::undo(&env);
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "[editor]\nline-number = \"relative\""
    );
}

const G_TOML: &str = r#"name = "g"
detect = {}

[[file]]
path = "~/.config/g/g.toml"
write = "key"
template = "g.tmpl"
key = "import"
section = "general"
note_replaced = true
"#;

#[test]
fn replaced_key_line_is_noted() {
    let env = TestEnv::new();
    put_target(&env, "g", G_TOML, &[("g.tmpl", "[\"{{ slug }}.toml\"]\n")]);
    let conf = env.write(".config/g/g.toml", "[general]\nimport = [\"mine.toml\"]\n");
    let note = |report: &ApplyReport| {
        report
            .get("g")
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.contains("replaced your import line; irida undo restores it"))
    };
    let report = apply_only(&env, &palette("ember"), "g").unwrap();
    assert!(note(&report), "{:?}", report.get("g").unwrap().warnings);
    let report = apply_only(&env, &palette("emberday"), "g").unwrap();
    assert!(!note(&report), "{:?}", report.get("g").unwrap().warnings);

    fs::write(&conf, "[general]\n").unwrap();
    let report = apply_only(&env, &palette("ember"), "g").unwrap();
    assert!(!note(&report), "{:?}", report.get("g").unwrap().warnings);
}

const H_TOML: &str = r#"name = "h"
detect = {}

[[file]]
path = "~/.config/h/h.conf"
write = "key"
template = "h.tmpl"
key = "include"
separator = " "
repeatable = true
"#;

#[test]
fn repeatable_key_round_trips() {
    let env = TestEnv::new();
    put_target(&env, "h", H_TOML, &[("h.tmpl", "irida.conf\n")]);
    let conf = env.write(".config/h/h.conf", "include mine.conf\n");
    apply_only(&env, &palette("ember"), "h").unwrap();
    assert_eq!(
        fs::read_to_string(&conf).unwrap(),
        "include mine.conf\ninclude irida.conf\n"
    );
    common::undo(&env);
    assert_eq!(fs::read_to_string(&conf).unwrap(), "include mine.conf\n");
}

const N_TOML: &str = r#"name = "n"
detect = {}
note = "after undo run n --rebuild"

[[file]]
path = "~/.config/n/n.conf"
write = "file"
template = "n.tmpl"
"#;

#[test]
fn target_note_is_reported_on_apply() {
    let env = TestEnv::new();
    put_target(&env, "n", N_TOML, &[("n.tmpl", "{{ accent }}\n")]);
    let report = apply_only(&env, &palette("ember"), "n").unwrap();
    let t = report.get("n").unwrap();
    assert!(
        t.warnings.iter().any(|w| w == "after undo run n --rebuild"),
        "{:?}",
        t.warnings
    );
}

#[test]
fn undo_leaves_a_key_line_the_user_changed() {
    let env = TestEnv::new();
    put_c(&env);
    let c_conf = env.write(".config/c/c.conf", "a = 1\ntheme = old\n");
    run(&env, &palette("ember")).unwrap();
    assert_eq!(read(&c_conf), "a = 1\ntheme = ember\n");
    fs::write(&c_conf, "a = 1\ntheme = mine\n").unwrap();

    let report = undo(&ctx(&env)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&c_conf), "a = 1\ntheme = mine\n");
}
