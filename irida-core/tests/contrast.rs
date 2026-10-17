mod common;

use common::palette;
use irida_core::color::{BLACK, Rgb, WHITE};
use irida_core::contrast::{check, ratio};
use irida_core::role::Role;

fn table(slug: &str) -> Vec<(Role, String)> {
    check(&palette(slug))
        .rows
        .iter()
        .map(|r| (r.role, format!("{:.1}/{:.1}", r.on_background, r.on_pane)))
        .collect()
}

fn assert_table(slug: &str, expected: &[(Role, &str)]) {
    let got = table(slug);
    for (role, want) in expected {
        let row = got.iter().find(|(r, _)| r == role).expect("role row");
        assert_eq!(&row.1, want, "{slug} {role}");
    }
    assert_eq!(got.len(), 10);
}

fn failing(slug: &str) -> Vec<Role> {
    let p = palette(slug);
    let report = check(&p);
    for f in report.failures() {
        assert!(
            !f.fails_on_background(),
            "{slug} {} fails on background",
            f.role
        );
        assert!(f.fails_on_pane(), "{slug} {} should fail on pane", f.role);
    }
    report.failures().iter().map(|f| f.role).collect()
}

#[test]
fn known_pairs() {
    assert!((ratio(WHITE, BLACK) - 21.0).abs() < 1e-9);
    let grey = Rgb::from_hex("#777777").unwrap();
    assert_eq!(format!("{:.3}", ratio(grey, WHITE)), "4.478");
    assert_eq!(ratio(grey, BLACK), ratio(BLACK, grey));
}

#[test]
fn ember_matches_contrast_py() {
    use Role::*;
    assert_table(
        "ember",
        &[
            (Text, "12.7/11.8"),
            (Accent, "5.9/5.4"),
            (Success, "7.8/7.3"),
            (Warning, "8.0/7.4"),
            (Error, "4.7/4.4"),
            (Blue, "6.8/6.3"),
            (Magenta, "6.3/5.8"),
            (Cyan, "8.0/7.4"),
            (Muted, "4.7/4.3"),
            (Comment, "2.9/2.7"),
        ],
    );
}

#[test]
fn emberday_matches_contrast_py() {
    use Role::*;
    assert_table(
        "emberday",
        &[
            (Text, "10.9/9.6"),
            (Accent, "4.9/4.4"),
            (Success, "4.8/4.3"),
            (Warning, "4.6/4.0"),
            (Error, "5.4/4.8"),
            (Blue, "5.6/4.9"),
            (Magenta, "5.4/4.8"),
            (Cyan, "4.7/4.2"),
            (Muted, "4.0/3.6"),
            (Comment, "2.6/2.3"),
        ],
    );
}

#[test]
fn noir_dossier_fixture_table() {
    use Role::*;
    assert_table(
        "noir",
        &[
            (Text, "11.5/11.5"),
            (Muted, "4.6/4.6"),
            (Comment, "3.4/3.4"),
            (Accent, "5.2/5.2"),
            (Success, "6.6/6.6"),
            (Warning, "6.1/6.1"),
            (Error, "5.2/5.2"),
            (Blue, "6.1/6.1"),
            (Magenta, "6.1/6.1"),
            (Cyan, "4.6/4.6"),
        ],
    );
    assert_table(
        "dossier",
        &[
            (Text, "10.0/9.1"),
            (Muted, "4.7/4.3"),
            (Comment, "3.2/2.9"),
            (Accent, "4.6/4.2"),
            (Success, "4.9/4.5"),
            (Warning, "4.6/4.2"),
            (Error, "5.0/4.6"),
            (Blue, "6.5/6.0"),
            (Magenta, "4.6/4.2"),
            (Cyan, "7.3/6.6"),
        ],
    );
}

#[test]
fn verdicts_follow_section_3() {
    use Role::*;
    assert_eq!(failing("ember"), vec![Error]);
    assert_eq!(failing("emberday"), vec![Accent, Success, Warning, Cyan]);
    assert_eq!(failing("noir"), Vec::<Role>::new());
    assert_eq!(failing("dossier"), vec![Accent, Success, Warning, Magenta]);
}

#[test]
fn raw_ratio_decides() {
    let report = check(&palette("dossier"));
    let row = report
        .rows
        .iter()
        .find(|r| r.role == Role::Success)
        .unwrap();
    assert_eq!(format!("{:.4}", row.on_pane), "4.4516");
    assert_eq!(format!("{:.1}", row.on_pane), "4.5");
    assert!(row.fails_on_pane());
    assert!(!row.passes());
}

#[test]
fn comment_never_fails() {
    let mut p = palette("ember");
    let bg = p.roles.get(Role::Background);
    p.roles.set(Role::Comment, bg);
    let report = check(&p);
    let row = report
        .rows
        .iter()
        .find(|r| r.role == Role::Comment)
        .unwrap();
    assert!(row.passes());
    assert!(report.failures().iter().all(|f| f.role != Role::Comment));
}
