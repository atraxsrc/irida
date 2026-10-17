use cosmic_theme::palette::{FromColor, Oklab, Srgb};
use irida_core::color::Rgb;
use irida_core::ramp::{RampError, ramp};

fn lightness(c: Rgb) -> f32 {
    let s = Srgb::new(c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
    Oklab::from_color(s).l
}

#[test]
fn endpoints_and_length() {
    let a = Rgb::from_hex("#1A2B3C").unwrap();
    let b = Rgb::from_hex("#F0E0D0").unwrap();
    let r = ramp(a, b, 8).unwrap();
    assert_eq!(r.len(), 8);
    assert_eq!(r[0], a);
    assert_eq!(r[7], b);
}

#[test]
fn black_to_white_is_monotonic_in_lightness() {
    let r = ramp(
        Rgb::from_hex("#000000").unwrap(),
        Rgb::from_hex("#FFFFFF").unwrap(),
        5,
    )
    .unwrap();
    let ls: Vec<f32> = r.iter().map(|c| lightness(*c)).collect();
    assert!(ls.windows(2).all(|w| w[0] < w[1]), "{ls:?}");
}

#[test]
fn step_count_is_bounded() {
    let a = Rgb::from_hex("#000000").unwrap();
    let b = Rgb::from_hex("#FFFFFF").unwrap();
    assert!(matches!(ramp(a, b, 1), Err(RampError::Steps(_))));
    assert!(matches!(ramp(a, b, 17), Err(RampError::Steps(_))));
}

#[test]
fn out_of_gamut_midpoints_do_not_panic() {
    let r = ramp(
        Rgb::from_hex("#FF0000").unwrap(),
        Rgb::from_hex("#00FFFF").unwrap(),
        16,
    )
    .unwrap();
    assert_eq!(r.len(), 16);
}
