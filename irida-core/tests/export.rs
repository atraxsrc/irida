mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use common::{TestEnv, fixture, palette, render_export, rice_text};
use irida_core::export::{ExportError, ExportReport, ExportRequest, ExportStatus, export};
use irida_core::import::import_rice;
use irida_core::palette::Palette;

fn run(env: &TestEnv, p: &Palette, folder: &Path) -> Result<ExportReport, ExportError> {
    export(
        &env.dirs,
        &ExportRequest {
            palette: p,
            pair: None,
            folder,
        },
    )
}

fn status<'a>(r: &'a ExportReport, path: &str) -> Vec<&'a ExportStatus> {
    r.lines
        .iter()
        .filter(|l| l.path == path)
        .map(|l| &l.status)
        .collect()
}

fn put_target(env: &TestEnv, name: &str, toml: &str, templates: &[(&str, &str)]) {
    let dir = env.dirs.config.join("irida/targets").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("target.toml"), toml).unwrap();
    for (file, text) in templates {
        fs::write(dir.join(file), text).unwrap();
    }
}

fn rice_target(name: &str, paths: &[&str]) -> String {
    let mut out = format!("name = \"{name}\"\n");
    for p in paths {
        out.push_str(&format!(
            "\n[[rice]]\npath = \"{p}\"\ntemplate = \"t.tmpl\"\n"
        ));
    }
    out
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

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// The rice files an export must reproduce byte for byte.
fn rice_files(slug: &str, stem: &str, mode: &str) -> Vec<String> {
    let mut files: Vec<String> = [
        format!("cosmic/{stem}.ron"),
        "cosmic/install.sh".into(),
        format!("cosmic/config/com.system76.CosmicTerm/v1/syntax_theme_{mode}"),
        format!("cosmic-term/{stem}-term.ron"),
        format!("btop/{stem}.theme"),
        format!("nvim/colors/{slug}.lua"),
        format!("nvim/lua/plugins/{slug}.lua"),
        "nvim/syntax/ron.vim".into(),
        "nvim/install.sh".into(),
        "fastfetch/config.jsonc".into(),
        "fastfetch/cosmic.txt".into(),
        "fastfetch/install.sh".into(),
        "lsd/config.yaml".into(),
        "lsd/install.sh".into(),
        format!("zsh/{slug}.zsh"),
        "zsh/install.sh".into(),
        ".gitignore".into(),
        ".pre-commit-config.yaml".into(),
    ]
    .into();
    for f in [
        "bold_font_weight",
        "dim_font_weight",
        "font_name",
        "font_size",
        "font_weight",
        "opacity",
        "show_headerbar",
        "use_bright_bold",
    ] {
        files.push(format!("cosmic/config/com.system76.CosmicTerm/v1/{f}"));
    }
    for f in [
        "apply_theme_global",
        "interface_font",
        "monospace_font",
        "show_maximize",
        "show_minimize",
    ] {
        files.push(format!("cosmic/config/com.system76.CosmicTk/v1/{f}"));
    }
    files
}

#[test]
fn ember_repo_matches() {
    for (slug, stem, m) in [
        ("ember", "Ember", "dark"),
        ("emberday", "EmberDay", "light"),
    ] {
        let env = TestEnv::new();
        let parent = env.tmp.path().join("out");
        fs::create_dir_all(&parent).unwrap();
        let report = run(&env, &palette(slug), &parent).unwrap();
        let repo = parent.join(format!("cosmic-{slug}-theme"));
        assert_eq!(report.repo, repo);
        let files = rice_files(slug, stem, m);
        assert_eq!(files.len(), 31);
        for rel in &files {
            let got = fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
            assert_eq!(got, rice_text(slug, rel), "{slug} {rel}");
            assert_eq!(status(&report, rel), [&ExportStatus::Created], "{rel}");
        }
        for rel in files.iter().filter(|f| f.ends_with("install.sh")) {
            assert_eq!(mode(&repo.join(rel)), 0o755, "{rel}");
        }
        assert!(
            !repo
                .join("cosmic/config/com.system76.CosmicTk/v1/icon_theme")
                .exists()
        );
        assert!(repo.join("cava/install.sh").is_file());
        assert_eq!(mode(&repo.join("cava/install.sh")), 0o755);
        assert!(report.notes.is_empty(), "{:?}", report.notes);
    }
}

#[test]
fn second_export_unchanged() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let parent = env.tmp.path().join("out");
    run(&env, &ember, &parent).unwrap();
    let report = run(&env, &ember, &parent).unwrap();
    for line in &report.lines {
        assert!(
            matches!(line.status, ExportStatus::Unchanged | ExportStatus::Kept),
            "{line:?}"
        );
    }
    assert_eq!(status(&report, ".gitignore"), [&ExportStatus::Kept]);
}

#[test]
fn existing_repo_untouched() {
    let env = TestEnv::new();
    let repo = env.tmp.path().join("cosmic-ember-theme");
    let opacity = repo.join("cosmic/config/com.system76.CosmicTerm/v1/opacity");
    fs::create_dir_all(opacity.parent().unwrap()).unwrap();
    fs::create_dir_all(repo.join("backgrounds")).unwrap();
    fs::write(repo.join("backgrounds/x.png"), b"\x89PNG").unwrap();
    fs::write(&opacity, "77").unwrap();
    fs::write(repo.join("cosmic/Old.ron"), "()").unwrap();

    let report = run(&env, &palette("ember"), &repo).unwrap();
    assert_eq!(
        fs::read(repo.join("backgrounds/x.png")).unwrap(),
        b"\x89PNG"
    );
    assert_eq!(fs::read_to_string(&opacity).unwrap(), "77");
    assert_eq!(
        status(&report, "cosmic/config/com.system76.CosmicTerm/v1/opacity"),
        [&ExportStatus::Kept]
    );
    assert!(repo.join("cosmic/Old.ron").exists());
    assert_eq!(status(&report, "cosmic/Old.ron"), [&ExportStatus::Stale]);
}

#[test]
fn escaping_rice_path_rejected() {
    let env = TestEnv::new();
    let absolute = env.tmp.path().join("evil");
    put_target(
        &env,
        "evil",
        &rice_target(
            "evil",
            &["../evil", absolute.to_str().unwrap(), "{{ '..' }}/evil"],
        ),
        &[("t.tmpl", "x\n")],
    );
    let parent = env.tmp.path().join("out");
    let report = run(&env, &palette("ember"), &parent).unwrap();
    let skipped: Vec<&str> = report
        .lines
        .iter()
        .filter(|l| l.status == ExportStatus::Skipped("path leaves the repo".into()))
        .map(|l| l.path.as_str())
        .collect();
    assert_eq!(skipped.len(), 3, "{:?}", report.lines);
    assert!(!absolute.exists());
    assert!(!parent.join("evil").exists());
    let names: Vec<_> = fs::read_dir(&parent)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["cosmic-ember-theme"]);
}

#[test]
fn symlink_escape_rejected() {
    let env = TestEnv::new();
    let outside = env.tmp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let repo = env.tmp.path().join("cosmic-ember-theme");
    fs::create_dir_all(&repo).unwrap();
    symlink(&outside, repo.join("nvim")).unwrap();

    let report = run(&env, &palette("ember"), &repo).unwrap();
    for rel in [
        "nvim/colors/ember.lua",
        "nvim/lua/plugins/ember.lua",
        "nvim/syntax/ron.vim",
        "nvim/install.sh",
    ] {
        assert_eq!(
            status(&report, rel),
            [&ExportStatus::Skipped("path leaves the repo".into())],
            "{rel}"
        );
    }
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
fn user_target_cannot_overwrite() {
    let env = TestEnv::new();
    put_target(
        &env,
        "mine",
        &rice_target("mine", &["nvim/install.sh"]),
        &[("t.tmpl", "mine\n")],
    );
    let repo = env.tmp.path().join("cosmic-ember-theme");
    let report = run(&env, &palette("ember"), &repo).unwrap();
    let st = status(&report, "nvim/install.sh");
    assert!(
        st.contains(&&ExportStatus::Skipped("already written by nvim".into())),
        "{st:?}"
    );
    assert_eq!(
        fs::read_to_string(repo.join("nvim/install.sh")).unwrap(),
        render_export("ember", "nvim", "install.sh.tmpl")
    );
}

#[test]
fn wrong_folder() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let noir = env.tmp.path().join("cosmic-noir-theme");
    fs::create_dir_all(&noir).unwrap();
    match run(&env, &ember, &noir) {
        Err(e @ ExportError::WrongFolder { .. }) => {
            let msg = e.to_string();
            assert!(msg.contains("cosmic-noir-theme"), "{msg}");
            assert!(msg.contains("cosmic-ember-theme"), "{msg}");
        }
        other => panic!("expected WrongFolder, got {other:?}"),
    }
    assert_eq!(fs::read_dir(&noir).unwrap().count(), 0);

    let named = env.tmp.path().join("cosmic-ember-theme");
    let report = run(&env, &ember, &named).unwrap();
    assert_eq!(report.repo, named);
    assert!(named.join("cosmic/Ember.ron").is_file());

    let fresh: PathBuf = env.tmp.path().join("not/yet");
    let report = run(&env, &ember, &fresh).unwrap();
    assert_eq!(report.repo, fresh.join("cosmic-ember-theme"));
    assert!(fresh.join("cosmic-ember-theme/cosmic/Ember.ron").is_file());
}

#[test]
fn render_error_writes_nothing() {
    let env = TestEnv::new();
    put_target(
        &env,
        "bad",
        &rice_target("bad", &["bad/file"]),
        &[("t.tmpl", "{{ nope }}\n")],
    );
    let parent = env.tmp.path().join("out");
    match run(&env, &palette("ember"), &parent) {
        Err(ExportError::Render { target, .. }) => assert_eq!(target, "bad"),
        other => panic!("expected Render, got {other:?}"),
    }
    assert!(!parent.join("cosmic-ember-theme").exists());
}

#[test]
fn noir_desktop_note() {
    let env = TestEnv::new();
    let repo = env.tmp.path().join("cosmic-noir-theme");
    copy_dir(&fixture("rices/cosmic-noir-theme"), &repo);
    let noir = import_rice(&repo).unwrap();
    let report = run(&env, &noir, &repo).unwrap();
    assert_eq!(status(&report, "cosmic/Noir.ron"), [&ExportStatus::Updated]);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.starts_with("cosmic/Noir.ron: rewritten from the palette")),
        "{:?}",
        report.notes
    );
    assert_eq!(
        status(&report, "cosmic-term/Noir-term.ron"),
        [&ExportStatus::Unchanged]
    );
    for line in &report.lines {
        assert!(
            !(line.path.starts_with("cosmic-term/Noir-term-")
                && line.status == ExportStatus::Stale),
            "{line:?}"
        );
    }
}
