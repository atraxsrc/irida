use irida_core::color::{mix, Rgb, BLACK, WHITE};

#[test]
fn hex_round_trip() {
    let c = Rgb::from_hex("#e8713a").unwrap();
    assert_eq!(c, Rgb { r: 0xE8, g: 0x71, b: 0x3A });
    assert_eq!(c.to_hex(), "#E8713A");
    assert_eq!(Rgb::from_hex("E8713A").unwrap(), c);
}

#[test]
fn bad_hex_is_an_error() {
    for s in ["", "#12345", "#1234567", "#GGGGGG", "#E8713AFF"] {
        assert!(Rgb::from_hex(s).is_err(), "{s}");
    }
}

#[test]
fn mix_rounds_half_to_even() {
    // Ember Day fastfetch box line: mix(text, pane, 0.70); blue is exactly 162.5.
    let text = Rgb::from_hex("#2B2E36").unwrap();
    let pane = Rgb::from_hex("#DDD9D1").unwrap();
    assert_eq!(mix(text, pane, 0.70).to_hex(), "#A8A6A2");
}

#[test]
fn mix_endpoints_and_derivation_examples() {
    let red = Rgb::from_hex("#D85A5A").unwrap();
    assert_eq!(mix(red, WHITE, 0.0), red);
    assert_eq!(mix(red, WHITE, 1.0), WHITE);
    assert_eq!(mix(red, WHITE, 0.12).to_hex(), "#DD6E6E");
    assert_eq!(mix(Rgb::from_hex("#9E3A3A").unwrap(), BLACK, 0.15).to_hex(), "#863131");
}
