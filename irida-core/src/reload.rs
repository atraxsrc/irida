//! Ask a running program to pick up new colours, by signal or command.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::Command;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use crate::target::Reload;

#[derive(Debug, Clone, PartialEq)]
pub enum ReloadStatus {
    Reloaded,
    NotRunning,
    NotNeeded,
    Restart(String),
    Failed(String),
}

/// Parse a version out of program output, failing safe.
///
/// The text is split on whitespace. From each token trailing `,;:)` and a
/// leading `v`, `V` or `(` are stripped; the first token that then starts
/// with an ASCII digit is the version token. It must be exactly three
/// non-empty ASCII digit runs joined by single dots (each fitting a `u64`),
/// otherwise the result is `None`. Later tokens are never consulted.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let token = s.split_whitespace().find_map(|t| {
        let t = t.trim_end_matches([',', ';', ':', ')']);
        let t = t.strip_prefix(['v', 'V', '(']).unwrap_or(t);
        t.starts_with(|c: char| c.is_ascii_digit()).then_some(t)
    })?;
    let mut parts = token.split('.');
    let mut next = || -> Option<u64> {
        let p = parts.next()?;
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok()
    };
    let v = (next()?, next()?, next()?);
    parts.next().is_none().then_some(v)
}

/// Pids of this user's processes whose `comm` is exactly `name`.
pub fn find_processes(name: &str) -> Vec<i32> {
    let Ok(me) = fs::metadata("/proc/self").map(|m| m.uid()) else {
        return Vec::new();
    };
    let Ok(rd) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut pids: Vec<i32> = rd
        .filter_map(Result::ok)
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let comm = fs::read_to_string(e.path().join("comm")).ok()?;
            if comm.trim_end_matches('\n') != name {
                return None;
            }
            (fs::metadata(e.path()).ok()?.uid() == me).then_some(pid)
        })
        .collect();
    pids.sort_unstable();
    pids
}

/// Signal names a target may ask for.
pub const SIGNAL_NAMES: [&str; 4] = ["USR1", "USR2", "HUP", "TERM"];

fn signal_by_name(name: &str) -> Option<Signal> {
    match name {
        "USR1" => Some(Signal::SIGUSR1),
        "USR2" => Some(Signal::SIGUSR2),
        "HUP" => Some(Signal::SIGHUP),
        "TERM" => Some(Signal::SIGTERM),
        _ => None,
    }
}

fn run_shell(command: &str, path: &[PathBuf]) -> std::io::Result<std::process::Output> {
    let joined: OsString = std::env::join_paths(path).unwrap_or_default();
    Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .env("PATH", joined)
        .output()
}

fn version_ok(command: &str, minimum: &str, path: &[PathBuf]) -> bool {
    let Some(min) = parse_version(minimum) else {
        return false;
    };
    let Ok(out) = run_shell(command, path) else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    parse_version(&text).is_some_and(|v| v >= min)
}

pub fn reload(r: Option<&Reload>, target: &str, path: &[PathBuf]) -> ReloadStatus {
    let Some(r) = r else {
        return ReloadStatus::NotNeeded;
    };
    if r.signal.is_none()
        && r.command.is_none()
        && let Some(hint) = &r.hint
    {
        return ReloadStatus::Restart(hint.clone());
    }
    if let Some(min) = &r.min_version
        && !version_ok(&min.command, &min.version, path)
    {
        return ReloadStatus::Restart(format!("restart {target} to see it"));
    }
    if let Some(sig) = &r.signal {
        let Some(signal) = signal_by_name(&sig.signal) else {
            return ReloadStatus::Failed(format!("unknown signal {}", sig.signal));
        };
        let pids = find_processes(&sig.process);
        if pids.is_empty() {
            return ReloadStatus::NotRunning;
        }
        for pid in pids {
            if let Err(e) = kill(Pid::from_raw(pid), signal) {
                return ReloadStatus::Failed(format!("signal {pid}: {e}"));
            }
        }
    }
    if let Some(command) = &r.command {
        match run_shell(command, path) {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                return ReloadStatus::Failed(
                    String::from_utf8_lossy(&out.stderr).trim().to_string(),
                );
            }
            Err(e) => return ReloadStatus::Failed(e.to_string()),
        }
    }
    ReloadStatus::Reloaded
}
