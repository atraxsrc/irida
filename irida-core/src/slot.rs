//! Terminal and COSMIC slots, and the pins that override derived values.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use crate::color::Rgb;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ansi {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl Ansi {
    pub const ALL: [Ansi; 8] = [
        Ansi::Black,
        Ansi::Red,
        Ansi::Green,
        Ansi::Yellow,
        Ansi::Blue,
        Ansi::Magenta,
        Ansi::Cyan,
        Ansi::White,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Ansi::Black => "black",
            Ansi::Red => "red",
            Ansi::Green => "green",
            Ansi::Yellow => "yellow",
            Ansi::Blue => "blue",
            Ansi::Magenta => "magenta",
            Ansi::Cyan => "cyan",
            Ansi::White => "white",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tone {
    Normal,
    Bright,
    Dim,
}

impl Tone {
    pub fn as_str(self) -> &'static str {
        match self {
            Tone::Normal => "normal",
            Tone::Bright => "bright",
            Tone::Dim => "dim",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    Ansi(Tone, Ansi),
    Foreground,
    Background,
    Cursor,
    BrightForeground,
    DimForeground,
    CosmicNeutral10,
    CosmicGray2,
}

const fn term_slots() -> [Slot; 29] {
    let mut out = [Slot::Foreground; 29];
    let tones = [Tone::Normal, Tone::Bright, Tone::Dim];
    let mut t = 0;
    while t < 3 {
        let mut a = 0;
        while a < 8 {
            out[t * 8 + a] = Slot::Ansi(tones[t], Ansi::ALL[a]);
            a += 1;
        }
        t += 1;
    }
    out[24] = Slot::Foreground;
    out[25] = Slot::Background;
    out[26] = Slot::Cursor;
    out[27] = Slot::BrightForeground;
    out[28] = Slot::DimForeground;
    out
}

impl Slot {
    /// The 29 terminal slots: 24 ANSI colours then the five specials.
    pub const ALL_TERM: [Slot; 29] = term_slots();
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Slot::Ansi(t, a) => f.pad(&format!("{}.{}", t.as_str(), a.as_str())),
            Slot::Foreground => f.pad("foreground"),
            Slot::Background => f.pad("background"),
            Slot::Cursor => f.pad("cursor"),
            Slot::BrightForeground => f.pad("bright_foreground"),
            Slot::DimForeground => f.pad("dim_foreground"),
            Slot::CosmicNeutral10 => f.pad("cosmic.neutral_10"),
            Slot::CosmicGray2 => f.pad("cosmic.gray_2"),
        }
    }
}

impl FromStr for Slot {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Slot::ALL_TERM
            .into_iter()
            .chain([Slot::CosmicNeutral10, Slot::CosmicGray2])
            .find(|slot| slot.to_string() == s)
            .ok_or_else(|| format!("unknown slot: {s:?}"))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PinValue {
    Color(Rgb),
    None,
}

/// Why a pin was refused.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PinError {
    /// Only `background` can be pinned to `none`.
    NoneNotAllowed(Slot),
}

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PinError::NoneNotAllowed(slot) => {
                write!(f, "{slot} cannot be none; only background can")
            }
        }
    }
}

impl std::error::Error for PinError {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pins(BTreeMap<Slot, PinValue>);

impl Pins {
    pub fn get(&self, slot: Slot) -> Option<PinValue> {
        self.0.get(&slot).copied()
    }

    /// Pin a slot. `PinValue::None` is accepted only for `Slot::Background`;
    /// any other slot is refused and the pins stay as they were.
    pub fn set(&mut self, slot: Slot, value: PinValue) -> Result<(), PinError> {
        if value == PinValue::None && slot != Slot::Background {
            return Err(PinError::NoneNotAllowed(slot));
        }
        self.0.insert(slot, value);
        Ok(())
    }

    pub fn remove(&mut self, slot: Slot) {
        self.0.remove(&slot);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn is_pinned(&self, slot: Slot) -> bool {
        self.0.contains_key(&slot)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Slot, PinValue)> + '_ {
        self.0.iter().map(|(s, v)| (*s, *v))
    }

    /// Build from the raw `[pins]` table. `"none"` is valid only for `background`.
    pub(crate) fn from_raw(raw: &BTreeMap<String, String>) -> Result<Pins, (String, String)> {
        let mut pins = Pins::default();
        for (key, value) in raw {
            let bad = || (format!("pins.{key}"), value.clone());
            let slot: Slot = key.parse().map_err(|_| bad())?;
            let pv = if value == "none" {
                PinValue::None
            } else {
                PinValue::Color(Rgb::from_hex(value).map_err(|_| bad())?)
            };
            pins.set(slot, pv).map_err(|_| bad())?;
        }
        Ok(pins)
    }
}
