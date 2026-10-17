//! Every string the popup shows, the greyed states, and the settings update
//! after an apply. Pure.

use irida_core::store::Settings;

/// Why the rows and Undo are disabled right now (spec 3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// A Try it live session is open in the window.
    Live,
    /// settings.toml exists but cannot be read; the reason.
    Settings(String),
    /// An apply of the row with this label is running.
    Applying(String),
    Undoing,
}

impl Blocked {
    pub fn text(&self) -> String {
        match self {
            Blocked::Live => "Live session in Irida".to_string(),
            Blocked::Settings(reason) => format!("Could not read settings: {reason}"),
            Blocked::Applying(label) => format!("Applying {label}\u{2026}"),
            Blocked::Undoing => "Undoing\u{2026}".to_string(),
        }
    }
}

pub const NO_PALETTES: &str = "No palettes yet";
pub const UNDO: &str = "Undo last apply";
pub const OPEN: &str = "Open Irida";
pub const CLOSE: &str = "Close Irida window";
pub const REMOVE: &str = "Remove from panel";
pub const HEADING_PAIRS: &str = "Pairs";
pub const HEADING_DARK: &str = "Dark";
pub const HEADING_LIGHT: &str = "Light";
pub const NOTHING_TO_UNDO: &str = "Nothing to undo";
pub const CANNOT_REACH: &str = "Could not reach Irida";
pub const NOT_ON_PANEL: &str = "Not on the panel";

pub fn applied(label: &str) -> String {
    format!("Applied {label}")
}

pub fn apply_failed(label: &str, reason: &str) -> String {
    format!("Could not apply {label}: {reason}")
}

/// `Undid {name}`, then one line per file that could not be put back.
pub fn undone(name: &str, failures: &[String]) -> String {
    let mut text = format!("Undid {name}");
    for f in failures {
        text.push('\n');
        text.push_str(f);
    }
    text
}

pub fn undo_failed(reason: &str) -> String {
    format!("Could not undo: {reason}")
}

pub fn cannot_start(reason: &str) -> String {
    format!("Could not start Irida: {reason}")
}

pub fn panel_failed(reason: &str) -> String {
    format!("Could not edit the panel settings: {reason}")
}

/// `settings` with `last_palette` set to `slug` and nothing else changed.
pub fn after_apply(settings: &Settings, slug: &str) -> Settings {
    Settings {
        last_palette: Some(slug.to_string()),
        ..settings.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use irida_core::store::Settings;
    use std::collections::BTreeSet;

    #[test]
    fn blocked_texts() {
        assert_eq!(Blocked::Live.text(), "Live session in Irida");
        assert_eq!(
            Blocked::Settings("bad toml".to_string()).text(),
            "Could not read settings: bad toml"
        );
        assert_eq!(
            Blocked::Applying("Ember \u{b7} Ember Day".to_string()).text(),
            "Applying Ember \u{b7} Ember Day\u{2026}"
        );
        assert_eq!(Blocked::Undoing.text(), "Undoing\u{2026}");
    }

    #[test]
    fn status_texts() {
        assert_eq!(applied("Noir"), "Applied Noir");
        assert_eq!(
            apply_failed("Noir", "nothing to apply"),
            "Could not apply Noir: nothing to apply"
        );
        assert_eq!(undone("Noir", &[]), "Undid Noir");
        assert_eq!(
            undone("Noir", &["a failed".to_string(), "b failed".to_string()]),
            "Undid Noir\na failed\nb failed"
        );
        assert_eq!(NOTHING_TO_UNDO, "Nothing to undo");
        assert_eq!(undo_failed("disk"), "Could not undo: disk");
        assert_eq!(CANNOT_REACH, "Could not reach Irida");
        assert_eq!(
            cannot_start("no such file"),
            "Could not start Irida: no such file"
        );
        assert_eq!(NOT_ON_PANEL, "Not on the panel");
        assert_eq!(
            panel_failed("permission"),
            "Could not edit the panel settings: permission"
        );
        assert_eq!(NO_PALETTES, "No palettes yet");
        assert_eq!(
            [UNDO, OPEN, CLOSE, REMOVE],
            [
                "Undo last apply",
                "Open Irida",
                "Close Irida window",
                "Remove from panel"
            ]
        );
        assert_eq!(
            [HEADING_PAIRS, HEADING_DARK, HEADING_LIGHT],
            ["Pairs", "Dark", "Light"]
        );
    }

    #[test]
    fn after_apply_keeps_disabled() {
        let before = Settings {
            disabled: BTreeSet::from(["btop".to_string(), "nvim".to_string()]),
            last_palette: Some("ember".to_string()),
        };
        let after = after_apply(&before, "noir");
        assert_eq!(after.last_palette.as_deref(), Some("noir"));
        assert_eq!(after.disabled, before.disabled);
    }
}
