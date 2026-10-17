mod common;

use std::path::PathBuf;

use common::TestEnv;
use irida_core::target::{
    Detect, LoadedTarget, NativeKind, Source, TargetDef, TargetError, WriteKind, is_detected,
    load_targets,
};

const CAVA: &str = r##"name = "cava"
detect = { binary = "cava", config = "~/.config/cava" }

[[file]]
path = "~/.config/cava/config"
write = "block"
template = "cava.tmpl"
comment = "#"

[reload]
signal = { process = "cava", signal = "USR2" }

[[rice]]
path = "cava/config.colors"
template = "cava.tmpl"
"##;

fn put(env: &TestEnv, folder: &str, toml: &str, templates: &[&str]) {
    let dir = env.dirs.config.join("irida/targets").join(folder);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("target.toml"), toml).unwrap();
    for t in templates {
        std::fs::write(dir.join(t), "x\n").unwrap();
    }
}

fn find<'a>(all: &'a [LoadedTarget], name: &str) -> &'a LoadedTarget {
    all.iter().find(|t| t.name == name).expect("target listed")
}

#[test]
fn user_target_loads() {
    let env = TestEnv::new();
    put(&env, "cava", CAVA, &["cava.tmpl"]);
    let all = load_targets(&env.dirs);
    let t = find(&all, "cava").result.as_ref().expect("loads");
    assert!(matches!(t.source, Source::User(_)));
    assert_eq!(t.def.files.len(), 1);
    assert_eq!(t.def.files[0].write, WriteKind::Block);
    assert_eq!(t.def.files[0].comment.as_deref(), Some("#"));
    assert!(!t.def.live);
    assert!(t.templates.contains_key("cava.tmpl"));
}

#[test]
fn broken_target_is_reported_not_fatal() {
    let env = TestEnv::new();
    put(&env, "cava", CAVA, &["cava.tmpl"]);
    put(&env, "x", "name = \"x\"\nbogus = 1", &[]);
    let all = load_targets(&env.dirs);
    assert!(matches!(find(&all, "x").result, Err(TargetError::Parse(_))));
    assert!(find(&all, "cava").result.is_ok());
}

#[test]
fn validation_errors() {
    let env = TestEnv::new();
    let file = |extra: &str, write: &str| {
        format!(
            "name = \"NAME\"\n[[file]]\npath = \"~/a\"\nwrite = \"{write}\"\ntemplate = \"a.tmpl\"\n{extra}\n"
        )
    };
    let cases: Vec<(&str, String, bool)> = vec![
        (
            "nocomment",
            file("", "block").replace("NAME", "nocomment"),
            true,
        ),
        ("nokey", file("", "key").replace("NAME", "nokey"), true),
        ("notmpl", file("", "file").replace("NAME", "notmpl"), false),
        (
            "native",
            "name = \"native\"\nnative = \"cosmic-term\"\n".to_string(),
            true,
        ),
        ("folder", file("", "file").replace("NAME", "other"), true),
    ];
    for (folder, toml, with_tmpl) in &cases {
        put(
            &env,
            folder,
            toml,
            if *with_tmpl { &["a.tmpl"] } else { &[] },
        );
    }
    let all = load_targets(&env.dirs);
    for (folder, _, _) in &cases {
        assert!(
            matches!(find(&all, folder).result, Err(TargetError::Invalid(_))),
            "{folder}: {:?}",
            find(&all, folder).result.as_ref().err()
        );
    }
}

#[test]
fn builtin_cosmic_term_and_user_override() {
    let env = TestEnv::new();
    let all = load_targets(&env.dirs);
    let t = find(&all, "cosmic-term")
        .result
        .as_ref()
        .expect("built-in loads");
    assert_eq!(t.source, Source::Builtin);
    assert_eq!(t.def.native, Some(NativeKind::CosmicTerm));
    assert!(t.def.live);

    let user = "name = \"cosmic-term\"\n\n[[file]]\npath = \"~/.config/x\"\nwrite = \"file\"\ntemplate = \"a.tmpl\"\n";
    put(&env, "cosmic-term", user, &["a.tmpl"]);
    let all = load_targets(&env.dirs);
    let t = find(&all, "cosmic-term")
        .result
        .as_ref()
        .expect("user target loads");
    assert!(matches!(t.source, Source::User(_)));
    assert_eq!(t.def.native, None);
    assert_eq!(t.def.files.len(), 1);
}

#[test]
fn detection() {
    let env = TestEnv::new();
    let bin = env.tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("mytool");
    std::fs::write(&exe, "#!/bin/sh\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path: Vec<PathBuf> = vec![bin];

    let by_bin = TargetDef {
        detect: Detect {
            binary: Some("mytool".into()),
            ..Default::default()
        },
        ..def()
    };
    assert!(is_detected(&by_bin, &env.dirs, &path));

    let by_cfg = TargetDef {
        detect: Detect {
            config: Some("~/.config/nothere".into()),
            ..Default::default()
        },
        ..def()
    };
    assert!(!is_detected(&by_cfg, &env.dirs, &[]));
    std::fs::create_dir_all(env.dirs.config.join("nothere")).unwrap();
    assert!(is_detected(&by_cfg, &env.dirs, &[]));

    let by_flat = TargetDef {
        detect: Detect {
            flatpak: Some("io.github.x".into()),
            ..Default::default()
        },
        ..def()
    };
    assert!(!is_detected(&by_flat, &env.dirs, &[]));
    std::fs::create_dir_all(env.dirs.home.join(".var/app/io.github.x/config")).unwrap();
    assert!(is_detected(&by_flat, &env.dirs, &[]));

    let none = TargetDef {
        detect: Detect {
            binary: Some("absent".into()),
            ..Default::default()
        },
        ..def()
    };
    assert!(!is_detected(&none, &env.dirs, &path));
    assert!(is_detected(&def(), &env.dirs, &[]));
}

fn def() -> TargetDef {
    TargetDef {
        name: "t".into(),
        detect: Detect::default(),
        native: None,
        files: vec![],
        reload: None,
        rice: vec![],
        live: false,
    }
}

#[test]
fn rice_entries() {
    let env = TestEnv::new();
    let rice = |extra: &str| format!("[[rice]]\npath = \"x/{{{{ stem }}}}.y\"\n{extra}\n");
    put(
        &env,
        "full",
        &format!(
            "name = \"full\"\n{}",
            rice("template = \"t.tmpl\"\nexecutable = true")
        ),
        &["t.tmpl"],
    );
    put(
        &env,
        "notmpl",
        &format!("name = \"notmpl\"\n{}", rice("")),
        &[],
    );
    put(
        &env,
        "install",
        &format!(
            "name = \"install\"\n{}",
            rice("template = \"t.tmpl\"\ninstall = \"file\"")
        ),
        &["t.tmpl"],
    );
    put(
        &env,
        "missing",
        &format!("name = \"missing\"\n{}", rice("template = \"gone.tmpl\"")),
        &[],
    );
    let all = load_targets(&env.dirs);
    let t = find(&all, "full").result.as_ref().expect("loads");
    assert_eq!(t.def.rice.len(), 1);
    assert_eq!(t.def.rice[0].path, "x/{{ stem }}.y");
    assert_eq!(t.def.rice[0].template, "t.tmpl");
    assert!(t.def.rice[0].executable);
    assert!(matches!(
        find(&all, "notmpl").result,
        Err(TargetError::Parse(_))
    ));
    assert!(matches!(
        find(&all, "install").result,
        Err(TargetError::Parse(_))
    ));
    assert!(find(&all, "missing").result.is_ok());
}

#[test]
fn create_only_needs_file() {
    let env = TestEnv::new();
    let toml = |name: &str, write: &str| {
        format!(
            "name = \"{name}\"\n[[file]]\npath = \"~/a\"\nwrite = \"{write}\"\ntemplate = \"a.tmpl\"\ncomment = \"#\"\ncreate_only = true\n"
        )
    };
    put(&env, "blk", &toml("blk", "block"), &["a.tmpl"]);
    put(&env, "fil", &toml("fil", "file"), &["a.tmpl"]);
    let all = load_targets(&env.dirs);
    assert!(matches!(
        find(&all, "blk").result,
        Err(TargetError::Invalid(_))
    ));
    let t = find(&all, "fil").result.as_ref().expect("loads");
    assert!(t.def.files[0].create_only);
}

#[test]
fn reload_hint_parses() {
    let env = TestEnv::new();
    put(
        &env,
        "h",
        "name = \"h\"\n[reload]\nhint = \"open a new shell\"\n",
        &[],
    );
    let all = load_targets(&env.dirs);
    let t = find(&all, "h").result.as_ref().expect("loads");
    assert_eq!(
        t.def.reload.as_ref().unwrap().hint.as_deref(),
        Some("open a new shell")
    );
}

#[test]
fn config_file_detects() {
    let env = TestEnv::new();
    let zsh = TargetDef {
        detect: Detect {
            config: Some("~/.zshrc".into()),
            ..Default::default()
        },
        ..def()
    };
    assert!(!is_detected(&zsh, &env.dirs, &[]));
    env.write(".zshrc", "");
    assert!(is_detected(&zsh, &env.dirs, &[]));
}

#[test]
fn reload_typos_and_unknown_signals_are_rejected() {
    let env = TestEnv::new();
    put(
        &env,
        "typo",
        "name = \"typo\"\n[reload]\nsingal = { process = \"x\", signal = \"USR1\" }\n",
        &[],
    );
    put(
        &env,
        "kill",
        "name = \"kill\"\n[reload]\nsignal = { process = \"x\", signal = \"KILL\" }\n",
        &[],
    );
    put(
        &env,
        "fine",
        "name = \"fine\"\n[reload]\nsignal = { process = \"x\", signal = \"HUP\" }\n",
        &[],
    );
    let all = load_targets(&env.dirs);
    assert!(matches!(
        find(&all, "typo").result,
        Err(TargetError::Parse(_))
    ));
    assert!(matches!(
        find(&all, "kill").result,
        Err(TargetError::Invalid(_))
    ));
    assert!(find(&all, "fine").result.is_ok());
}
