mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use irida_core::write::{
    BlockError, KeyOpts, KeyScope, atomic_write, resolve_write_path, restore_block, restore_key,
    set_block, set_key, sets_inline,
};

#[test]
fn block_append_then_replace_then_restore() {
    let first = set_block(Some("a\nb"), "#", "x\n").unwrap();
    assert_eq!(
        first.contents,
        "a\nb\n# >>> irida >>>\nx\n# <<< irida <<<\n"
    );
    assert!(first.appended_newline);
    assert_eq!(first.before, None);

    let second = set_block(Some(&first.contents), "#", "y\n").unwrap();
    assert_eq!(second.before, Some("x\n".to_string()));
    let back = restore_block(&second.contents, "#", Some("x\n"), false).unwrap();
    assert_eq!(back, first.contents);

    let orig = restore_block(&first.contents, "#", None, true).unwrap();
    assert_eq!(orig, "a\nb");
}

#[test]
fn block_keeps_user_edits() {
    let first = set_block(Some("a\n"), "#", "x\n").unwrap();
    let edited = format!("user\n{}", first.contents);
    let out = restore_block(&edited, "#", None, first.appended_newline).unwrap();
    assert_eq!(out, "user\na\n");
}

#[test]
fn block_restore_without_markers_is_a_no_op() {
    assert_eq!(restore_block("a\n", "#", None, true), Ok("a\n".to_string()));
}

#[test]
fn broken_markers() {
    for text in [
        "# >>> irida >>>\nx\n",
        "# <<< irida <<<\n# >>> irida >>>\n",
        "# >>> irida >>>\n# >>> irida >>>\n# <<< irida <<<\n",
    ] {
        assert_eq!(
            set_block(Some(text), "#", "x\n").unwrap_err(),
            BlockError::Broken
        );
    }
}

#[test]
fn block_on_missing_file() {
    let e = set_block(None, "#", "x\n").unwrap();
    assert_eq!(e.contents, "# >>> irida >>>\nx\n# <<< irida <<<\n");
    assert!(!e.appended_newline);
}

#[test]
fn key_replace_append_create_restore() {
    let o = KeyOpts::default();
    let src = "a = 1\ncolor_theme = \"Default\"\nb = 2\n";
    let e = set_key(Some(src), "color_theme", "\"irida\"", &o);
    assert_eq!(e.contents, "a = 1\ncolor_theme = \"irida\"\nb = 2\n");
    assert_eq!(e.before.as_deref(), Some("color_theme = \"Default\""));
    assert_eq!(
        restore_key(
            &e.contents,
            "color_theme",
            "color_theme = \"irida\"",
            e.before.as_deref(),
            e.appended_newline,
            false,
            &o
        ),
        src
    );

    let e = set_key(Some("a = 1"), "color_theme", "\"irida\"", &o);
    assert_eq!(e.contents, "a = 1\ncolor_theme = \"irida\"\n");
    assert_eq!(
        restore_key(
            &e.contents,
            "color_theme",
            "color_theme = \"irida\"",
            None,
            true,
            false,
            &o
        ),
        "a = 1"
    );

    let e = set_key(None, "color_theme", "\"irida\"", &o);
    assert_eq!(e.contents, "color_theme = \"irida\"\n");
}

fn sep(s: &str) -> KeyOpts {
    KeyOpts {
        separator: s.into(),
        ..KeyOpts::default()
    }
}

#[test]
fn separator_space_matches_and_writes() {
    let o = sep(" ");
    let e = set_key(
        Some("font_size 12\ninclude   old.conf\n"),
        "include",
        "irida.conf",
        &o,
    );
    assert_eq!(e.contents, "font_size 12\ninclude irida.conf\n");
    assert_eq!(e.before.as_deref(), Some("include   old.conf"));
}

#[test]
fn separator_equals_without_spaces() {
    let o = sep("=");
    let e = set_key(
        Some("--style=plain\n--theme=\"TwoDark\"\n"),
        "--theme",
        "irida",
        &o,
    );
    assert_eq!(e.contents, "--style=plain\n--theme=irida\n");
    let back = restore_key(
        &e.contents,
        "--theme",
        "--theme=irida",
        e.before.as_deref(),
        false,
        false,
        &o,
    );
    assert_eq!(back, "--style=plain\n--theme=\"TwoDark\"\n");
}

#[test]
fn separator_does_not_match_a_longer_key() {
    let o = sep(" ");
    let e = set_key(Some("includes x\n"), "include", "irida.conf", &o);
    assert_eq!(e.contents, "includes x\ninclude irida.conf\n");
    assert_eq!(e.before, None);
}

#[test]
fn default_opts_keep_old_behaviour() {
    let o = KeyOpts::default();
    assert_eq!(o.scope, KeyScope::End);
    assert!(!o.repeatable);
    let e = set_key(Some("a = 1\n"), "a", "2", &o);
    assert_eq!(e.contents, "a = 2\n");
}

#[test]
fn atomic_write_through_symlink() {
    let tmp = tempfile::TempDir::new().unwrap();
    let real = tmp.path().join("real.conf");
    let link = tmp.path().join("link.conf");
    fs::write(&real, "old").unwrap();
    symlink(&real, &link).unwrap();
    atomic_write(&link, b"new").unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&real).unwrap(), "new");
}

#[test]
fn atomic_write_through_dangling_symlink() {
    let tmp = tempfile::TempDir::new().unwrap();
    let link = tmp.path().join("link.conf");
    symlink("dotfiles/real.conf", &link).unwrap();
    atomic_write(&link, b"new").unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("dotfiles/real.conf")).unwrap(),
        "new"
    );
}

#[test]
fn symlink_loop_is_an_error() {
    let tmp = tempfile::TempDir::new().unwrap();
    let a = tmp.path().join("a");
    symlink("b", &a).unwrap();
    symlink("a", tmp.path().join("b")).unwrap();
    assert!(resolve_write_path(&a).is_err());
    assert!(atomic_write(&a, b"x").is_err());
}

#[test]
fn atomic_write_keeps_mode_and_cleans_up() {
    let tmp = tempfile::TempDir::new().unwrap();
    let f = tmp.path().join("sub/f.conf");
    atomic_write(&f, b"one").unwrap();
    assert_eq!(
        fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o644
    );
    fs::set_permissions(&f, fs::Permissions::from_mode(0o600)).unwrap();
    atomic_write(&f, b"two").unwrap();
    assert_eq!(
        fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_to_string(&f).unwrap(), "two");

    let leftovers: Vec<_> = fs::read_dir(f.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".irida-tmp"))
        .collect();
    assert!(leftovers.is_empty());

    let bad = f.join("child.conf");
    assert!(atomic_write(&bad, b"x").is_err());
    assert!(
        fs::read_dir(f.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .all(|e| !e.file_name().to_string_lossy().ends_with(".irida-tmp"))
    );
}

#[test]
fn planted_temp_symlink_is_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let victim = dir.path().join("victim.txt");
    fs::write(&victim, "keep me").unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    let f = repo.join("f.theme");
    fs::write(&f, "old").unwrap();
    // The fixed temporary name the old code used, planted as a link outside.
    symlink(&victim, repo.join(".f.theme.irida-tmp")).unwrap();

    atomic_write(&f, b"new").unwrap();

    assert_eq!(fs::read_to_string(&victim).unwrap(), "keep me");
    assert!(!fs::symlink_metadata(&f).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&f).unwrap(), "new");
}

fn top() -> KeyOpts {
    KeyOpts {
        scope: KeyScope::Top,
        ..KeyOpts::default()
    }
}

fn section(n: &str) -> KeyOpts {
    KeyOpts {
        scope: KeyScope::Section(n.into()),
        ..KeyOpts::default()
    }
}

#[test]
fn top_inserts_before_first_header() {
    let e = set_key(
        Some("# helix\n[editor]\nline-number = \"relative\"\n"),
        "theme",
        "\"irida\"",
        &top(),
    );
    assert_eq!(
        e.contents,
        "# helix\ntheme = \"irida\"\n[editor]\nline-number = \"relative\"\n"
    );
    assert_eq!(e.before, None);
    let back = restore_key(
        &e.contents,
        "theme",
        "theme = \"irida\"",
        None,
        false,
        false,
        &top(),
    );
    assert_eq!(back, "# helix\n[editor]\nline-number = \"relative\"\n");
}

#[test]
fn top_replaces_only_a_top_level_key() {
    let e = set_key(
        Some("theme = \"old\"\n[editor.x]\ntheme = \"inner\"\n"),
        "theme",
        "\"irida\"",
        &top(),
    );
    assert_eq!(
        e.contents,
        "theme = \"irida\"\n[editor.x]\ntheme = \"inner\"\n"
    );
    assert_eq!(e.before.as_deref(), Some("theme = \"old\""));
}

#[test]
fn top_appends_when_no_headers() {
    let e = set_key(Some("a = 1"), "theme", "\"irida\"", &top());
    assert_eq!(e.contents, "a = 1\ntheme = \"irida\"\n");
    assert!(e.appended_newline);
    assert_eq!(
        restore_key(
            &e.contents,
            "theme",
            "theme = \"irida\"",
            None,
            true,
            false,
            &top()
        ),
        "a = 1"
    );
}

#[test]
fn section_replaces_inside_its_section_only() {
    let text = "[font]\nsize = 12\n\n[general]\nimport = [\"a\"]\n\n[window]\nimport = [\"b\"]\n";
    let e = set_key(Some(text), "import", "[\"irida\"]", &section("general"));
    assert_eq!(
        e.contents,
        "[font]\nsize = 12\n\n[general]\nimport = [\"irida\"]\n\n[window]\nimport = [\"b\"]\n"
    );
    assert_eq!(e.before.as_deref(), Some("import = [\"a\"]"));
    assert_eq!(
        restore_key(
            &e.contents,
            "import",
            "import = [\"irida\"]",
            e.before.as_deref(),
            false,
            false,
            &section("general")
        ),
        text
    );
}

#[test]
fn section_appends_after_last_line_of_section() {
    let text = "[general]\nlive_config_reload = true\n\n[window]\nopacity = 1\n";
    let e = set_key(Some(text), "import", "[\"irida\"]", &section("general"));
    assert_eq!(
        e.contents,
        "[general]\nlive_config_reload = true\nimport = [\"irida\"]\n\n[window]\nopacity = 1\n"
    );
    assert!(!e.created_section);
    assert_eq!(
        restore_key(
            &e.contents,
            "import",
            "import = [\"irida\"]",
            None,
            false,
            false,
            &section("general")
        ),
        text
    );
}

#[test]
fn section_created_and_removed() {
    let text = "[window]\nopacity = 1\n";
    let e = set_key(Some(text), "import", "[\"irida\"]", &section("general"));
    assert_eq!(
        e.contents,
        "[window]\nopacity = 1\n[general]\nimport = [\"irida\"]\n"
    );
    assert!(e.created_section);
    assert_eq!(
        restore_key(
            &e.contents,
            "import",
            "import = [\"irida\"]",
            None,
            false,
            true,
            &section("general")
        ),
        text
    );
}

#[test]
fn created_section_kept_when_user_added_to_it() {
    let e = set_key(Some(""), "import", "[\"irida\"]", &section("general"));
    assert_eq!(e.contents, "[general]\nimport = [\"irida\"]\n");
    let edited = e.contents.replace(
        "import = [\"irida\"]\n",
        "import = [\"irida\"]\nworking_directory = \"/x\"\n",
    );
    assert_eq!(
        restore_key(
            &edited,
            "import",
            "import = [\"irida\"]",
            None,
            false,
            true,
            &section("general")
        ),
        "[general]\nworking_directory = \"/x\"\n"
    );
}

#[test]
fn header_match_is_exact() {
    let text = "[general.sub]\nimport = [\"a\"]\n";
    let e = set_key(Some(text), "import", "[\"irida\"]", &section("general"));
    assert_eq!(
        e.contents,
        "[general.sub]\nimport = [\"a\"]\n[general]\nimport = [\"irida\"]\n"
    );
}

#[test]
fn comments_only_and_empty_files() {
    let e = set_key(Some("# just a comment\n"), "theme", "\"irida\"", &top());
    assert_eq!(e.contents, "# just a comment\ntheme = \"irida\"\n");
    let e = set_key(None, "theme", "\"irida\"", &top());
    assert_eq!(e.contents, "theme = \"irida\"\n");
    assert_eq!(
        restore_key(
            &e.contents,
            "theme",
            "theme = \"irida\"",
            None,
            false,
            false,
            &top()
        ),
        ""
    );
}

fn rep() -> KeyOpts {
    KeyOpts {
        separator: " ".into(),
        repeatable: true,
        ..KeyOpts::default()
    }
}

#[test]
fn repeatable_keeps_other_values_and_adds_after_them() {
    let text = "include current-theme.conf\nfont_size 12\n";
    let e = set_key(Some(text), "include", "irida.conf", &rep());
    assert_eq!(
        e.contents,
        "include current-theme.conf\nfont_size 12\ninclude irida.conf\n"
    );
    assert_eq!(e.before, None);
    assert_eq!(
        restore_key(
            &e.contents,
            "include",
            "include irida.conf",
            None,
            false,
            false,
            &rep()
        ),
        text
    );
}

#[test]
fn repeatable_matches_its_own_value_only() {
    let text = "include a.conf\ninclude  irida.conf\n";
    let e = set_key(Some(text), "include", "irida.conf", &rep());
    assert_eq!(e.contents, "include a.conf\ninclude irida.conf\n");
    assert_eq!(e.before.as_deref(), Some("include  irida.conf"));
}

#[test]
fn repeatable_restore_leaves_a_user_line_with_same_key() {
    let out = restore_key(
        "include a.conf\n",
        "include",
        "include irida.conf",
        None,
        false,
        false,
        &rep(),
    );
    assert_eq!(out, "include a.conf\n");
}

#[test]
fn multi_line_value_is_replaced_whole_and_restored() {
    let text = "[general]\nimport = [\n  \"~/a.toml\",\n]\nlive_config_reload = true\n";
    let e = set_key(
        Some(text),
        "import",
        "[\"irida.toml\"]",
        &section("general"),
    );
    assert_eq!(
        e.contents,
        "[general]\nimport = [\"irida.toml\"]\nlive_config_reload = true\n"
    );
    assert_eq!(e.before.as_deref(), Some("import = [\n  \"~/a.toml\",\n]"));
    assert_eq!(
        restore_key(
            &e.contents,
            "import",
            "import = [\"irida.toml\"]",
            e.before.as_deref(),
            false,
            false,
            &section("general")
        ),
        text
    );
}

#[test]
fn header_with_comment_or_inner_spaces_is_a_header() {
    let text = "[ general ] # terminal\nlive_config_reload = true\n[font]\nsize = 12\n";
    let e = set_key(
        Some(text),
        "import",
        "[\"irida.toml\"]",
        &section("general"),
    );
    assert_eq!(
        e.contents,
        "[ general ] # terminal\nlive_config_reload = true\nimport = [\"irida.toml\"]\n[font]\nsize = 12\n"
    );
    let text = "[editor] # x\nline-number = \"relative\"\n";
    let e = set_key(Some(text), "theme", "\"irida\"", &top());
    assert_eq!(
        e.contents,
        "theme = \"irida\"\n[editor] # x\nline-number = \"relative\"\n"
    );
}

/// `restore_key` with the arguments a `set_key` edit gives it.
fn undo_key(e: &irida_core::write::KeyEdit, key: &str, written: &str, opts: &KeyOpts) -> String {
    restore_key(
        &e.contents,
        key,
        written,
        e.before.as_deref(),
        e.appended_newline,
        e.created_section,
        opts,
    )
}

#[test]
fn crlf_key_replace_keeps_line_endings() {
    let o = KeyOpts::default();
    let src = "a = 1\r\ntheme = \"dark\"\r\nb = 2\r\n";
    let e = set_key(Some(src), "theme", "\"irida\"", &o);
    assert_eq!(e.contents, "a = 1\r\ntheme = \"irida\"\r\nb = 2\r\n");
    assert_eq!(e.before.as_deref(), Some("theme = \"dark\""));
    assert_eq!(undo_key(&e, "theme", "theme = \"irida\"", &o), src);
}

#[test]
fn crlf_key_append_and_insert_use_crlf() {
    let o = KeyOpts::default();
    for src in ["a = 1\r\n", "a = 1\r\nb = 2"] {
        let e = set_key(Some(src), "theme", "\"irida\"", &o);
        assert!(
            e.contents.ends_with("\r\ntheme = \"irida\"\r\n"),
            "{:?}",
            e.contents
        );
        assert!(
            !e.contents.replace("\r\n", "").contains('\n'),
            "{:?}",
            e.contents
        );
        assert_eq!(undo_key(&e, "theme", "theme = \"irida\"", &o), src);
    }

    let src = "# helix\r\n[editor]\r\nline-number = \"relative\"\r\n";
    let e = set_key(Some(src), "theme", "\"irida\"", &top());
    assert_eq!(
        e.contents,
        "# helix\r\ntheme = \"irida\"\r\n[editor]\r\nline-number = \"relative\"\r\n"
    );
    assert_eq!(undo_key(&e, "theme", "theme = \"irida\"", &top()), src);

    let src = "[window]\r\nopacity = 1";
    let e = set_key(Some(src), "import", "[\"irida\"]", &section("general"));
    assert_eq!(
        e.contents,
        "[window]\r\nopacity = 1\r\n[general]\r\nimport = [\"irida\"]\r\n"
    );
    assert_eq!(
        undo_key(&e, "import", "import = [\"irida\"]", &section("general")),
        src
    );
}

#[test]
fn crlf_block_uses_crlf_and_restores() {
    let src = "a\r\nb";
    let e = set_block(Some(src), "#", "x\ny\n").unwrap();
    assert_eq!(
        e.contents,
        "a\r\nb\r\n# >>> irida >>>\r\nx\r\ny\r\n# <<< irida <<<\r\n"
    );
    assert_eq!(
        restore_block(&e.contents, "#", None, e.appended_newline).unwrap(),
        src
    );

    let again = set_block(Some(&e.contents), "#", "z\n").unwrap();
    assert_eq!(
        again.contents,
        "a\r\nb\r\n# >>> irida >>>\r\nz\r\n# <<< irida <<<\r\n"
    );
    assert_eq!(
        restore_block(&again.contents, "#", again.before.as_deref(), false).unwrap(),
        e.contents
    );
}

#[test]
fn lf_files_stay_lf() {
    let e = set_key(Some("a = 1\n"), "theme", "\"irida\"", &KeyOpts::default());
    assert_eq!(e.contents, "a = 1\ntheme = \"irida\"\n");
    let e = set_block(Some("a\n"), "#", "x\n").unwrap();
    assert!(!e.contents.contains('\r'));
}

#[test]
fn indented_key_line_keeps_its_indent() {
    let src = "[general]\n    import = [\"a.toml\"]\n\tlive_config_reload = true\n";
    let e = set_key(Some(src), "import", "[\"irida.toml\"]", &section("general"));
    assert_eq!(
        e.contents,
        "[general]\n    import = [\"irida.toml\"]\n\tlive_config_reload = true\n"
    );
    assert_eq!(e.before.as_deref(), Some("    import = [\"a.toml\"]"));
    assert_eq!(
        undo_key(
            &e,
            "import",
            "import = [\"irida.toml\"]",
            &section("general")
        ),
        src
    );

    // Applying again finds the indented line it wrote and leaves it as is.
    let again = set_key(
        Some(&e.contents),
        "import",
        "[\"irida.toml\"]",
        &section("general"),
    );
    assert_eq!(again.contents, e.contents);

    let src = "  include  a.conf\n  include  irida.conf\n";
    let e = set_key(Some(src), "include", "irida.conf", &rep());
    assert_eq!(e.contents, "  include  a.conf\n  include irida.conf\n");
}

#[test]
fn restore_key_leaves_a_line_the_user_changed() {
    let o = KeyOpts::default();
    let e = set_key(Some("a = 1\ntheme = old\n"), "theme", "irida", &o);
    let edited = e.contents.replace("theme = irida", "theme = mine");
    let out = restore_key(
        &edited,
        "theme",
        "theme = irida",
        e.before.as_deref(),
        e.appended_newline,
        false,
        &o,
    );
    assert_eq!(out, "a = 1\ntheme = mine\n");

    // An added line the user changed stays too.
    let e = set_key(Some("a = 1\n"), "theme", "irida", &o);
    let edited = e.contents.replace("theme = irida", "theme = mine");
    assert_eq!(
        restore_key(&edited, "theme", "theme = irida", None, false, false, &o),
        edited
    );
}

#[test]
fn restore_key_without_written_line_matches_any_value() {
    // Records from v0.1 do not say which line Irida wrote.
    let o = KeyOpts::default();
    let out = restore_key(
        "a = 1\ntheme = mine\n",
        "theme",
        "",
        Some("theme = old"),
        false,
        false,
        &o,
    );
    assert_eq!(out, "a = 1\ntheme = old\n");
}

#[test]
fn overrides_finds_keys_and_tables() {
    use irida_core::write::overrides;
    let names: Vec<String> = ["background", "palette", "[colors]"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let text = "theme = irida\n# background = #000000\npalette=0=#111111\n  background = #222222\n";
    assert_eq!(overrides(text, &names, " = "), ["background", "palette"]);
    let text = "[general]\nimport = []\n[ colors.primary ]\nbackground = \"#000000\"\n";
    assert_eq!(overrides(text, &names, " = "), ["background", "[colors]"]);
    assert!(overrides("[colorscheme]\nbackgrounds = 1\n", &names, " = ").is_empty());
}

#[test]
fn sets_inline_finds_a_table_set_before_any_header() {
    assert!(sets_inline("flavor = { dark = \"x\" }\n", "flavor"));
    assert!(sets_inline("# mine\nflavor.dark = \"x\"\n", "flavor"));
    assert!(sets_inline("  flavor={}\r\n[mgr]\n", "flavor"));
    assert!(!sets_inline("[flavor]\ndark = \"x\"\n", "flavor"));
    assert!(!sets_inline("flavors = 1\n", "flavor"));
    assert!(!sets_inline("[mgr]\nflavor = {}\n", "flavor"));
    assert!(!sets_inline("", "flavor"));
}
