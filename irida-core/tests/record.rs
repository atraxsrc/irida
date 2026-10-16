mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use common::TestEnv;
use irida_core::dirs::Dirs;
use irida_core::record::{self, Entry, RecordWriter, RestoreMode};
use irida_core::write::{atomic_write, set_block, set_key};

fn read_opt(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Capture `path`, apply a block to it and record the matching entry.
fn apply_block(w: &mut RecordWriter, path: &Path, body: &str) {
    let blob = w.capture(path).unwrap();
    let edit = set_block(read_opt(path).as_deref(), "#", body).unwrap();
    atomic_write(path, edit.contents.as_bytes()).unwrap();
    w.push(Entry::Block {
        path: path.to_path_buf(),
        blob,
        comment: "#".to_string(),
        before: edit.before,
        appended_newline: edit.appended_newline,
    });
}

/// Capture `path`, set a key in it and record the matching entry.
fn apply_key(w: &mut RecordWriter, path: &Path, key: &str, value: &str) {
    let blob = w.capture(path).unwrap();
    let edit = set_key(read_opt(path).as_deref(), key, value);
    atomic_write(path, edit.contents.as_bytes()).unwrap();
    w.push(Entry::Key {
        path: path.to_path_buf(),
        blob,
        key: key.to_string(),
        before: edit.before,
        appended_newline: edit.appended_newline,
    });
}

fn append(path: &Path, extra: &str) {
    let text = fs::read_to_string(path).unwrap();
    fs::write(path, text + extra).unwrap();
}

fn record_ids(dirs: &Dirs) -> Vec<u32> {
    let mut ids: Vec<u32> = fs::read_dir(dirs.state.join("irida/applies"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap().parse().unwrap())
        .collect();
    ids.sort();
    ids
}

#[test]
fn file_round_trip() {
    let env = TestEnv::new();
    let kept = env.write("a.conf", "old a\n");
    let made = env.dirs.home.join("sub/b.conf");

    let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
    let blob_a = w.capture(&kept).unwrap();
    let blob_b = w.capture(&made).unwrap();
    assert!(blob_a.is_some());
    assert_eq!(blob_b, None);
    w.push(Entry::File { path: kept.clone(), blob: blob_a });
    w.push(Entry::File { path: made.clone(), blob: blob_b });
    atomic_write(&kept, b"new a\n").unwrap();
    atomic_write(&made, b"new b\n").unwrap();
    let rec = w.finish().unwrap();

    let failures = record::restore(&env.dirs, &rec, RestoreMode::Undo);
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(fs::read_to_string(&kept).unwrap(), "old a\n");
    assert!(!made.exists());
}

#[test]
fn capture_under_a_file_is_absent() {
    let env = TestEnv::new();
    let plain = env.write("plain", "x\n");
    let under = plain.join("x");

    let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
    let blob = w.capture(&under).unwrap();
    assert_eq!(blob, None);
    w.push(Entry::File { path: under, blob });
    let rec = w.finish().unwrap();

    for mode in [RestoreMode::Rollback, RestoreMode::Undo] {
        let failures = record::restore(&env.dirs, &rec, mode);
        assert!(failures.is_empty(), "{failures:?}");
    }
    assert_eq!(fs::read_to_string(&plain).unwrap(), "x\n");
}

#[test]
fn block_and_key_undo_keep_user_edits() {
    let env = TestEnv::new();
    let zshrc = env.write(".zshrc", "export A=1\n");
    let btop = env.write(".config/btop/btop.conf", "color_theme = \"Default\"\nrounded = True\n");

    let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
    apply_block(&mut w, &zshrc, "export IRIDA=1\n");
    apply_key(&mut w, &btop, "color_theme", "\"irida\"");
    let rec = w.finish().unwrap();

    append(&zshrc, "# mine\n");
    append(&btop, "# mine\n");

    let failures = record::restore(&env.dirs, &rec, RestoreMode::Undo);
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(fs::read_to_string(&zshrc).unwrap(), "export A=1\n# mine\n");
    assert_eq!(
        fs::read_to_string(&btop).unwrap(),
        "color_theme = \"Default\"\nrounded = True\n# mine\n"
    );
}

#[test]
fn rollback_is_byte_exact() {
    let env = TestEnv::new();
    let zshrc = env.write(".zshrc", "export A=1");
    let btop = env.write(".config/btop/btop.conf", "rounded = True");

    let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
    apply_block(&mut w, &zshrc, "export IRIDA=1\n");
    apply_key(&mut w, &btop, "color_theme", "\"irida\"");
    let rec = w.finish().unwrap();
    assert_ne!(fs::read_to_string(&zshrc).unwrap(), "export A=1");

    let failures = record::restore(&env.dirs, &rec, RestoreMode::Rollback);
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(fs::read(&zshrc).unwrap(), b"export A=1");
    assert_eq!(fs::read(&btop).unwrap(), b"rounded = True");
}

#[test]
fn created_file_is_deleted() {
    let env = TestEnv::new();
    let path = env.dirs.home.join(".config/foot/irida.ini");

    let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
    apply_block(&mut w, &path, "[colors]\n");
    let rec = w.finish().unwrap();
    assert!(path.exists());

    let failures = record::restore(&env.dirs, &rec, RestoreMode::Undo);
    assert!(failures.is_empty(), "{failures:?}");
    assert!(!path.exists());
}

#[test]
fn restore_through_symlink() {
    let env = TestEnv::new();
    let real = env.write("dotfiles/zshrc", "export A=1\n");
    let link = env.dirs.home.join(".zshrc");
    symlink("dotfiles/zshrc", &link).unwrap();

    let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
    apply_block(&mut w, &link, "export IRIDA=1\n");
    let rec = w.finish().unwrap();
    assert_ne!(fs::read_to_string(&real).unwrap(), "export A=1\n");

    let failures = record::restore(&env.dirs, &rec, RestoreMode::Undo);
    assert!(failures.is_empty(), "{failures:?}");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&real).unwrap(), "export A=1\n");

    let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
    let blob = w.capture(&link).unwrap();
    w.push(Entry::File { path: link.clone(), blob });
    atomic_write(&link, b"replaced\n").unwrap();
    let rec = w.finish().unwrap();
    assert_eq!(fs::read_to_string(&real).unwrap(), "replaced\n");

    let failures = record::restore(&env.dirs, &rec, RestoreMode::Rollback);
    assert!(failures.is_empty(), "{failures:?}");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&real).unwrap(), "export A=1\n");
}

#[test]
fn numbering_latest_prune() {
    let env = TestEnv::new();
    assert_eq!(record::latest(&env.dirs).unwrap(), None);
    for n in 1..=12u32 {
        let rec = RecordWriter::create(&env.dirs, "ember").unwrap().finish().unwrap();
        assert_eq!(rec.id, n);
    }
    assert_eq!(record::latest(&env.dirs).unwrap().unwrap().id, 12);

    record::prune(&env.dirs, 10).unwrap();
    assert_eq!(record_ids(&env.dirs), (3..=12).collect::<Vec<u32>>());

    record::delete(&env.dirs, 12).unwrap();
    assert_eq!(record::latest(&env.dirs).unwrap().unwrap().id, 11);
}

#[test]
fn record_toml_round_trips() {
    let env = TestEnv::new();
    let a = env.write("a.conf", "a\n");
    let zshrc = env.write(".zshrc", "export A=1");
    let btop = env.dirs.home.join(".config/btop/btop.conf");

    let mut w = RecordWriter::create(&env.dirs, "noir").unwrap();
    let blob = w.capture(&a).unwrap();
    w.push(Entry::File { path: a.clone(), blob });
    w.push(Entry::File { path: PathBuf::from("/nowhere/x"), blob: None });
    apply_block(&mut w, &zshrc, "x\n");
    apply_block(&mut w, &zshrc, "y\n");
    apply_key(&mut w, &btop, "color_theme", "\"irida\"");
    apply_key(&mut w, &btop, "color_theme", "\"other\"");
    let rec = w.finish().unwrap();

    assert_eq!(rec.palette, "noir");
    assert_eq!(rec.entries.len(), 6);
    assert_eq!(record::latest(&env.dirs).unwrap(), Some(rec));
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn records_are_private() {
    let env = TestEnv::new();
    let a = env.write("a.conf", "secret\n");

    let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
    let blob = w.capture(&a).unwrap().unwrap();
    w.push(Entry::File { path: a, blob: Some(blob.clone()) });
    let rec = w.finish().unwrap();

    let applies = env.dirs.state.join("irida/applies");
    let dir = applies.join(rec.id.to_string());
    assert_eq!(mode(&applies), 0o700);
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join("files")), 0o700);
    assert_eq!(mode(&dir.join(&blob)), 0o600);
    assert_eq!(mode(&dir.join("record.toml")), 0o600);
}

#[test]
fn dangling_symlink_round_trip() {
    let env = TestEnv::new();
    let link = env.dirs.home.join(".zshrc");
    let real = env.dirs.home.join("dotfiles/zshrc");
    fs::create_dir_all(&env.dirs.home).unwrap();
    symlink("dotfiles/zshrc", &link).unwrap();

    for (mode, as_file) in [
        (RestoreMode::Undo, false),
        (RestoreMode::Rollback, false),
        (RestoreMode::Undo, true),
        (RestoreMode::Rollback, true),
    ] {
        let mut w = RecordWriter::create(&env.dirs, "ember").unwrap();
        if as_file {
            let blob = w.capture(&link).unwrap();
            assert_eq!(blob, None);
            w.push(Entry::File { path: link.clone(), blob });
            atomic_write(&link, b"written\n").unwrap();
        } else {
            apply_block(&mut w, &link, "export IRIDA=1\n");
        }
        let rec = w.finish().unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(real.is_file());

        let failures = record::restore(&env.dirs, &rec, mode);
        assert!(failures.is_empty(), "{failures:?}");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&link).unwrap(), Path::new("dotfiles/zshrc"));
        assert!(!real.exists());
    }
}

#[test]
fn abort_removes_only_its_folder() {
    let env = TestEnv::new();
    let file = env.write("a.conf", "old a\n");
    let applies = env.dirs.state.join("irida/applies");

    let mut first = RecordWriter::create(&env.dirs, "ember").unwrap();
    let blob = first.capture(&file).unwrap();
    first.push(Entry::File { path: file.clone(), blob: blob.clone() });
    let rec = first.finish().unwrap();
    assert_eq!(rec.id, 1);

    let mut second = RecordWriter::create(&env.dirs, "noir").unwrap();
    assert_eq!(second.id(), 2);
    assert!(second.capture(&file).unwrap().is_some());
    assert!(applies.join("2/files").is_dir());
    second.abort().unwrap();

    assert_eq!(record_ids(&env.dirs), [1]);
    assert!(!applies.join("2").exists());
    assert_eq!(record::latest(&env.dirs).unwrap(), Some(rec));
    let blob = applies.join("1").join(blob.unwrap());
    assert_eq!(fs::read_to_string(blob).unwrap(), "old a\n");
}
