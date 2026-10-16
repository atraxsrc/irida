mod common;

use irida_core::palette::{Mode, Palette, PaletteError, validate_slug};
use irida_core::role::Role;

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
    assert_eq!(Palette::from_toml(&text), Err(PaletteError::MissingRole(Role::Cyan)));
}

#[test]
fn unknown_role_is_named() {
    let text = fixture_text("palettes/ember.toml")
        .replace("[roles]\n", "[roles]\norange = \"#FF8800\"\n");
    assert!(matches!(
        Palette::from_toml(&text),
        Err(PaletteError::UnknownRole(ref s)) if s == "orange"
    ));
}

#[test]
fn slug_rules() {
    assert!(validate_slug("ember").is_ok());
    for bad in ["Ember", "ember-day", "ember2", ""] {
        assert!(matches!(validate_slug(bad), Err(PaletteError::BadSlug(_))), "{bad}");
    }
    let text = fixture_text("palettes/ember.toml").replace("slug = \"ember\"", "slug = \"ember-day\"");
    assert!(matches!(Palette::from_toml(&text), Err(PaletteError::BadSlug(_))));
}

#[test]
fn minimums_and_display() {
    assert_eq!(Role::Text.min_contrast(), Some(4.5));
    assert_eq!(Role::Muted.min_contrast(), Some(3.0));
    assert_eq!(Role::Comment.min_contrast(), None);
    assert_eq!(Role::Pane.min_contrast(), None);
    assert_eq!(format!("{:<8}|", Role::Error), "error   |");
}
