//! The sRGB colour type used throughout Irida.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ColorError {
    #[error("invalid hex colour: {0:?}")]
    InvalidHex(String),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const WHITE: Rgb = Rgb { r: 255, g: 255, b: 255 };
pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

impl Rgb {
    /// Parse `#RRGGBB` or `RRGGBB`, in any case.
    pub fn from_hex(s: &str) -> Result<Rgb, ColorError> {
        let err = || ColorError::InvalidHex(s.to_string());
        let digits = s.strip_prefix('#').unwrap_or(s);
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let ch = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).map_err(|_| err());
        Ok(Rgb { r: ch(0)?, g: ch(2)?, b: ch(4)? })
    }

    /// Uppercase `#RRGGBB`.
    pub fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for Rgb {
    type Err = ColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Rgb::from_hex(s)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// Linear blend per channel: `a + (b - a) * t`, rounded half to even.
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let ch = |a: u8, b: u8| {
        (a as f64 + (b as f64 - a as f64) * t)
            .round_ties_even()
            .clamp(0.0, 255.0) as u8
    };
    Rgb { r: ch(a.r, b.r), g: ch(a.g, b.g), b: ch(a.b, b.b) }
}
