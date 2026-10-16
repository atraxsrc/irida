//! A mirror of COSMIC Terminal's `ColorScheme` RON, so schemes round-trip unchanged.

use hex_color::HexColor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::color::Rgb;
use crate::slot::{Ansi, Slot, Tone};

fn de_color<'de, D: Deserializer<'de>>(d: D) -> Result<Option<HexColor>, D::Error> {
    HexColor::deserialize(d).map(Some)
}

fn ser_color<S: Serializer>(c: &Option<HexColor>, s: S) -> Result<S::Ok, S::Error> {
    match c {
        Some(c) => c.serialize(s),
        None => s.serialize_none(),
    }
}

fn to_rgb(c: HexColor) -> Rgb {
    Rgb {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

fn to_hex(c: Rgb) -> HexColor {
    HexColor::rgb(c.r, c.g, c.b)
}

macro_rules! scheme_structs {
    () => {
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct AnsiScheme {
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub black: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub red: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub green: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub yellow: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub blue: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub magenta: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub cyan: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub white: Option<HexColor>,
        }

        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct ColorScheme {
            pub name: String,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub foreground: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub background: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub cursor: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub bright_foreground: Option<HexColor>,
            #[serde(
                deserialize_with = "de_color",
                serialize_with = "ser_color",
                skip_serializing_if = "Option::is_none"
            )]
            pub dim_foreground: Option<HexColor>,
            #[serde(skip_serializing_if = "AnsiScheme::is_empty")]
            pub normal: AnsiScheme,
            #[serde(skip_serializing_if = "AnsiScheme::is_empty")]
            pub bright: AnsiScheme,
            #[serde(skip_serializing_if = "AnsiScheme::is_empty")]
            pub dim: AnsiScheme,
        }
    };
}

scheme_structs!();

impl AnsiScheme {
    fn is_empty(&self) -> bool {
        *self == AnsiScheme::default()
    }

    fn get(&self, a: Ansi) -> Option<HexColor> {
        match a {
            Ansi::Black => self.black,
            Ansi::Red => self.red,
            Ansi::Green => self.green,
            Ansi::Yellow => self.yellow,
            Ansi::Blue => self.blue,
            Ansi::Magenta => self.magenta,
            Ansi::Cyan => self.cyan,
            Ansi::White => self.white,
        }
    }

    fn from_set(set: &crate::derive::AnsiSet) -> AnsiScheme {
        let c = |a: Ansi| Some(to_hex(set.get(a)));
        AnsiScheme {
            black: c(Ansi::Black),
            red: c(Ansi::Red),
            green: c(Ansi::Green),
            yellow: c(Ansi::Yellow),
            blue: c(Ansi::Blue),
            magenta: c(Ansi::Magenta),
            cyan: c(Ansi::Cyan),
            white: c(Ansi::White),
        }
    }
}

impl ColorScheme {
    pub fn from_ron(s: &str) -> Result<ColorScheme, ron::error::SpannedError> {
        ron::from_str(s)
    }

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::new())
            .expect("a colour scheme serialises")
    }

    /// The slot's colour as written, alpha included.
    pub fn raw(&self, s: Slot) -> Option<HexColor> {
        match s {
            Slot::Ansi(Tone::Normal, a) => self.normal.get(a),
            Slot::Ansi(Tone::Bright, a) => self.bright.get(a),
            Slot::Ansi(Tone::Dim, a) => self.dim.get(a),
            Slot::Foreground => self.foreground,
            Slot::Background => self.background,
            Slot::Cursor => self.cursor,
            Slot::BrightForeground => self.bright_foreground,
            Slot::DimForeground => self.dim_foreground,
            Slot::CosmicNeutral10 | Slot::CosmicGray2 => None,
        }
    }

    /// The slot's colour with any alpha dropped.
    pub fn slot(&self, s: Slot) -> Option<Rgb> {
        self.raw(s).map(to_rgb)
    }
}

impl crate::derive::TermColors {
    pub fn to_scheme(&self, name: &str) -> ColorScheme {
        ColorScheme {
            name: name.to_string(),
            foreground: Some(to_hex(self.foreground)),
            background: self.background.map(to_hex),
            cursor: Some(to_hex(self.cursor)),
            bright_foreground: Some(to_hex(self.bright_foreground)),
            dim_foreground: Some(to_hex(self.dim_foreground)),
            normal: AnsiScheme::from_set(&self.normal),
            bright: AnsiScheme::from_set(&self.bright),
            dim: AnsiScheme::from_set(&self.dim),
        }
    }
}
