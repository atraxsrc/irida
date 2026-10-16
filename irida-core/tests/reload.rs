mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use irida_core::reload::{ReloadStatus, parse_version, reload};
use irida_core::target::{MinVersion, Reload, SignalReload};

/// A spawned copy of `sleep` that is always killed and reaped on drop.
struct Spawned {
    child: Child,
    _dir: tempfile::TempDir,
}

impl Spawned {
    fn new(name: &str) -> Spawned {
        let dir = tempfile::TempDir::new().unwrap();
        let exe = dir.path().join(name);
        fs::copy("/bin/sleep", &exe).unwrap();
        let child = spawn_retrying(&exe);
        // Wait until the kernel reports the new name.
        let start = Instant::now();
        while irida_core::reload::find_processes(name).is_empty() {
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "process never appeared"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        Spawned { child, _dir: dir }
    }
}

/// Spawn `exe 30`, retrying while the copy is "Text file busy": a sibling test
/// thread can fork while our write handle is open, so the child briefly holds it.
fn spawn_retrying(exe: &Path) -> Child {
    let mut tries = 0;
    loop {
        match Command::new(exe).arg("30").spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 50 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            result => return result.unwrap(),
        }
    }
}

impl Drop for Spawned {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn fake_btop(version: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("btop");
    fs::write(
        &script,
        format!("#!/bin/sh\necho 'btop version: {version}'\n"),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    (tmp, bin)
}

fn term_reload(process: &str, min: Option<&str>) -> Reload {
    Reload {
        signal: Some(SignalReload {
            process: process.into(),
            signal: "TERM".into(),
        }),
        command: None,
        min_version: min.map(|v| MinVersion {
            command: "btop --version".into(),
            version: v.into(),
        }),
        hint: None,
    }
}

fn path_of(bin: &Path) -> Vec<PathBuf> {
    vec![bin.to_path_buf()]
}

#[test]
fn version_parsing() {
    assert_eq!(parse_version("btop version: 1.3.0"), Some((1, 3, 0)));
    assert_eq!(parse_version("1.4.6"), Some((1, 4, 6)));
    assert_eq!(parse_version("btop v1.4.6"), Some((1, 4, 6)));
    assert_eq!(parse_version("none"), None);
    assert_eq!(parse_version("btop 1.3 (gcc 13.2.0)"), None);
    assert_eq!(parse_version("2025.10 build 3.4.5"), None);
    assert_eq!(parse_version("v1.3.x 2.0.0"), None);
    assert_eq!(parse_version("1.2.3.4"), None);
}

#[test]
fn old_btop_is_not_signalled() {
    let (_tmp, bin) = fake_btop("1.3.0");
    let mut p = Spawned::new("iridaoldproc");
    let r = term_reload("iridaoldproc", Some("1.3.2"));
    assert_eq!(
        reload(Some(&r), "btop", &path_of(&bin)),
        ReloadStatus::Restart("restart btop to see it".into())
    );
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        p.child.try_wait().unwrap().is_none(),
        "process was signalled"
    );
}

#[test]
fn new_btop_is_signalled() {
    let (_tmp, bin) = fake_btop("1.3.2");
    let mut p = Spawned::new("iridanewproc");
    let r = term_reload("iridanewproc", Some("1.3.2"));
    assert_eq!(
        reload(Some(&r), "btop", &path_of(&bin)),
        ReloadStatus::Reloaded
    );
    let start = Instant::now();
    let status = loop {
        if let Some(s) = p.child.try_wait().unwrap() {
            break s;
        }
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "process did not exit"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.signal().is_some());
}

#[test]
fn not_running() {
    let r = term_reload("iridanoproc", None);
    assert_eq!(reload(Some(&r), "x", &[]), ReloadStatus::NotRunning);
}

#[test]
fn command_reload() {
    let ok = Reload {
        command: Some("true".into()),
        ..Reload::default()
    };
    assert_eq!(reload(Some(&ok), "x", &[]), ReloadStatus::Reloaded);
    let bad = Reload {
        command: Some("echo boom >&2; exit 3".into()),
        ..Reload::default()
    };
    match reload(Some(&bad), "x", &[]) {
        ReloadStatus::Failed(s) => assert!(s.contains("boom"), "{s}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn none_is_not_needed() {
    assert_eq!(reload(None, "x", &[]), ReloadStatus::NotNeeded);
}

#[test]
fn hint_only_is_restart() {
    let hint = Reload {
        hint: Some("open a new shell".into()),
        ..Reload::default()
    };
    assert_eq!(
        reload(Some(&hint), "x", &[]),
        ReloadStatus::Restart("open a new shell".into())
    );
    let with_command = Reload {
        command: Some("true".into()),
        ..hint
    };
    assert_eq!(
        reload(Some(&with_command), "x", &[]),
        ReloadStatus::Reloaded
    );
}
