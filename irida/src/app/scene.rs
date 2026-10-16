//! The preview scene model: what the preview draws, as plain coloured text
//! and colour lists, decided from the palette alone. `preview.rs` only
//! draws it.
//!
//! Every colour follows the target templates in `irida-core/targets`, so the
//! preview shows what an apply would give: fastfetch's box, `lsd -l` with the
//! zsh `LS_COLORS`, zsh syntax highlighting and autosuggestions, the Neovim
//! colorscheme, btop's meter gradient and cava's bar gradient.

use cosmic::cosmic_theme::palette::Srgba;
use irida_core::color::{Rgb, mix};
use irida_core::contrast::ratio;
use irida_core::derive::derive;
use irida_core::native::cosmic_desktop::cosmic_palette;
use irida_core::palette::Palette;
use irida_core::ramp::ramp;
use irida_core::role::Role;

/// A run of text in one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub fg: Rgb,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// The COSMIC settings card's colours.
#[derive(Debug, Clone, PartialEq)]
pub struct CardColors {
    /// The window around the card.
    pub window: Rgb,
    /// The window's title.
    pub title: Rgb,
    /// The card itself.
    pub container: Rgb,
    pub text: Rgb,
    pub caption: Rgb,
    pub divider: Rgb,
    /// The toggle that is on and the suggested button.
    pub accent: Rgb,
    pub on_accent: Rgb,
    /// The destructive button.
    pub destructive: Rgb,
    pub on_destructive: Rgb,
}

/// Everything the preview shows for one palette.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub panel: Vec<Span>,
    pub panel_bg: Rgb,
    pub terminal: Vec<Vec<Span>>,
    pub terminal_bg: Rgb,
    pub terminal_fg: Rgb,
    pub nvim: Vec<Vec<Span>>,
    pub nvim_bg: Rgb,
    pub btop: Vec<Rgb>,
    pub cava: Vec<(f32, Rgb)>,
    pub card: CardColors,
}

/// Builds spans without repeating the style flags.
#[derive(Clone, Copy)]
struct Style {
    fg: Rgb,
    bg: Option<Rgb>,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl Style {
    fn fg(fg: Rgb) -> Style {
        Style {
            fg,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    fn bold(self) -> Style {
        Style { bold: true, ..self }
    }

    fn italic(self) -> Style {
        Style {
            italic: true,
            ..self
        }
    }

    fn underline(self) -> Style {
        Style {
            underline: true,
            ..self
        }
    }

    fn on(self, bg: Rgb) -> Style {
        Style {
            bg: Some(bg),
            ..self
        }
    }

    fn span(self, text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
        }
    }
}

/// The roles by name, as the templates use them.
struct Roles<'a>(&'a Palette);

impl Roles<'_> {
    fn get(&self, role: Role) -> Rgb {
        self.0.roles.get(role)
    }

    fn style(&self, role: Role) -> Style {
        Style::fg(self.get(role))
    }
}

/// `ramp(a, b, n)` then `ramp(b, c, m)` without the repeated middle colour,
/// as the btop and cava templates join them.
fn joined(a: Rgb, b: Rgb, c: Rgb, n: usize, m: usize) -> Vec<Rgb> {
    let mut out = ramp(a, b, n).expect("steps in range");
    out.extend_from_slice(&ramp(b, c, m).expect("steps in range")[1..]);
    out
}

/// A shell prompt (starship's defaults: the folder in bold cyan, the prompt
/// character in bold green), then the typed command line.
fn prompt(r: &Roles, typed: Vec<Span>) -> Vec<Span> {
    let mut line = vec![
        r.style(Role::Cyan).bold().span("~/rice"),
        r.style(Role::Success).bold().span(" $ "),
    ];
    line.extend(typed);
    line
}

const FASTFETCH_WIDTH: usize = 30;
const LOGO: [&str; 6] = [
    "   ____   ",
    "  / __ \\  ",
    " | |  | | ",
    " | |__| | ",
    "  \\____/  ",
    "          ",
];

/// A box border line with a title, as the fastfetch template draws it.
fn ff_title(lines: Style, title: Style, name: &str) -> Vec<Span> {
    let rest = FASTFETCH_WIDTH - 2 - name.chars().count();
    let left = rest / 2;
    vec![
        lines.span(format!("┌{}", "─".repeat(left))),
        title.span(name),
        lines.span(format!("{}┐", "─".repeat(rest - left))),
    ]
}

/// fastfetch: logo and values in `text`, keys in `accent`, box lines in
/// `mix(text, pane, 0.70)` and section titles in `magenta`.
fn fastfetch(r: &Roles, p: &Palette) -> Vec<Vec<Span>> {
    let text = r.style(Role::Text);
    let key = r.style(Role::Accent);
    let lines = Style::fg(mix(r.get(Role::Text), r.get(Role::Pane), 0.70));
    let title = r.style(Role::Magenta);
    let bottom = || vec![lines.span(format!("└{}┘", "─".repeat(FASTFETCH_WIDTH - 2)))];
    let row = |branch: &str, k: &str, v: &str| {
        vec![
            lines.span(branch.to_string()),
            key.span(k),
            text.span(": "),
            text.span(v),
        ]
    };
    let hardware = vec![
        ff_title(lines, title, "Hardware"),
        row(" ", "PC", "Framework 13"),
        row("├ ", "CPU", "Ryzen 7 7840U"),
        row("└ ", "Memory", "9.1 GiB / 32 GiB"),
        bottom(),
    ];
    let mut colours = row("└ ", "Palette", &p.name);
    colours.push(text.span(" "));
    for role in [
        Role::Text,
        Role::Accent,
        Role::Success,
        Role::Warning,
        Role::Error,
        Role::Magenta,
        Role::Cyan,
    ] {
        colours.push(r.style(role).span("●"));
    }
    let software = vec![
        ff_title(lines, title, "Software"),
        row(" ", "OS", "Pop!_OS 24.04"),
        row("├ ", "DE", "COSMIC"),
        row("├ ", "Shell", "zsh 5.9"),
        colours,
        bottom(),
    ];

    let mut out = Vec::new();
    for (i, mut line) in hardware.into_iter().chain(software).enumerate() {
        let logo = LOGO.get(i).copied().unwrap_or(LOGO[LOGO.len() - 1]);
        let mut full = vec![text.span(format!("  {logo}  "))];
        full.append(&mut line);
        out.push(full);
    }
    out
}

/// One `lsd -l` entry.
struct Entry {
    kind: char,
    perms: &'static str,
    size: (&'static str, Role),
    date: (&'static str, Role),
    name: Span,
    target: Option<Span>,
}

/// `lsd -l`: permissions, user and group, size and date as the lsd template
/// colours them; names as the zsh template's `LS_COLORS` (`di=1`, `ex=1`,
/// `*.gz=4`, `ln=3`, `*.md` in `text`).
fn lsd(r: &Roles) -> Vec<Vec<Span>> {
    let folder = r.style(Role::Blue).bold();
    let entries = [
        Entry {
            kind: 'd',
            perms: "rwxr-xr-x",
            size: ("-", Role::Comment),
            date: ("Sep 28 18:20", Role::Comment),
            name: folder.span("rice"),
            target: None,
        },
        Entry {
            kind: '.',
            perms: "rw-r--r--",
            size: ("2.1 KB", Role::Success),
            date: ("Oct 17 09:12", Role::Accent),
            name: r.style(Role::Text).span("README.md"),
            target: None,
        },
        Entry {
            kind: '.',
            perms: "rwxr-xr-x",
            size: ("5.3 MB", Role::Warning),
            date: ("Oct 16 22:40", Role::Success),
            name: r.style(Role::Success).bold().span("irida"),
            target: None,
        },
        Entry {
            kind: '.',
            perms: "rw-------",
            size: ("1.2 GB", Role::Error),
            date: ("Sep 30 11:05", Role::Comment),
            name: r.style(Role::Warning).underline().span("backup.tar.gz"),
            target: None,
        },
        Entry {
            kind: 'l',
            perms: "rwxrwxrwx",
            size: ("4 B", Role::Success),
            date: ("Oct 17 08:55", Role::Accent),
            name: r.style(Role::Cyan).italic().span("config"),
            target: Some(folder.span("rice")),
        },
    ];
    let pad = |s: &str, w: usize| format!("{s:>w$}");
    entries
        .into_iter()
        .map(|e| {
            // The type letter takes the name's colour; a plain file's dot is quiet.
            let kind = match e.kind {
                '.' => r.style(Role::Comment),
                _ => Style::fg(e.name.fg),
            };
            let mut line = vec![kind.span(e.kind.to_string())];
            for c in e.perms.chars() {
                let role = match c {
                    'r' => Role::Muted,
                    'w' => Role::Warning,
                    'x' => Role::Error,
                    _ => Role::Comment,
                };
                line.push(r.style(role).span(c.to_string()));
            }
            line.push(r.style(Role::Text).span(" "));
            line.push(r.style(Role::Accent).span("user"));
            line.push(r.style(Role::Text).span(" "));
            line.push(r.style(Role::Muted).span("users"));
            line.push(r.style(Role::Text).span(pad("", 7 - e.size.0.len())));
            line.push(r.style(e.size.1).span(e.size.0));
            line.push(r.style(Role::Text).span(" "));
            line.push(r.style(e.date.1).span(e.date.0));
            line.push(r.style(Role::Text).span(" "));
            line.push(e.name);
            if let Some(target) = e.target {
                line.push(r.style(Role::Text).span(" ⇒ "));
                line.push(target);
            }
            line
        })
        .collect()
}

/// The terminal session: fastfetch, `lsd -l`, a mistyped command, and a
/// prompt with an autosuggestion. Typed commands follow the zsh template's
/// highlighting: commands in `accent`, options in `muted`, an unknown
/// command in `error`, bold and underlined; the suggestion in
/// `mix(comment, pane, 0.30)`.
fn terminal(r: &Roles, p: &Palette, fg: Rgb) -> Vec<Vec<Span>> {
    let command = r.style(Role::Accent);
    let option = r.style(Role::Muted);
    let plain = Style::fg(fg);
    let mut out = vec![prompt(r, vec![command.span("fastfetch")])];
    out.extend(fastfetch(r, p));
    out.push(prompt(
        r,
        vec![command.span("lsd"), plain.span(" "), option.span("-l")],
    ));
    out.extend(lsd(r));
    out.push(prompt(
        r,
        vec![
            r.style(Role::Error).bold().underline().span("gti"),
            plain.span(" status"),
        ],
    ));
    out.push(vec![plain.span("zsh: command not found: gti")]);
    let suggestion = Style::fg(mix(r.get(Role::Comment), r.get(Role::Pane), 0.30));
    out.push(prompt(
        r,
        vec![command.span("git"), suggestion.span(" status")],
    ));
    out
}

const NVIM_WIDTH: usize = 44;
const NVIM_CURSOR: usize = 4;

/// Neovim: 8 lines of Rust as the colorscheme template maps the groups
/// (keyword `magenta`, function `blue`, string `success`, number `warning`,
/// type `cyan`, operator `muted`, comment `comment` and italic, macro and
/// parameter as the template's `spec` and `@variable.parameter`), line
/// numbers in `comment`, the cursor line on `background` with its number in
/// bold `accent`, and a status line.
fn nvim(r: &Roles) -> Vec<Vec<Span>> {
    let kw = r.style(Role::Magenta);
    let func = r.style(Role::Blue);
    let string = r.style(Role::Success);
    let num = r.style(Role::Warning);
    let ty = r.style(Role::Cyan);
    let builtin = ty.bold();
    let op = r.style(Role::Muted);
    let param = op.italic();
    let comment = r.style(Role::Comment).italic();
    let macro_ = r.style(Role::Accent);
    let fg = r.style(Role::Text);
    let s = |st: Style, t: &str| (st, t.to_string());

    let code: Vec<Vec<(Style, String)>> = vec![
        vec![s(comment, "// Warm colours first, cool ones last.")],
        vec![
            s(kw, "struct"),
            s(fg, " "),
            s(ty, "Coal"),
            s(fg, " "),
            s(op, "{"),
            s(fg, " heat"),
            s(op, ":"),
            s(fg, " "),
            s(builtin, "u32"),
            s(fg, " "),
            s(op, "}"),
        ],
        vec![],
        vec![
            s(kw, "fn"),
            s(fg, " "),
            s(func, "glow"),
            s(op, "("),
            s(param, "c"),
            s(op, ":"),
            s(fg, " "),
            s(op, "&"),
            s(ty, "Coal"),
            s(op, ")"),
            s(fg, " "),
            s(op, "->"),
            s(fg, " "),
            s(ty, "String"),
            s(fg, " "),
            s(op, "{"),
        ],
        vec![
            s(fg, "    "),
            s(kw, "let"),
            s(fg, " "),
            s(fg, "level"),
            s(fg, " "),
            s(op, "="),
            s(fg, " c"),
            s(op, "."),
            s(fg, "heat"),
            s(fg, " "),
            s(kw, "as"),
            s(fg, " "),
            s(builtin, "f64"),
            s(fg, " "),
            s(op, "*"),
            s(fg, " "),
            s(num, "0.42"),
            s(op, ";"),
        ],
        vec![
            s(fg, "    "),
            s(macro_, "format!"),
            s(op, "("),
            s(string, "\"glowing at {level:.1}\""),
            s(op, ")"),
        ],
        vec![s(op, "}")],
        vec![
            s(kw, "const"),
            s(fg, " "),
            s(num, "EMBERS"),
            s(op, ":"),
            s(fg, " "),
            s(builtin, "usize"),
            s(fg, " "),
            s(op, "="),
            s(fg, " "),
            s(num, "7"),
            s(op, ";"),
        ],
    ];

    let line_nr = r.style(Role::Comment);
    let cursor_nr = r.style(Role::Accent).bold();
    let cursor_bg = r.get(Role::Background);
    let mut out: Vec<Vec<Span>> = code
        .into_iter()
        .enumerate()
        .map(|(i, parts)| {
            let on_cursor = i == NVIM_CURSOR;
            let paint = |st: Style| if on_cursor { st.on(cursor_bg) } else { st };
            let nr = if on_cursor { cursor_nr } else { line_nr };
            let mut line = vec![paint(nr).span(format!("{:>3} ", i + 1))];
            let mut width = 4;
            for (st, t) in parts {
                width += t.chars().count();
                line.push(paint(st).span(t));
            }
            if on_cursor && width < NVIM_WIDTH {
                line.push(paint(fg).span(" ".repeat(NVIM_WIDTH - width)));
            }
            line
        })
        .collect();

    // The status line: StatusLine is `text` on `background`, with the mode
    // in the colours of a selected item (`pane` on `accent`).
    let bar = fg.on(r.get(Role::Background));
    let mode = Style::fg(r.get(Role::Pane)).on(r.get(Role::Accent)).bold();
    let left = " NORMAL ";
    let file = " src/main.rs ";
    let right = format!(" rust  {}:9 ", NVIM_CURSOR + 1);
    let used = left.len() + file.len() + right.len();
    out.push(vec![
        mode.span(left),
        bar.span(file),
        bar.span(" ".repeat(NVIM_WIDTH.saturating_sub(used))),
        bar.span(right),
    ]);
    out
}

/// cava's bar heights, low at the edges like a song's spectrum.
const CAVA_HEIGHTS: [f32; 16] = [
    0.35, 0.55, 0.8, 1.0, 0.9, 0.7, 0.5, 0.65, 0.85, 0.6, 0.4, 0.3, 0.45, 0.25, 0.15, 0.1,
];

/// The colour of the cava gradient at the top of a bar of height `h`: the
/// gradient runs from the bottom of the area to its top.
fn cava_colour(gradient: &[Rgb], h: f32) -> Rgb {
    let n = gradient.len();
    let k = ((h * n as f32).ceil() as usize).clamp(1, n);
    gradient[k - 1]
}

fn rgb(c: Srgba) -> Rgb {
    let c: Srgba<u8> = c.into_format();
    Rgb {
        r: c.red,
        g: c.green,
        b: c.blue,
    }
}

/// The settings card: window, card and text from the roles Irida writes to
/// the theme builder (`bg_color`, `primary_container_bg`, `text_tint`,
/// `accent`, `destructive`); the title, divider and the text on buttons from
/// `cosmic_palette(p)` (`neutral_10`, `gray_2`, and whichever end of the
/// neutral ramp reads better on the button).
fn card(r: &Roles, p: &Palette) -> CardColors {
    let cp = cosmic_palette(p);
    let inner = cp.as_ref();
    let (low, high) = (rgb(inner.neutral_0), rgb(inner.neutral_10));
    let on = |bg: Rgb| {
        if ratio(low, bg) >= ratio(high, bg) {
            low
        } else {
            high
        }
    };
    CardColors {
        window: r.get(Role::Background),
        title: high,
        container: r.get(Role::Container),
        text: r.get(Role::Text),
        caption: r.get(Role::Muted),
        divider: rgb(inner.gray_2),
        accent: r.get(Role::Accent),
        on_accent: on(r.get(Role::Accent)),
        destructive: r.get(Role::Error),
        on_destructive: on(r.get(Role::Error)),
    }
}

/// The COSMIC panel: workspaces (the active one in `accent`), the clock and
/// the tray, on the theme's background.
fn panel(r: &Roles, on_accent: Rgb) -> Vec<Span> {
    let text = r.style(Role::Text);
    vec![
        Style::fg(on_accent)
            .on(r.get(Role::Accent))
            .bold()
            .span(" 1 "),
        text.span(" 2 "),
        text.span(" 3 "),
        text.span("    "),
        text.bold().span("Fri Oct 17  09:41"),
        text.span("    "),
        r.style(Role::Muted).span("Wi-Fi  100%"),
    ]
}

pub fn scene(p: &Palette) -> Scene {
    let r = Roles(p);
    let d = derive(p);
    let terminal_fg = d.term.foreground;
    let terminal_bg = d.term.background.unwrap_or(r.get(Role::Pane));
    let (success, accent, error) = (
        r.get(Role::Success),
        r.get(Role::Accent),
        r.get(Role::Error),
    );
    let gradient = joined(success, accent, error, 4, 5);
    let card = card(&r, p);
    Scene {
        panel: panel(&r, card.on_accent),
        panel_bg: r.get(Role::Background),
        terminal: terminal(&r, p, terminal_fg),
        terminal_bg,
        terminal_fg,
        nvim: nvim(&r),
        nvim_bg: r.get(Role::Pane),
        btop: joined(success, accent, error, 5, 6),
        cava: CAVA_HEIGHTS
            .iter()
            .map(|&h| (h, cava_colour(&gradient, h)))
            .collect(),
        card,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use irida_core::color::mix;
    use irida_core::ramp::ramp;
    use irida_core::role::Role;
    use irida_core::store::starters;

    fn starter(slug: &str) -> Palette {
        starters()
            .into_iter()
            .find(|p| p.slug == slug)
            .expect("a starter")
    }

    /// Every span of the scene's lines, in order.
    fn spans(lines: &[Vec<Span>]) -> impl Iterator<Item = &Span> {
        lines.iter().flatten()
    }

    fn find<'a>(lines: &'a [Vec<Span>], text: &str) -> &'a Span {
        spans(lines)
            .find(|s| s.text == text)
            .unwrap_or_else(|| panic!("no span {text:?}"))
    }

    fn line_text(line: &[Span]) -> String {
        line.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn lsd_colours_follow_template() {
        let p = starter("ember");
        let r = |role| p.roles.get(role);
        let s = scene(&p);
        let t = &s.terminal;

        let folder = find(t, "rice");
        assert_eq!(folder.fg, r(Role::Blue));
        assert!(folder.bold && !folder.italic && !folder.underline);

        let exe = find(t, "irida");
        assert_eq!(exe.fg, r(Role::Success));
        assert!(exe.bold);

        let archive = find(t, "backup.tar.gz");
        assert_eq!(archive.fg, r(Role::Warning));
        assert!(archive.underline && !archive.bold);

        let link = find(t, "config");
        assert_eq!(link.fg, r(Role::Cyan));
        assert!(link.italic && !link.bold);

        let md = find(t, "README.md");
        assert_eq!(md.fg, r(Role::Text));

        // Every lsd row names the user in accent and the group in muted.
        let users: Vec<_> = spans(t).filter(|s| s.text == "user").collect();
        assert_eq!(users.len(), 5);
        assert!(users.iter().all(|s| s.fg == r(Role::Accent)));
        let groups: Vec<_> = spans(t).filter(|s| s.text == "users").collect();
        assert_eq!(groups.len(), 5);
        assert!(groups.iter().all(|s| s.fg == r(Role::Muted)));

        // Permissions: read muted, write warning, exec error, none comment.
        assert!(spans(t).any(|s| s.text == "r" && s.fg == r(Role::Muted)));
        assert!(spans(t).any(|s| s.text == "w" && s.fg == r(Role::Warning)));
        assert!(spans(t).any(|s| s.text == "x" && s.fg == r(Role::Error)));
        assert!(spans(t).any(|s| s.text == "-" && s.fg == r(Role::Comment)));

        // Sizes: none, small, medium, large.
        assert_eq!(find(t, "2.1 KB").fg, r(Role::Success));
        assert_eq!(find(t, "5.3 MB").fg, r(Role::Warning));
        assert_eq!(find(t, "1.2 GB").fg, r(Role::Error));

        // Dates: hour old, day old, older.
        assert_eq!(find(t, "Oct 17 09:12").fg, r(Role::Accent));
        assert_eq!(find(t, "Oct 16 22:40").fg, r(Role::Success));
        assert_eq!(find(t, "Sep 30 11:05").fg, r(Role::Comment));
    }

    #[test]
    fn mistyped_command_in_error() {
        let p = starter("ember");
        let s = scene(&p);
        let t = &s.terminal;

        assert!(
            spans(t).any(|s| s.text == "gti"
                && s.fg == p.roles.get(Role::Error)
                && s.bold
                && s.underline)
        );
        assert!(
            t.iter()
                .any(|l| line_text(l).contains("command not found: gti"))
        );

        let suggestion = mix(p.roles.get(Role::Comment), p.roles.get(Role::Pane), 0.30);
        let last = t.last().expect("a prompt line");
        let ghost = last.last().expect("an autosuggestion");
        assert_eq!(ghost.fg, suggestion);
        assert_eq!(ghost.text, " status");
        // The typed command before it is in accent.
        assert!(
            last.iter()
                .any(|s| s.text == "git" && s.fg == p.roles.get(Role::Accent))
        );
    }

    #[test]
    fn fastfetch_box() {
        let p = starter("ember");
        let r = |role| p.roles.get(role);
        let s = scene(&p);
        let t = &s.terminal;
        let lines = mix(r(Role::Text), r(Role::Pane), 0.70);

        assert_eq!(find(t, "Hardware").fg, r(Role::Magenta));
        assert_eq!(find(t, "Software").fg, r(Role::Magenta));
        assert!(spans(t).any(|s| s.text.starts_with('┌') && s.fg == lines));
        assert!(spans(t).any(|s| s.text.starts_with('└') && s.fg == lines));
        assert_eq!(find(t, "CPU").fg, r(Role::Accent));
        assert_eq!(find(t, "Ryzen 7 7840U").fg, r(Role::Text));
        assert_eq!(find(t, "Ember").fg, r(Role::Text));
    }

    #[test]
    fn nvim_mapping() {
        let p = starter("ember");
        let r = |role| p.roles.get(role);
        let s = scene(&p);
        let n = &s.nvim;
        assert_eq!(n.len(), 9, "8 lines of code and a status line");

        assert_eq!(find(n, "fn").fg, r(Role::Magenta));
        assert_eq!(find(n, "glow").fg, r(Role::Blue));
        assert_eq!(find(n, "\"glowing at {level:.1}\"").fg, r(Role::Success));
        assert_eq!(find(n, "0.42").fg, r(Role::Warning));
        assert_eq!(find(n, "Coal").fg, r(Role::Cyan));
        assert_eq!(find(n, "->").fg, r(Role::Muted));
        let comment = spans(n)
            .find(|s| s.text.starts_with("//"))
            .expect("a comment");
        assert_eq!(comment.fg, r(Role::Comment));
        assert!(comment.italic);

        // Line numbers in comment, the cursor line's in accent on background.
        let cursor = n
            .iter()
            .position(|l| l.iter().any(|s| s.text == "level"))
            .expect("the cursor line");
        assert_eq!(n[0][0].fg, r(Role::Comment));
        assert_eq!(n[cursor][0].fg, r(Role::Accent));
        assert!(n[cursor].iter().all(|s| s.bg == Some(r(Role::Background))));
        assert!(n[0].iter().all(|s| s.bg.is_none()));
        assert_eq!(s.nvim_bg, r(Role::Pane));

        // The status line sits on background.
        let status = n.last().expect("a status line");
        assert!(status.iter().all(|s| s.bg.is_some()));
    }

    #[test]
    fn ramps() {
        let p = starter("ember");
        let r = |role| p.roles.get(role);
        let s = scene(&p);
        assert_eq!(s.btop.len(), 10);
        assert_eq!(s.btop[0], r(Role::Success));
        assert_eq!(s.btop[4], r(Role::Accent));
        assert_eq!(s.btop[9], r(Role::Error));

        let mut eight = ramp(r(Role::Success), r(Role::Accent), 4).unwrap();
        eight.extend_from_slice(&ramp(r(Role::Accent), r(Role::Error), 5).unwrap()[1..]);
        assert_eq!(eight.len(), 8);
        assert_eq!(s.cava.len(), 16);
        for (h, c) in &s.cava {
            assert!(*h > 0.0 && *h <= 1.0);
            assert!(eight.contains(c), "{c:?} is not a cava colour");
        }
        // Taller bars reach further along the gradient.
        let (tallest, low) = s.cava.iter().fold((s.cava[0], s.cava[0]), |(hi, lo), b| {
            (
                if b.0 > hi.0 { *b } else { hi },
                if b.0 < lo.0 { *b } else { lo },
            )
        });
        assert_eq!(tallest.1, r(Role::Error));
        assert_eq!(low.1, r(Role::Success));
    }

    #[test]
    fn card_and_surfaces() {
        let p = starter("ember");
        let r = |role| p.roles.get(role);
        let s = scene(&p);
        assert_eq!(s.card.window, r(Role::Background));
        assert_eq!(s.card.container, r(Role::Container));
        assert_eq!(s.card.text, r(Role::Text));
        assert_eq!(s.card.accent, r(Role::Accent));
        assert_eq!(s.card.destructive, r(Role::Error));
        assert_eq!(s.card.divider, irida_core::derive::derive(&p).gray_2);
        assert_eq!(s.panel_bg, r(Role::Background));
        assert_eq!(s.terminal_bg, r(Role::Pane));
        assert_eq!(s.terminal_fg, r(Role::Text));
    }

    #[test]
    fn no_background_shows_pane() {
        let mut p = starter("ember");
        p.pins
            .set(
                irida_core::slot::Slot::Background,
                irida_core::slot::PinValue::None,
            )
            .unwrap();
        assert_eq!(scene(&p).terminal_bg, p.roles.get(Role::Pane));
    }

    #[test]
    fn light_palette_scene() {
        let p = starter("emberday");
        let s = scene(&p);
        assert_eq!(find(&s.terminal, "rice").fg, p.roles.get(Role::Blue));
        assert_eq!(s.nvim_bg, p.roles.get(Role::Pane));
        assert_eq!(s.card.window, p.roles.get(Role::Background));
    }
}
