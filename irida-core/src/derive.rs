//! Derive the terminal slots and COSMIC extras from a palette and its pins.

use crate::color::{BLACK, Rgb, WHITE, mix};
use crate::palette::{Mode, Palette};
use crate::role::Role;
use crate::slot::{Ansi, PinValue, Pins, Slot, Tone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnsiSet([Rgb; 8]);

impl AnsiSet {
    fn index(a: Ansi) -> usize {
        Ansi::ALL.iter().position(|x| *x == a).expect("ansi in ALL")
    }

    pub fn get(&self, a: Ansi) -> Rgb {
        self.0[Self::index(a)]
    }

    fn set(&mut self, a: Ansi, c: Rgb) {
        self.0[Self::index(a)] = c;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermColors {
    pub foreground: Rgb,
    pub background: Option<Rgb>,
    pub cursor: Rgb,
    pub bright_foreground: Rgb,
    pub dim_foreground: Rgb,
    pub normal: AnsiSet,
    pub bright: AnsiSet,
    pub dim: AnsiSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub term: TermColors,
    pub neutral_10: Rgb,
    pub gray_2: Rgb,
}

impl Derived {
    /// The effective colour of a slot; `None` only for an unset background.
    pub fn slot(&self, s: Slot) -> Option<Rgb> {
        let t = &self.term;
        match s {
            Slot::Ansi(Tone::Normal, a) => Some(t.normal.get(a)),
            Slot::Ansi(Tone::Bright, a) => Some(t.bright.get(a)),
            Slot::Ansi(Tone::Dim, a) => Some(t.dim.get(a)),
            Slot::Foreground => Some(t.foreground),
            Slot::Background => t.background,
            Slot::Cursor => Some(t.cursor),
            Slot::BrightForeground => Some(t.bright_foreground),
            Slot::DimForeground => Some(t.dim_foreground),
            Slot::CosmicNeutral10 => Some(self.neutral_10),
            Slot::CosmicGray2 => Some(self.gray_2),
        }
    }
}

fn pinned(pins: &Pins, slot: Slot, derived: Rgb) -> Rgb {
    match pins.get(slot) {
        Some(PinValue::Color(c)) => c,
        _ => derived,
    }
}

fn tone_set(pins: &Pins, tone: Tone, base: [Rgb; 8]) -> AnsiSet {
    let mut set = AnsiSet(base);
    for a in Ansi::ALL {
        set.set(
            a,
            pinned(pins, Slot::Ansi(tone, a), base[AnsiSet::index(a)]),
        );
    }
    set
}

pub fn derive(p: &Palette) -> Derived {
    let r = |role: Role| p.roles.get(role);
    let pins = &p.pins;
    let dark = p.mode == Mode::Dark;
    let (text, muted, pane) = (r(Role::Text), r(Role::Muted), r(Role::Pane));

    let normal_base = [
        if dark {
            mix(r(Role::Container), text, 0.05)
        } else {
            text
        },
        r(Role::Error),
        r(Role::Success),
        r(Role::Warning),
        r(Role::Blue),
        r(Role::Magenta),
        r(Role::Cyan),
        if dark { text } else { muted },
    ];
    let normal = tone_set(pins, Tone::Normal, normal_base);

    let mut bright_base = [BLACK; 8];
    for a in Ansi::ALL {
        let n = normal.get(a);
        bright_base[AnsiSet::index(a)] = if dark {
            match a {
                Ansi::Black => muted,
                Ansi::White => mix(text, WHITE, 0.29),
                _ => mix(n, WHITE, 0.12),
            }
        } else {
            mix(n, BLACK, 0.15)
        };
    }
    let bright = tone_set(pins, Tone::Bright, bright_base);

    let mut dim_base = [BLACK; 8];
    for a in Ansi::ALL {
        dim_base[AnsiSet::index(a)] = mix(normal.get(a), pane, 0.40);
    }
    let dim = tone_set(pins, Tone::Dim, dim_base);

    let bright_fg = if dark {
        bright.get(Ansi::White)
    } else {
        bright.get(Ansi::Black)
    };
    let background = match pins.get(Slot::Background) {
        Some(PinValue::None) => None,
        Some(PinValue::Color(c)) => Some(c),
        None => Some(pane),
    };
    let term = TermColors {
        foreground: pinned(pins, Slot::Foreground, text),
        background,
        cursor: pinned(pins, Slot::Cursor, r(Role::Accent)),
        bright_foreground: pinned(pins, Slot::BrightForeground, bright_fg),
        dim_foreground: pinned(pins, Slot::DimForeground, muted),
        normal,
        bright,
        dim,
    };

    let gray_base = if dark { normal.get(Ansi::Black) } else { pane };
    Derived {
        neutral_10: pinned(pins, Slot::CosmicNeutral10, text),
        gray_2: pinned(pins, Slot::CosmicGray2, gray_base),
        term,
    }
}
