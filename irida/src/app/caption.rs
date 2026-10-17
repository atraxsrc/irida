// The caption view in the next task is the caller. Until then the binary
// does not name these.
#![allow(dead_code)]

use irida_core::apply::TargetInfo;

pub struct Caption {
    /// Detected, enabled, not broken. Order is the input order.
    pub written: Vec<String>,
    /// Detected, not enabled, not broken. Order is the input order.
    pub off: Vec<String>,
}

pub fn caption(targets: &[TargetInfo]) -> Caption {
    let mut written = Vec::new();
    let mut off = Vec::new();
    for t in targets {
        if !t.detected || t.broken.is_some() {
            continue;
        }
        let names = if t.enabled { &mut written } else { &mut off };
        names.push(t.display.clone());
    }
    Caption { written, off }
}

/// 1 name: "{a} off". 2: "{a} and {b} off". 3 or more: "{a}, {b} and {c} off".
pub fn off_phrase(names: &[&str]) -> String {
    let body = match names {
        [] => String::new(),
        [a] => (*a).to_string(),
        [a, b] => format!("{a} and {b}"),
        [lead @ .., last] => format!("{} and {last}", lead.join(", ")),
    };
    format!("{body} off")
}

#[cfg(test)]
mod tests {
    use irida_core::apply::TargetInfo;

    use super::{caption, off_phrase};

    fn info(
        name: &str,
        display: &str,
        detected: bool,
        enabled: bool,
        broken: bool,
        live: bool,
    ) -> TargetInfo {
        TargetInfo {
            name: name.to_string(),
            display: display.to_string(),
            builtin: true,
            live,
            detected,
            enabled,
            broken: broken.then(|| "broken".to_string()),
        }
    }

    #[test]
    fn keeps_input_order_and_splits_off() {
        let targets = vec![
            info("zsh", "zsh", true, true, false, false),
            info("ghostty", "Ghostty", true, false, false, false),
            info("nvim", "Neovim", true, true, false, true),
        ];
        let c = caption(&targets);
        assert_eq!(c.written, ["zsh", "Neovim"]);
        assert_eq!(c.off, ["Ghostty"]);
    }

    #[test]
    fn drops_broken_and_undetected() {
        let targets = vec![
            info("btop", "btop", true, true, true, false),
            info("kitty", "kitty", false, true, false, false),
            info("cava", "cava", true, true, false, false),
        ];
        let c = caption(&targets);
        assert_eq!(c.written, ["cava"]);
        assert!(c.off.is_empty());
    }

    #[test]
    fn uses_display_not_id() {
        let targets = vec![info(
            "cosmic-term",
            "COSMIC Terminal",
            true,
            true,
            false,
            false,
        )];
        assert_eq!(caption(&targets).written, ["COSMIC Terminal"]);
    }

    #[test]
    fn empty_when_nothing_can_be_written() {
        let targets = vec![
            info("btop", "btop", true, true, true, false),
            info("kitty", "kitty", false, false, false, false),
        ];
        let c = caption(&targets);
        assert!(c.written.is_empty());
        assert!(c.off.is_empty());
    }

    #[test]
    fn off_phrase_joins_names() {
        assert_eq!(off_phrase(&["Ghostty"]), "Ghostty off");
        assert_eq!(off_phrase(&["Ghostty", "kitty"]), "Ghostty and kitty off");
        assert_eq!(
            off_phrase(&["Ghostty", "kitty", "Alacritty"]),
            "Ghostty, kitty and Alacritty off"
        );
    }
}
