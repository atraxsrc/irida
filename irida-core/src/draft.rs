//! Drafting a palette from wallpaper colours.
//!
//! A port of `docs/superpowers/specs/2026-10-16-wallpaper-draft.js`, function
//! by function, so its vectors match: pixels to OKLab, a seeded k-means with
//! k = 8, then 13 roles whose lightness and chroma come from Ember (dark) and
//! Ember Day (light). The wallpaper only decides the neutral cast, the accent
//! and how far each semantic hue leans toward a nearby wallpaper colour. Every
//! role with a minimum is moved in lightness until it passes on both surfaces.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::color::{BLACK, Rgb};
use crate::contrast::ratio;
use crate::palette::{Mode, Palette, Roles, slug_from_name};
use crate::role::Role;
use crate::slot::Pins;

const K: usize = 8;
const ITERATIONS: usize = 14;
const SEED: u32 = 0x9e37_79b9;

/// One k-means cluster: its colour (gamut-mapped), OKLCH lightness, chroma
/// and hue in degrees, and its share of the opaque pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    pub rgb: Rgb,
    pub l: f64,
    pub c: f64,
    pub h: f64,
    pub weight: f64,
}

/// A drafted set of roles. `accent_from` indexes the clusters the draft was
/// made from: the cluster the accent hue came from, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub roles: Roles,
    pub accent_from: Option<usize>,
}

fn to_lin(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

type Lab = [f64; 3];

fn oklab(r: u8, g: u8, b: u8) -> Lab {
    let (r, g, b) = (
        to_lin(r as f64 / 255.0),
        to_lin(g as f64 / 255.0),
        to_lin(b as f64 / 255.0),
    );
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn lin_rgb(l: f64, a: f64, b: f64) -> [f64; 3] {
    let lc = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let mc = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let sc = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        4.0767416621 * lc - 3.3077115913 * mc + 0.2309699292 * sc,
        -1.2684380046 * lc + 2.6097574011 * mc - 0.3413193965 * sc,
        -0.0041960863 * lc - 0.7034186147 * mc + 1.7076147010 * sc,
    ]
}

fn in_gamut(c: &[f64; 3]) -> bool {
    c.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v))
}

/// OKLCH to sRGB, lowering chroma (18 bisection steps) until the colour fits.
fn hex_of(l: f64, c: f64, h: f64) -> Rgb {
    let rad = h * PI / 180.0;
    let at = |c: f64| lin_rgb(l, c * rad.cos(), c * rad.sin());
    let mut best = lin_rgb(l, 0.0, 0.0);
    let full = at(c);
    if in_gamut(&full) {
        best = full;
    } else {
        let (mut lo, mut hi) = (0.0, c);
        for _ in 0..18 {
            let mid = (lo + hi) / 2.0;
            let m = at(mid);
            if in_gamut(&m) {
                lo = mid;
                best = m;
            } else {
                hi = mid;
            }
        }
    }
    let byte = |v: f64| (to_srgb(v.clamp(0.0, 1.0)).clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb {
        r: byte(best[0]),
        g: byte(best[1]),
        b: byte(best[2]),
    }
}

fn hue_of(a: f64, b: f64) -> f64 {
    (b.atan2(a) * 180.0 / PI + 360.0) % 360.0
}

fn hue_dist(x: f64, y: f64) -> f64 {
    let d = (x - y).abs() % 360.0;
    if d > 180.0 { 360.0 - d } else { d }
}

fn lerp_hue(from: f64, to: f64, t: f64) -> f64 {
    let d = ((to - from + 540.0) % 360.0) - 180.0;
    (from + d * t + 360.0) % 360.0
}

fn d2(p: &Lab, q: &Lab) -> f64 {
    (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
}

/// k-means (k = 8, 14 iterations, k-means++ from a fixed seed) in OKLab over
/// RGBA bytes, skipping pixels with alpha under 128. Clusters with no pixels
/// are dropped; the rest are sorted by weight, heaviest first, stable.
pub fn clusters(rgba: &[u8]) -> Vec<Cluster> {
    let pts: Vec<Lab> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] >= 128)
        .map(|p| oklab(p[0], p[1], p[2]))
        .collect();
    if pts.is_empty() {
        return Vec::new();
    }
    let mut seed = SEED;
    let mut rand = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        seed as f64 / 4294967296.0
    };

    let first = (rand() * pts.len() as f64).floor() as usize;
    let mut cents: Vec<Lab> = vec![pts[first]];
    while cents.len() < K.min(pts.len()) {
        let w: Vec<f64> = pts
            .iter()
            .map(|p| cents.iter().map(|c| d2(p, c)).fold(f64::INFINITY, f64::min))
            .collect();
        let total = w.iter().fold(0.0, |s, v| s + v);
        if total == 0.0 {
            break;
        }
        let mut t = rand() * total;
        let mut i = 0;
        while t > w[i] && i < w.len() - 1 {
            t -= w[i];
            i += 1;
        }
        cents.push(pts[i]);
    }

    let mut assign = vec![0usize; pts.len()];
    for _ in 0..ITERATIONS {
        for (slot, p) in assign.iter_mut().zip(&pts) {
            let (mut best, mut bd) = (0, f64::INFINITY);
            for (c, cent) in cents.iter().enumerate() {
                let d = d2(p, cent);
                if d < bd {
                    bd = d;
                    best = c;
                }
            }
            *slot = best;
        }
        let mut sum = vec![[0.0f64; 4]; cents.len()];
        for (&c, p) in assign.iter().zip(&pts) {
            let s = &mut sum[c];
            s[0] += p[0];
            s[1] += p[1];
            s[2] += p[2];
            s[3] += 1.0;
        }
        for (cent, s) in cents.iter_mut().zip(&sum) {
            if s[3] != 0.0 {
                *cent = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
            }
        }
    }

    let mut count = vec![0usize; cents.len()];
    for &c in &assign {
        count[c] += 1;
    }
    let mut out: Vec<Cluster> = cents
        .iter()
        .zip(&count)
        .map(|(cent, &n)| {
            let (l, c, h) = (cent[0], cent[1].hypot(cent[2]), hue_of(cent[1], cent[2]));
            Cluster {
                rgb: hex_of(l, c, h),
                l,
                c,
                h,
                weight: n as f64 / pts.len() as f64,
            }
        })
        .filter(|c| c.weight > 0.0)
        .collect();
    out.sort_by(|x, y| y.weight.total_cmp(&x.weight));
    out
}

/// Dark when the weighted mean lightness is under 0.55, else light.
pub fn mode_of(clusters: &[Cluster]) -> Mode {
    let l = clusters.iter().fold(0.0, |s, c| s + c.l * c.weight);
    if l < 0.55 { Mode::Dark } else { Mode::Light }
}

struct Style {
    background: f64,
    pane: f64,
    container: f64,
    text: f64,
    muted: f64,
    comment: f64,
    accent: f64,
    sem: f64,
}

const DARK: Style = Style {
    background: 0.205,
    pane: 0.235,
    container: 0.268,
    text: 0.885,
    muted: 0.606,
    comment: 0.49,
    accent: 0.68,
    sem: 0.70,
};

const LIGHT: Style = Style {
    background: 0.925,
    pane: 0.886,
    container: 0.902,
    text: 0.302,
    muted: 0.541,
    comment: 0.653,
    accent: 0.51,
    sem: 0.49,
};

/// Semantic roles: default hue and chroma.
const SEM: [(Role, f64, f64); 6] = [
    (Role::Error, 23.0, 0.155),
    (Role::Warning, 78.0, 0.125),
    (Role::Success, 141.0, 0.075),
    (Role::Cyan, 190.0, 0.062),
    (Role::Blue, 250.0, 0.075),
    (Role::Magenta, 310.0, 0.095),
];

/// Draft the 13 roles for a mode (spec steps 4 to 8). No clusters gives the
/// grey-wallpaper draft: no cast, accent hue 44, chroma 0.15.
pub fn draft(clusters: &[Cluster], mode: Mode) -> Draft {
    let (st, dark) = match mode {
        Mode::Dark => (&DARK, true),
        Mode::Light => (&LIGHT, false),
    };
    // Neutral cast: the weighted mean of a and b, kept faint.
    let a = clusters
        .iter()
        .fold(0.0, |s, c| s + c.c * (c.h * PI / 180.0).cos() * c.weight);
    let b = clusters
        .iter()
        .fold(0.0, |s, c| s + c.c * (c.h * PI / 180.0).sin() * c.weight);
    let cast_c = a.hypot(b).min(0.018);
    let cast_h = hue_of(a, b);

    let mut roles = Roles::new([BLACK; 13]);
    let background = hex_of(st.background, cast_c * 0.8, cast_h);
    let pane = hex_of(st.pane, cast_c, cast_h);
    roles.set(Role::Background, background);
    roles.set(Role::Pane, pane);
    roles.set(Role::Container, hex_of(st.container, cast_c, cast_h));

    // Move away from the surfaces until both ratios reach min plus a hair,
    // as check rounds down.
    let fit = |mut l: f64, c: f64, h: f64, min: f64| {
        let mut hex = hex_of(l, c, h);
        for _ in 0..120 {
            if ratio(hex, background).min(ratio(hex, pane)) >= min + 0.02 {
                break;
            }
            l = if dark {
                (l + 0.005).min(0.99)
            } else {
                (l - 0.005).max(0.02)
            };
            hex = hex_of(l, c, h);
        }
        hex
    };
    roles.set(Role::Text, fit(st.text, cast_c * 0.8, cast_h, 4.5));
    roles.set(Role::Muted, fit(st.muted, cast_c * 1.4, cast_h, 3.0));
    roles.set(Role::Comment, hex_of(st.comment, cast_c * 1.6, cast_h));

    // Accent: the most vivid colour that covers some of the wallpaper. On a
    // tie the first in weight order wins, as the reference's stable sort.
    let mut vivid: Option<(usize, f64)> = None;
    for (i, c) in clusters.iter().enumerate() {
        if c.c > 0.045 {
            let score = c.c * c.weight.sqrt();
            if vivid.is_none_or(|(_, s)| score > s) {
                vivid = Some((i, score));
            }
        }
    }
    let accent_from = vivid.map(|(i, _)| i);
    let (acc_h, acc_c) = match accent_from {
        Some(i) => (clusters[i].h, clusters[i].c.clamp(0.11, 0.19)),
        None => (44.0, 0.15),
    };
    roles.set(Role::Accent, fit(st.accent, acc_c, acc_h, 4.5));

    for (role, h0, c0) in SEM {
        // Clusters are sorted by weight, so the first match is the heaviest.
        let near = clusters
            .iter()
            .find(|c| c.c > 0.04 && hue_dist(c.h, h0) < 35.0);
        let (h, c) = match near {
            Some(n) => (lerp_hue(h0, n.h, 0.5), n.c.clamp(c0 * 0.8, c0 * 1.3)),
            None => (h0, c0),
        };
        roles.set(role, fit(st.sem, c, h, 4.5));
    }
    Draft { roles, accent_from }
}

/// A palette from a draft (spec step 9): the slug follows from the name; no
/// pins, no pair, no role names, no notes.
pub fn draft_palette(clusters: &[Cluster], mode: Mode, name: &str) -> Palette {
    Palette {
        name: name.to_string(),
        slug: slug_from_name(name),
        mode,
        pair: None,
        notes: None,
        roles: draft(clusters, mode).roles,
        names: BTreeMap::new(),
        pins: Pins::default(),
    }
}
