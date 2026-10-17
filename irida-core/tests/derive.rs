mod common;

use irida_core::color::Rgb;
use irida_core::derive::derive;
use irida_core::palette::{Palette, PaletteError};
use irida_core::slot::{PinValue, Slot};
use irida_core::term::ColorScheme;

use common::{fixture_text, palette};

fn hex(s: &str) -> Rgb {
    Rgb::from_hex(s).unwrap()
}

fn slot(s: &str) -> Slot {
    s.parse().unwrap()
}

fn scheme(file: &str) -> ColorScheme {
    ColorScheme::from_ron(&fixture_text(file)).unwrap()
}

fn close(a: Rgb, b: Rgb) -> bool {
    a.r.abs_diff(b.r) <= 2 && a.g.abs_diff(b.g) <= 2 && a.b.abs_diff(b.b) <= 2
}

#[test]
fn ember_rule_within_two() {
    let mut p = palette("ember");
    p.pins.clear();
    let d = derive(&p);
    let want = scheme("rices/Ember-term.ron");
    let mut differing = Vec::new();
    for s in Slot::ALL_TERM {
        let got = d.slot(s).unwrap();
        let exp = want.slot(s).unwrap();
        assert!(close(got, exp), "{s}: {got} vs {exp}");
        if got != exp {
            differing.push(s.to_string());
        }
    }
    differing.sort();
    assert_eq!(differing, ["dim.black", "normal.black"]);
    assert_eq!(d.slot(slot("normal.black")).unwrap(), hex("#2B2F37"));
    assert_eq!(d.slot(slot("dim.black")).unwrap(), hex("#25282F"));
}

#[test]
fn ember_with_pin_is_exact() {
    let d = derive(&palette("ember"));
    let want = scheme("rices/Ember-term.ron");
    for s in Slot::ALL_TERM {
        assert_eq!(d.slot(s), want.slot(s), "{s}");
    }
}

#[test]
fn emberday_is_exact() {
    let d = derive(&palette("emberday"));
    let want = scheme("rices/EmberDay-term.ron");
    for s in Slot::ALL_TERM {
        assert_eq!(d.slot(s), want.slot(s), "{s}");
    }
}

#[test]
fn pins_propagate() {
    let mut p = palette("ember");
    p.pins
        .set(slot("normal.red"), PinValue::Color(hex("#FF0000")))
        .expect("colour pins are always allowed");
    let d = derive(&p);
    assert_eq!(d.slot(slot("dim.red")).unwrap(), hex("#A40C0E"));
    assert_eq!(d.slot(slot("bright.red")).unwrap(), hex("#FF1F1F"));

    let mut p = palette("ember");
    p.pins
        .set(slot("bright.white"), PinValue::Color(hex("#FFFFFF")))
        .expect("colour pins are always allowed");
    assert_eq!(derive(&p).term.bright_foreground, hex("#FFFFFF"));

    let mut p = palette("ember");
    p.pins
        .set(slot("normal.white"), PinValue::Color(hex("#FFFFFF")))
        .expect("colour pins are always allowed");
    assert_eq!(
        derive(&p).slot(slot("bright.white")).unwrap(),
        hex("#E2E4E9")
    );

    let mut p = palette("emberday");
    p.pins
        .set(slot("bright.black"), PinValue::Color(hex("#000000")))
        .expect("colour pins are always allowed");
    assert_eq!(derive(&p).term.bright_foreground, hex("#000000"));
}

#[test]
fn background_none() {
    let mut p = palette("ember");
    p.pins
        .set(slot("background"), PinValue::None)
        .expect("background can be none");
    let d = derive(&p);
    assert_eq!(d.term.background, None);
    assert_eq!(d.term.to_scheme("x").background, None);
}

#[test]
fn cosmic_defaults() {
    let d = derive(&palette("ember"));
    assert_eq!(d.neutral_10, hex("#E6E8EE"));
    assert_eq!(d.gray_2, hex("#2A2F38"));
    let d = derive(&palette("emberday"));
    assert_eq!(d.neutral_10, hex("#2B2E36"));
    assert_eq!(d.gray_2, hex("#DDD9D1"));
}

#[test]
fn pins_toml_round_trip() {
    let p = palette("ember");
    assert!(p.pins.is_pinned(slot("normal.black")));
    assert_eq!(Palette::from_toml(&p.to_toml()).unwrap(), p);

    let base = fixture_text("palettes/ember.toml");
    for bad in [
        r##""dim.blue" = "none""##,
        r##""bright.orange" = "#FFFFFF""##,
    ] {
        let text = base.replace(r##""normal.black" = "#2A2F38""##, bad);
        assert!(
            matches!(
                Palette::from_toml(&text),
                Err(PaletteError::BadColor { .. })
            ),
            "{bad}"
        );
    }
}
