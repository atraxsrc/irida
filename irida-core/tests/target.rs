mod common;

use std::path::PathBuf;

use common::TestEnv;
use irida_core::target::{
    Detect, LoadedTarget, NativeKind, Place, Source, TargetDef, TargetError, WriteKind,
    is_detected, load_targets,
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
        (
            "hintonly",
            file("key = \"k\"\nreplaced_hint = \"h\"", "key").replace("NAME", "hintonly"),
            true,
        ),
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
        display: None,
        note: None,
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

#[test]
fn display_defaults_to_folder_name() {
    let env = TestEnv::new();
    put(&env, "mine", "name = \"mine\"\n", &[]);
    let all = load_targets(&env.dirs);
    let t = find(&all, "mine").result.as_ref().unwrap();
    assert_eq!(t.def.display, None);
    assert_eq!(t.display(), "mine");
}

#[test]
fn display_is_read() {
    let env = TestEnv::new();
    put(&env, "mine", "name = \"mine\"\ndisplay = \"My App\"\n", &[]);
    let all = load_targets(&env.dirs);
    assert_eq!(
        find(&all, "mine").result.as_ref().unwrap().display(),
        "My App"
    );
}

#[test]
fn empty_display_rejected() {
    let env = TestEnv::new();
    put(&env, "mine", "name = \"mine\"\ndisplay = \" \"\n", &[]);
    let all = load_targets(&env.dirs);
    assert!(matches!(
        find(&all, "mine").result,
        Err(TargetError::Invalid(_))
    ));
}

#[test]
fn every_builtin_has_display() {
    let env = TestEnv::new();
    for t in load_targets(&env.dirs) {
        let target = t
            .result
            .as_ref()
            .unwrap_or_else(|e| panic!("{}: {e}", t.name));
        assert!(target.def.display.is_some(), "{} lacks display", t.name);
    }
}

#[test]
fn separator_only_on_key_writes() {
    let env = TestEnv::new();
    put(
        &env,
        "s",
        "name = \"s\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"file\"\ntemplate = \"a\"\nseparator = \" \"\n",
        &["a"],
    );
    assert!(matches!(
        find(&load_targets(&env.dirs), "s").result,
        Err(TargetError::Invalid(_))
    ));
    put(
        &env,
        "e",
        "name = \"e\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"key\"\nkey = \"k\"\ntemplate = \"a\"\nseparator = \"\"\n",
        &["a"],
    );
    assert!(matches!(
        find(&load_targets(&env.dirs), "e").result,
        Err(TargetError::Invalid(_))
    ));
    put(
        &env,
        "t",
        "name = \"t\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"key\"\nkey = \"include\"\ntemplate = \"a\"\nseparator = \" \"\n",
        &["a"],
    );
    let all = load_targets(&env.dirs);
    let t = find(&all, "t").result.as_ref().unwrap();
    assert_eq!(t.def.files[0].separator.as_deref(), Some(" "));
}

fn key_entry(extra: &str) -> String {
    format!(
        "name = \"p\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"key\"\nkey = \"k\"\ntemplate = \"a\"\n{extra}\n"
    )
}

#[test]
fn place_and_section_validation() {
    let env = TestEnv::new();
    for (bad, toml) in [
        ("middle", key_entry("place = \"middle\"")),
        ("both", key_entry("place = \"top\"\nsection = \"s\"")),
        ("bracket", key_entry("section = \"a]\"")),
        ("empty", key_entry("section = \"\"")),
        (
            "file",
            "name = \"p\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"file\"\ntemplate = \"a\"\nplace = \"top\"\n".to_string(),
        ),
    ] {
        put(&env, "p", &toml, &["a"]);
        // An unknown `place` value is a parse error, the rest are invalid.
        assert!(find(&load_targets(&env.dirs), "p").result.is_err(), "{bad}");
    }
    put(&env, "p", &key_entry("place = \"top\""), &["a"]);
    let all = load_targets(&env.dirs);
    let f = &find(&all, "p").result.as_ref().unwrap().def.files[0];
    assert_eq!(f.place, Some(Place::Top));
    put(&env, "p", &key_entry("section = \"general\""), &["a"]);
    let all = load_targets(&env.dirs);
    let f = &find(&all, "p").result.as_ref().unwrap().def.files[0];
    assert_eq!(f.section.as_deref(), Some("general"));
}

#[test]
fn note_overrides_only_on_key_writes() {
    let env = TestEnv::new();
    put(
        &env,
        "f",
        "name = \"f\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"file\"\ntemplate = \"a\"\nnote_overrides = [\"background\"]\n",
        &["a"],
    );
    put(
        &env,
        "k",
        "name = \"k\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"key\"\nkey = \"theme\"\ntemplate = \"a\"\nnote_overrides = [\"background\", \"[colors]\"]\n",
        &["a"],
    );
    let all = load_targets(&env.dirs);
    assert!(matches!(
        find(&all, "f").result,
        Err(TargetError::Invalid(_))
    ));
    let k = find(&all, "k").result.as_ref().unwrap();
    assert_eq!(k.def.files[0].note_overrides, ["background", "[colors]"]);
}

#[test]
fn builtin_names_are_the_target_folders() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("targets");
    let mut folders: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    folders.sort();
    assert_eq!(irida_core::target::builtin_names(), folders);
}

#[test]
fn file_write_sharing_a_path_is_rejected() {
    let env = TestEnv::new();
    put(
        &env,
        "f",
        "name = \"f\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"file\"\ntemplate = \"a\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"key\"\nkey = \"theme\"\ntemplate = \"a\"\n",
        &["a"],
    );
    let all = load_targets(&env.dirs);
    match &find(&all, "f").result {
        Err(TargetError::Invalid(msg)) => {
            assert!(msg.contains("several writes to"), "{msg}");
            assert!(msg.contains("~/.x"), "{msg}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn two_key_writes_to_one_path_load() {
    let env = TestEnv::new();
    put(
        &env,
        "k",
        "name = \"k\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"key\"\nkey = \"dark\"\ntemplate = \"a\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"key\"\nkey = \"light\"\ntemplate = \"a\"\n\n[[file]]\npath = \"~/.x\"\nwrite = \"block\"\ncomment = \"#\"\ntemplate = \"a\"\n",
        &["a"],
    );
    let all = load_targets(&env.dirs);
    let k = find(&all, "k").result.as_ref().unwrap();
    assert_eq!(k.def.files.len(), 3);
}
