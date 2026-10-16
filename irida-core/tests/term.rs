mod common;

use irida_core::term::ColorScheme;

use common::fixture_text;

const FILES: [&str; 4] = [
    "rices/Ember-term.ron",
    "rices/EmberDay-term.ron",
    "rices/Noir-term.ron",
    "rices/Dossier-term.ron",
];

#[test]
fn fixtures_round_trip() {
    for f in FILES {
        let text = fixture_text(f);
        let out = ColorScheme::from_ron(&text).unwrap().to_ron();
        assert_eq!(out, text.strip_suffix('\n').unwrap_or(&text), "{f}");
        assert!(!out.contains("Some("), "{f}");
    }
}

#[test]
fn noir_has_no_background() {
    let s = ColorScheme::from_ron(&fixture_text("rices/Noir-term.ron")).unwrap();
    assert_eq!(s.background, None);
}

#[test]
fn unknown_field_is_rejected() {
    let text = fixture_text("rices/Ember-term.ron");
    let bad = text.replacen("    name: \"Ember\",\n", "    name: \"Ember\",\n    italic: \"#FFFFFF\",\n", 1);
    assert_ne!(bad, text);
    assert!(ColorScheme::from_ron(&bad).is_err());
}
