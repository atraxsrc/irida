//! Perceptual colour ramps interpolated in OKLab.

use cosmic_theme::palette::{FromColor, Oklab, Srgb};

use crate::color::Rgb;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RampError {
    #[error("ramp steps must be between 2 and 16, got {0}")]
    Steps(usize),
}

fn to_oklab(c: Rgb) -> Oklab {
    Oklab::from_color(Srgb::new(c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0))
}

fn to_rgb(lab: Oklab) -> Rgb {
    let s = Srgb::from_color(lab);
    let ch = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8;
    Rgb { r: ch(s.red), g: ch(s.green), b: ch(s.blue) }
}

/// `steps` colours from `from` to `to`, interpolated linearly in OKLab.
pub fn ramp(from: Rgb, to: Rgb, steps: usize) -> Result<Vec<Rgb>, RampError> {
    if !(2..=16).contains(&steps) {
        return Err(RampError::Steps(steps));
    }
    let (a, b) = (to_oklab(from), to_oklab(to));
    Ok((0..steps)
        .map(|i| {
            if i == 0 {
                return from;
            }
            if i == steps - 1 {
                return to;
            }
            let t = i as f32 / (steps - 1) as f32;
            to_rgb(Oklab::new(
                a.l + (b.l - a.l) * t,
                a.a + (b.a - a.a) * t,
                a.b + (b.b - a.b) * t,
            ))
        })
        .collect())
}
