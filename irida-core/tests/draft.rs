mod common;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

use common::fixture_text;
use irida_core::color::Rgb;
use irida_core::contrast;
use irida_core::draft::{Draft, clusters, draft, draft_palette, mode_of};
use irida_core::palette::Mode;
use irida_core::role::Role;

const CASES: [&str; 5] = ["two-tone", "flat", "grey", "noise", "pastel"];

fn vectors() -> Value {
    serde_json::from_str(&fixture_text("draft/vectors.json")).expect("vectors.json parses")
}

fn pixels(case: &Value) -> Vec<u8> {
    let rgba = STANDARD
        .decode(case["rgba"].as_str().expect("rgba is a string"))
        .expect("rgba is base64");
    assert_eq!(rgba.len(), 4000, "1000 RGBA pixels");
    rgba
}

fn hex(v: &Value) -> Rgb {
    Rgb::from_hex(v.as_str().expect("hex is a string")).expect("hex parses")
}

fn near(got: Rgb, want: Rgb) -> bool {
    got.r.abs_diff(want.r) <= 1 && got.g.abs_diff(want.g) <= 1 && got.b.abs_diff(want.b) <= 1
}

fn mode_str(m: Mode) -> &'static str {
    match m {
        Mode::Dark => "dark",
        Mode::Light => "light",
    }
}

#[test]
fn matches_reference_vectors() {
    let v = vectors();
    for name in CASES {
        let case = &v[name];
        let cl = clusters(&pixels(case));
        let want = case["clusters"].as_array().expect("clusters array");
        assert_eq!(cl.len(), want.len(), "{name}: cluster count");
        for (i, (got, w)) in cl.iter().zip(want).enumerate() {
            let wh = hex(&w["hex"]);
            assert!(
                near(got.rgb, wh),
                "{name}: cluster {i} is {} not {}",
                got.rgb,
                wh
            );
            let ww = w["weight"].as_f64().expect("weight");
            assert!(
                (got.weight - ww).abs() < 1e-6,
                "{name}: cluster {i} weight {} not {ww}",
                got.weight
            );
        }
        assert_eq!(
            mode_str(mode_of(&cl)),
            case["mode"].as_str().expect("mode"),
            "{name}: mode"
        );
        for mode in [Mode::Dark, Mode::Light] {
            let d = draft(&cl, mode);
            let exp = &case[mode_str(mode)];
            for role in Role::ALL {
                let wr = hex(&exp["roles"][role.as_str()]);
                let gr = d.roles.get(role);
                assert!(near(gr, wr), "{name} {mode}: {role} is {gr} not {wr}");
            }
            match &exp["accentFrom"] {
                Value::Null => assert_eq!(d.accent_from, None, "{name} {mode}: accent_from"),
                h => {
                    let i = d
                        .accent_from
                        .unwrap_or_else(|| panic!("{name} {mode}: accent_from is None"));
                    assert_eq!(cl[i].rgb, hex(h), "{name} {mode}: accent_from cluster");
                }
            }
        }
    }
}

#[test]
fn every_draft_passes() {
    let v = vectors();
    for name in CASES {
        let cl = clusters(&pixels(&v[name]));
        for mode in [Mode::Dark, Mode::Light] {
            let p = draft_palette(&cl, mode, "Two Tone");
            assert_eq!(p.slug, "twotone");
            assert_eq!(p.mode, mode);
            assert!(p.pins.is_empty() && p.pair.is_none() && p.names.is_empty());
            p.validate().expect("draft palette is valid");
            let report = contrast::check(&p);
            let fails: Vec<Role> = report.failures().iter().map(|r| r.role).collect();
            assert!(fails.is_empty(), "{name} {mode}: {fails:?} fail");
        }
    }
}

#[test]
fn deterministic() {
    let v = vectors();
    for name in CASES {
        let px = pixels(&v[name]);
        let (a, b) = (clusters(&px), clusters(&px));
        assert_eq!(a, b, "{name}: clusters");
        for mode in [Mode::Dark, Mode::Light] {
            let (da, db): (Draft, Draft) = (draft(&a, mode), draft(&b, mode));
            assert_eq!(da, db, "{name} {mode}: draft");
            assert_eq!(
                draft_palette(&a, mode, "Same"),
                draft_palette(&b, mode, "Same")
            );
        }
    }
}

#[test]
fn transparent_pixels_ignored() {
    let v = vectors();
    let px = pixels(&v["noise"]);
    // Splice half-transparent pixels of a colour not in the image between
    // the opaque ones.
    let mut spliced = Vec::new();
    for (i, p) in px.chunks(4).enumerate() {
        spliced.extend_from_slice(p);
        if i % 3 == 0 {
            spliced.extend_from_slice(&[255, 0, 255, 127]);
        }
    }
    spliced.extend_from_slice(&[0, 255, 0, 0]);
    assert_eq!(clusters(&spliced), clusters(&px));

    let none = [10u8, 200, 30, 127].repeat(500);
    assert!(clusters(&none).is_empty());
    assert!(clusters(&[]).is_empty());

    let grey = clusters(&[128u8, 128, 128, 255].repeat(100));
    assert_eq!(grey.len(), 1);
    for mode in [Mode::Dark, Mode::Light] {
        let d = draft(&[], mode);
        assert_eq!(d.accent_from, None);
        assert_eq!(
            d,
            draft(&grey, mode),
            "{mode}: empty draft is the grey draft"
        );
        // The grey vector's accent comes from hue 44, chroma 0.15 too.
        let want = hex(&v["grey"][mode_str(mode)]["roles"]["accent"]);
        assert!(near(d.roles.get(Role::Accent), want), "{mode}: accent");
    }
    assert_eq!(mode_of(&[]), Mode::Dark);
}
