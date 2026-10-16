mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use irida_core::write::{
    BlockError, atomic_write, resolve_write_path, restore_block, restore_key, set_block, set_key,
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
    let src = "a = 1\ncolor_theme = \"Default\"\nb = 2\n";
    let e = set_key(Some(src), "color_theme", "\"irida\"");
    assert_eq!(e.contents, "a = 1\ncolor_theme = \"irida\"\nb = 2\n");
    assert_eq!(e.before.as_deref(), Some("color_theme = \"Default\""));
    assert_eq!(
        restore_key(
            &e.contents,
            "color_theme",
            e.before.as_deref(),
            e.appended_newline
        ),
        src
    );

    let e = set_key(Some("a = 1"), "color_theme", "\"irida\"");
    assert_eq!(e.contents, "a = 1\ncolor_theme = \"irida\"\n");
    assert_eq!(restore_key(&e.contents, "color_theme", None, true), "a = 1");

    let e = set_key(None, "color_theme", "\"irida\"");
    assert_eq!(e.contents, "color_theme = \"irida\"\n");
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
