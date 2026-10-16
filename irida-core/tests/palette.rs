mod common;

use irida_core::color::Rgb;
use irida_core::palette::{
    Mode, Palette, PaletteError, Roles, slug_from_name, validate_name, validate_slug,
};
use irida_core::role::Role;
use irida_core::slot::{PinError, PinValue, Pins, Slot};

use common::{fixture_text, palette};

const SLUGS: [&str; 4] = ["ember", "emberday", "noir", "dossier"];

#[test]
fn loads_all_fixtures() {
    for s in SLUGS {
        palette(s);
    }
    assert_eq!(palette("ember").roles.get(Role::Accent).to_hex(), "#E8713A");
    assert_eq!(palette("emberday").mode, Mode::Light);
    assert_eq!(palette("emberday").stem(), "EmberDay");
    assert_eq!(palette("ember").names[&Role::Background], "Night");
}

#[test]
fn toml_round_trip() {
    for s in SLUGS {
        let p = palette(s);
        assert_eq!(Palette::from_toml(&p.to_toml()).unwrap(), p);
    }
}

#[test]
fn missing_role_is_named() {
    let text: String = fixture_text("palettes/ember.toml")
        .lines()
        .filter(|l| !l.starts_with("cyan ="))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(
        Palette::from_toml(&text),
        Err(PaletteError::MissingRole(Role::Cyan))
    );
}

#[test]
fn unknown_role_is_named() {
    let text =
        fixture_text("palettes/ember.toml").replace("[roles]\n", "[roles]\norange = \"#FF8800\"\n");
    assert!(matches!(
        Palette::from_toml(&text),
        Err(PaletteError::UnknownRole(ref s)) if s == "orange"
    ));
}

#[test]
fn slug_rules() {
    assert!(validate_slug("ember").is_ok());
    for bad in ["Ember", "ember-day", "ember2", ""] {
        assert!(
            matches!(validate_slug(bad), Err(PaletteError::BadSlug(_))),
            "{bad}"
        );
    }
    let text =
        fixture_text("palettes/ember.toml").replace("slug = \"ember\"", "slug = \"ember-day\"");
    assert!(matches!(
        Palette::from_toml(&text),
        Err(PaletteError::BadSlug(_))
    ));
}

#[test]
fn minimums_and_display() {
    assert_eq!(Role::Text.min_contrast(), Some(4.5));
    assert_eq!(Role::Muted.min_contrast(), Some(3.0));
    assert_eq!(Role::Comment.min_contrast(), None);
    assert_eq!(Role::Pane.min_contrast(), None);
    assert_eq!(format!("{:<8}|", Role::Error), "error   |");
}

#[test]
fn name_rules() {
    for good in ["Ember", "Ember Day", "Noir 2"] {
        assert!(validate_name(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        " Ember",
        "Ember ",
        "Ember  Day",
        "Ember-Day",
        "Ember\"s",
        "\u{c9}mber",
        "a\nb",
    ] {
        assert!(
            matches!(validate_name(bad), Err(PaletteError::BadName(_))),
            "{bad:?}"
        );
    }
    let text =
        fixture_text("palettes/ember.toml").replace("name = \"Ember\"", "name = \"Ember-Day\"");
    assert!(matches!(
        Palette::from_toml(&text),
        Err(PaletteError::BadName(_))
    ));
}

#[test]
fn roles_new() {
    let p = palette("ember");
    let colors = Role::ALL.map(|r| p.roles.get(r));
    assert_eq!(Roles::new(colors), p.roles);
}

#[test]
fn pins_none_only_for_background() {
    let mut pins = Pins::default();
    assert_eq!(pins.set(Slot::Background, PinValue::None), Ok(()));
    assert_eq!(pins.get(Slot::Background), Some(PinValue::None));

    let red = PinValue::Color(Rgb::from_hex("#FF0000").unwrap());
    pins.set(Slot::Foreground, red)
        .expect("colour pins are always allowed");
    let before = pins.clone();
    let err = pins.set(Slot::Foreground, PinValue::None).unwrap_err();
    assert_eq!(err, PinError::NoneNotAllowed(Slot::Foreground));
    assert_eq!(
        err.to_string(),
        "foreground cannot be none; only background can"
    );
    assert_eq!(pins, before);
    assert_eq!(pins.get(Slot::Foreground), Some(red));

    let mut empty = Pins::default();
    assert!(empty.set(Slot::Cursor, PinValue::None).is_err());
    assert!(empty.is_empty());
}

#[test]
fn validate_rules() {
    for s in SLUGS {
        assert_eq!(palette(s).validate(), Ok(()), "{s}");
    }
    let mut paired = palette("ember");
    paired.pair = Some("emberday".into());
    assert_eq!(paired.validate(), Ok(()));

    let mut p = palette("ember");
    p.name = "Ember-Day".into();
    let err = p.validate().unwrap_err();
    assert!(matches!(err, PaletteError::BadName(_)), "{err:?}");
    assert!(err.to_string().contains("name \"Ember-Day\""), "{err}");

    let mut p = palette("ember");
    p.slug = "ember2".into();
    let err = p.validate().unwrap_err();
    assert!(matches!(err, PaletteError::BadSlug(_)), "{err:?}");
    assert!(err.to_string().contains("slug \"ember2\""), "{err}");

    let mut p = palette("ember");
    p.pair = Some("ember".into());
    let err = p.validate().unwrap_err();
    assert!(matches!(err, PaletteError::BadPair(_)), "{err:?}");
    assert!(err.to_string().contains("pair \"ember\""), "{err}");

    let mut p = palette("ember");
    p.pair = Some("ember-day".into());
    assert!(matches!(p.validate(), Err(PaletteError::BadPair(_))));

    let mut p = palette("ember");
    p.names.insert(Role::Accent, "  ".into());
    let err = p.validate().unwrap_err();
    assert_eq!(err, PaletteError::EmptyRoleName(Role::Accent));
    assert!(err.to_string().contains("names.accent"), "{err}");
}

#[test]
fn slug_from_name_cases() {
    assert_eq!(slug_from_name("Ember Day"), "emberday");
    assert_eq!(slug_from_name("Neon 80s"), "neons");
    assert_eq!(slug_from_name("42"), "");
}
