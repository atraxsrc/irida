//! WCAG contrast ratios and the per-role check against background and pane.

use crate::color::Rgb;
use crate::palette::Palette;
use crate::role::Role;

fn channel(v: u8) -> f64 {
    let c = v as f64 / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance.
pub fn luminance(c: Rgb) -> f64 {
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

/// WCAG contrast ratio, symmetric, always at least 1.
pub fn ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoleContrast {
    pub role: Role,
    pub color: Rgb,
    pub on_background: f64,
    pub on_pane: f64,
    pub min: Option<f64>,
}

impl RoleContrast {
    pub fn fails_on_background(&self) -> bool {
        self.min.is_some_and(|m| self.on_background < m)
    }

    pub fn fails_on_pane(&self) -> bool {
        self.min.is_some_and(|m| self.on_pane < m)
    }

    pub fn passes(&self) -> bool {
        !self.fails_on_background() && !self.fails_on_pane()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContrastReport {
    pub rows: Vec<RoleContrast>,
}

impl ContrastReport {
    pub fn failures(&self) -> Vec<&RoleContrast> {
        self.rows.iter().filter(|r| !r.passes()).collect()
    }
}

/// Check the ten non-surface roles against background and pane.
pub fn check(p: &Palette) -> ContrastReport {
    let bg = p.roles.get(Role::Background);
    let pane = p.roles.get(Role::Pane);
    let rows = Role::ALL
        .iter()
        .copied()
        .filter(|r| !matches!(r, Role::Background | Role::Pane | Role::Container))
        .map(|role| {
            let color = p.roles.get(role);
            RoleContrast {
                role,
                color,
                on_background: ratio(color, bg),
                on_pane: ratio(color, pane),
                min: role.min_contrast(),
            }
        })
        .collect();
    ContrastReport { rows }
}
