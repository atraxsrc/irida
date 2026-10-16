//! Text output for the CLI.

use irida_core::contrast::ContrastReport;
use irida_core::palette::Palette;

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
