//! Text output for the CLI.

use irida_core::apply::{ApplyReport, SkipReason, TargetResult, TargetStatus, UndoReport};
use irida_core::contrast::ContrastReport;
use irida_core::export::{BY_HAND, ExportReport, ExportStatus};
use irida_core::palette::Palette;
use irida_core::reload::ReloadStatus;

pub fn format_check(p: &Palette, r: &ContrastReport) -> String {
    let mut out = format!("{} ({})\n", p.name, p.mode);
    out.push_str("role     colour     bg      pane    min  verdict\n");
    for row in &r.rows {
        let bg = format!("{:.1}:1", row.on_background);
        let pane = format!("{:.1}:1", row.on_pane);
        let min = match row.min {
            Some(m) => format!("{m:.1}"),
            None => "-".to_string(),
        };
        let verdict = match (row.fails_on_background(), row.fails_on_pane()) {
            (false, false) => "",
            (true, false) => "FAIL on background",
            (false, true) => "FAIL on pane",
            (true, true) => "FAIL on both",
        };
        let line = format!(
            "{:<8} {}  {:>6}  {:>6}  {:<3}  {}",
            row.role.as_str(),
            row.color.to_hex(),
            bg,
            pane,
            min,
            verdict
        );
        out.push_str(line.trim_end());
        out.push('\n');
    }
    let n = r.failures().len();
    if n == 0 {
        out.push_str("all pass\n");
    } else {
        out.push_str(&format!("{n} failure(s)\n"));
    }
    out
}

/// One line per target in report order, each warning indented below it, then
/// the targets that are not installed folded into a single line.
pub fn format_targets(targets: &[TargetResult]) -> String {
    let mut out = String::new();
    let mut not_installed = Vec::new();
    for t in targets {
        let name = &t.display;
        let line = match &t.status {
            TargetStatus::Applied(ReloadStatus::Reloaded | ReloadStatus::NotNeeded) => {
                format!("{name}: applied")
            }
            TargetStatus::Applied(ReloadStatus::NotRunning) => {
                format!("{name}: applied, not running")
            }
            TargetStatus::Applied(ReloadStatus::Restart(msg)) => {
                format!("{name}: applied, {msg}")
            }
            TargetStatus::Applied(ReloadStatus::Failed(msg)) => {
                format!("{name}: applied, reload failed ({msg}); restart {name} to see it")
            }
            TargetStatus::Skipped(SkipReason::Disabled) => format!("{name}: skipped, turned off"),
            TargetStatus::Skipped(SkipReason::Broken(msg)) => format!("{name}: skipped, {msg}"),
            TargetStatus::Skipped(SkipReason::NotInstalled) => {
                not_installed.push(name.as_str());
                continue;
            }
        };
        out.push_str(&line);
        out.push('\n');
        for w in &t.warnings {
            out.push_str(&format!("  {w}\n"));
        }
    }
    if !not_installed.is_empty() {
        out.push_str(&format!("not installed: {}\n", not_installed.join(", ")));
    }
    out
}

pub fn format_apply(r: &ApplyReport) -> String {
    let mut out = format_targets(&r.targets);
    out.push_str("Undo with: irida undo\n");
    out
}

pub fn format_undo(r: &UndoReport) -> String {
    let mut out = format!("Restored apply {} ({})\n", r.record_id, r.palette);
    for (path, error) in &r.failures {
        out.push_str(&format!(
            "  could not restore {}: {error}\n",
            path.display()
        ));
    }
    out
}

pub fn format_export(r: &ExportReport, p: &Palette) -> String {
    let mut out = format!("Exported {} to {}\n", p.name, r.repo.display());
    for line in &r.lines {
        let (word, extra) = match &line.status {
            ExportStatus::Created => ("created", String::new()),
            ExportStatus::Updated => ("updated", String::new()),
            ExportStatus::Unchanged => ("unchanged", String::new()),
            ExportStatus::Kept => ("kept", String::new()),
            ExportStatus::Skipped(reason) => ("skipped", format!(": {reason}")),
            ExportStatus::Stale => ("stale?", " (not written by this export)".to_string()),
        };
        out.push_str(&format!("  {word:<10}{}{extra}\n", line.path));
    }
    for note in &r.notes {
        out.push_str(note);
        out.push('\n');
    }
    let rewritten = r
        .lines
        .iter()
        .filter(|l| l.status == ExportStatus::Updated)
        .count();
    if rewritten > 0 {
        out.push_str(&format!(
            "{rewritten} existing files were rewritten; review with git diff\n"
        ));
    }
    out.push_str(&format!("By hand: {BY_HAND}\n"));
    let failures = r.contrast.failures().len();
    if failures == 0 {
        out.push_str("Contrast: all pass\n");
        return out;
    }
    out.push_str(&format!("Contrast: {failures} failure(s)\n"));
    let check = format_check(p, &r.contrast);
    let mut lines = check.lines().skip(1);
    if let Some(header) = lines.next() {
        out.push_str(header);
        out.push('\n');
    }
    for (row, line) in r.contrast.rows.iter().zip(lines) {
        if !row.passes() {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}
