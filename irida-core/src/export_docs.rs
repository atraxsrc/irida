//! The README, showcase and `switch.sh` of an exported rice repo.

use std::collections::BTreeMap;

use crate::contrast::{self, RoleContrast};
use crate::palette::Palette;
use crate::role::Role;
use crate::slot::{PinValue, Slot};

pub const README_BEGIN: &str = "<!-- >>> irida palette >>> -->";
pub const README_END: &str = "<!-- <<< irida palette <<< -->";
pub const SHOWCASE_BEGIN: &str = "<!-- >>> irida contrast >>> -->";
pub const SHOWCASE_END: &str = "<!-- <<< irida contrast <<< -->";

/// Every rice's slug, for the LazyVim specs `switch.sh` parks.
const RICES: &str =
    "darkgold whitegold tokyonight tokyoday ember emberday monochrome noir dossier harbordark";

fn capitalised(role: Role) -> String {
    let s = role.as_str();
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// The role's display name, or its capitalised role name.
pub(crate) fn name_of(p: &Palette, role: Role) -> String {
    p.names
        .get(&role)
        .cloned()
        .unwrap_or_else(|| capitalised(role))
}

fn hex(p: &Palette, role: Role) -> String {
    p.roles.get(role).to_hex()
}

/// `\`, `|` and `/` escaped with a backslash, so a name keeps to its table
/// cell and the ` / ` between the container and pane names stays the only one.
fn escape_cell(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '|' | '/') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// [`escape_cell`] undone: every backslash keeps only the character after it.
fn unescape_cell(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    out
}

/// A table row's cells, split on the pipes no backslash escapes.
fn split_cells(line: &str) -> Vec<&str> {
    let mut cells = Vec::new();
    let mut start = 0;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '|' => {
                cells.push(line[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    cells.push(line[start..].trim());
    cells
}

/// How the terminal pane is drawn: `transparent` or `opaque`.
pub(crate) fn opacity(p: &Palette) -> &'static str {
    if p.pins.get(Slot::Background) == Some(PinValue::None) {
        "transparent"
    } else {
        "opaque"
    }
}

fn swatch(hex: &str) -> String {
    let lower = hex.trim_start_matches('#').to_lowercase();
    format!("![{hex}](https://placehold.co/12x12/{lower}/{lower}.png)")
}

/// The README's palette block: notes, the role table, the hint and the pair table.
pub fn readme_block(p: &Palette, pair: Option<&Palette>) -> String {
    let mut out = String::new();
    if let Some(notes) = &p.notes {
        out.push_str(notes.trim_end());
        out.push_str("\n\n");
    }
    out.push_str("| Role | Color | Hex |\n|------|-------|-----|\n");
    let row = |label: &str, swatch_hex: &str, name: &str, hexes: &str| {
        format!("| {label} | {} {name} | {hexes} |\n", swatch(swatch_hex))
    };
    let bg = hex(p, Role::Background);
    let pane = hex(p, Role::Pane);
    let container = hex(p, Role::Container);
    out.push_str(&row(
        "Background",
        &bg,
        &escape_cell(&name_of(p, Role::Background)),
        &format!("`{bg}`"),
    ));
    out.push_str(&row(
        "Containers",
        &container,
        &format!(
            "{} / {}",
            escape_cell(&name_of(p, Role::Container)),
            escape_cell(&name_of(p, Role::Pane))
        ),
        &format!("`{container}` / `{pane}`"),
    ));
    out.push_str(&row(
        "Terminal",
        &pane,
        &format!("{}, {}", escape_cell(&name_of(p, Role::Pane)), opacity(p)),
        &format!("`{pane}`"),
    ));
    for role in [
        Role::Text,
        Role::Accent,
        Role::Success,
        Role::Warning,
        Role::Error,
        Role::Blue,
        Role::Magenta,
        Role::Cyan,
        Role::Muted,
        Role::Comment,
    ] {
        let h = hex(p, role);
        out.push_str(&row(
            &capitalised(role),
            &h,
            &escape_cell(&name_of(p, role)),
            &format!("`{h}`"),
        ));
    }
    out.push_str(&format!(
        "\nWindow hint `{}`. Ratios per role are in `docs/showcase.md`, or run\n`irida check .`.\n",
        hex(p, Role::Accent)
    ));
    if let Some(q) = pair {
        out.push_str(&format!("\n| Role | {} | {} |\n|------|", p.name, q.name));
        for n in [&p.name, &q.name] {
            out.push_str(&"-".repeat(n.len() + 2));
            out.push('|');
        }
        out.push('\n');
        for role in [Role::Background, Role::Text, Role::Accent, Role::Success] {
            out.push_str(&format!(
                "| {} | `{}` | `{}` |\n",
                capitalised(role),
                hex(p, role),
                hex(q, role)
            ));
        }
    }
    out
}

/// The display name in a role table Color cell: the text after the swatch
/// image. `None` when the cell does not start with a swatch.
fn swatch_name(cell: &str) -> Option<&str> {
    let rest = cell.strip_prefix("![#")?;
    let (_, after) = rest.split_once("](")?;
    let (_, name) = after.split_once(')')?;
    Some(name.trim())
}

/// The display names in a README's palette block, read back from its role
/// table and unescaped. Names equal to the capitalised role, and empty ones,
/// are left out. No markers or a malformed table gives an empty map.
pub fn readme_names(readme: &str) -> BTreeMap<Role, String> {
    let lines: Vec<&str> = readme.lines().collect();
    let Some(begin) = lines.iter().position(|l| l.trim_end() == README_BEGIN) else {
        return BTreeMap::new();
    };
    let Some(len) = lines[begin + 1..]
        .iter()
        .position(|l| l.trim_end() == README_END)
    else {
        return BTreeMap::new();
    };
    let block = &lines[begin + 1..begin + 1 + len];
    let mut names = BTreeMap::new();
    let mut seen: Vec<String> = Vec::new();
    let mut in_table = false;
    for line in block {
        let line = line.trim();
        if line.is_empty() {
            if in_table {
                break;
            }
            continue;
        }
        if !line.starts_with('|') {
            continue;
        }
        let cells = split_cells(line);
        let (["", label, color, _hex, ""] | ["", label, color, _hex]) = cells.as_slice() else {
            continue;
        };
        let Some(name) = swatch_name(color) else {
            continue;
        };
        in_table = true;
        if seen.iter().any(|s| s == label) {
            return BTreeMap::new();
        }
        seen.push(label.to_string());
        let mut keep = |role: Role, name: &str| {
            let name = unescape_cell(name.trim());
            if !name.is_empty() && name != capitalised(role) {
                names.insert(role, name);
            }
        };
        match *label {
            "Terminal" => {}
            "Containers" => {
                let Some((container, pane)) = name.split_once(" / ") else {
                    return BTreeMap::new();
                };
                keep(Role::Container, container);
                keep(Role::Pane, pane);
            }
            _ => match label.to_lowercase().parse::<Role>() {
                Ok(role) if *label == capitalised(role) => keep(role, name),
                _ => return BTreeMap::new(),
            },
        }
    }
    names
}

fn ratio_text(ratio: f64, min: Option<f64>) -> String {
    let one = format!("{ratio:.1}");
    match min {
        Some(m) if ratio < m => {
            let shown = if one.parse::<f64>().is_ok_and(|r| r >= m) {
                format!("{ratio:.2}")
            } else {
                one
            };
            format!("{shown}:1 (fail)")
        }
        _ => format!("{one}:1"),
    }
}

fn showcase_row(p: &Palette, r: &RoleContrast) -> String {
    let cap = capitalised(r.role);
    let label = match p.names.get(&r.role) {
        Some(n) if *n != cap => format!("{}, {}", escape_cell(n), r.role),
        _ => cap,
    };
    let min = match r.min {
        Some(m) => format!("{m}:1"),
        None => "-".to_string(),
    };
    format!(
        "| {label} | `{}` | {} | {} | {min} |\n",
        r.color.to_hex(),
        ratio_text(r.on_background, r.min),
        ratio_text(r.on_pane, r.min)
    )
}

/// The showcase's contrast block: every role against the window and the pane.
pub fn showcase_block(p: &Palette) -> String {
    let mut out = format!(
        "Every text colour is checked against the window background `{}` and the\nterminal pane `{}`.\n\n",
        hex(p, Role::Background),
        hex(p, Role::Pane)
    );
    out.push_str("| Role | Hex | On background | On pane | Minimum |\n");
    out.push_str("|------|-----|---------------|---------|---------|\n");
    for row in &contrast::check(p).rows {
        out.push_str(&showcase_row(p, row));
    }
    out
}

/// The lines between a pair of markers: the block with a blank line on each side.
pub fn marked_body(block: &str) -> String {
    format!("\n{block}\n")
}

fn marked(begin: &str, block: &str, end: &str) -> String {
    format!("{begin}\n{}{end}\n", marked_body(block))
}

/// A new repo's README, with the palette block between its markers.
pub fn readme_skeleton(p: &Palette, block: &str) -> String {
    let stem = p.stem();
    let slot = match p.mode {
        crate::palette::Mode::Dark => "Dark",
        crate::palette::Mode::Light => "Light",
    };
    format!(
        "# COSMIC · {name}\n\
         \n\
         ## Palette\n\
         \n\
         {block}\
         \n\
         ## Setup\n\
         \n\
         Make {name} the active rice with `./scripts/switch.sh`, or set it up file by file:\n\
         \n\
         - `./cosmic/install.sh` copies the baseline COSMIC settings in `cosmic/config/`\n\
         - Settings, Desktop, Appearance, {slot}: import `cosmic/{stem}.ron`\n\
         - COSMIC Terminal, View, Color schemes, {slot}: import `cosmic-term/{stem}-term.ron`\n\
         - btop: copy `btop/{stem}.theme` to `~/.config/btop/themes/` and pick it in btop\n\
         - `./nvim/install.sh`, `./fastfetch/install.sh`, `./lsd/install.sh`, `./zsh/install.sh` and `./cava/install.sh`\n\
         \n\
         ## License\n\
         \n\
         See [LICENSE](LICENSE).\n",
        name = p.name,
        block = marked(README_BEGIN, block, README_END),
    )
}

/// A new repo's `docs/showcase.md`, with the contrast block between its markers.
pub fn showcase_skeleton(p: &Palette, pair: Option<&Palette>, block: &str) -> String {
    let pair_name = pair.map(|q| format!(" / {}", q.name)).unwrap_or_default();
    format!(
        "# {name}\n\n> The {mode} half of {name}{pair_name}.\n\n## Palette\n\n{block}",
        name = p.name,
        mode = p.mode,
        block = marked(SHOWCASE_BEGIN, block, SHOWCASE_END),
    )
}

/// The settings block at the top of `switch.sh`.
pub fn switch_settings(p: &Palette) -> String {
    let stem = p.stem();
    [
        (format!("NAME=\"{}\"", p.name), "shown in messages"),
        (
            format!("SLUG=\"{}\"", p.slug),
            "zsh/<slug>.zsh, nvim/lua/plugins/<slug>.lua",
        ),
        (
            format!("MODE=\"{}\"", p.mode),
            "the COSMIC half this rice fills: dark or light",
        ),
        (format!("RON=\"{stem}.ron\""), "desktop theme in cosmic/"),
        (
            format!("TERM_RON=\"{stem}-term.ron\""),
            "terminal scheme in cosmic-term/",
        ),
        (format!("BTOP=\"{stem}\""), "btop/<name>.theme"),
    ]
    .iter()
    .map(|(a, comment)| format!("{a:<35} # {comment}"))
    .collect::<Vec<_>>()
    .join("\n")
}

/// The `RICES` list of `switch.sh`, with `slug` added when it is missing.
pub fn rices_list(slug: &str) -> String {
    if RICES.split(' ').any(|r| r == slug) {
        RICES.to_string()
    } else {
        format!("{RICES} {slug}")
    }
}
