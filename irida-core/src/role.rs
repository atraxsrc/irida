//! The thirteen colour roles a palette defines.

use std::fmt;
use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Background,
    Pane,
    Container,
    Text,
    Muted,
    Comment,
    Accent,
    Success,
    Warning,
    Error,
    Blue,
    Magenta,
    Cyan,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RoleGroup {
    Surfaces,
    Text,
    Accent,
    Status,
    Hues,
}

impl Role {
    pub const ALL: [Role; 13] = [
        Role::Background,
        Role::Pane,
        Role::Container,
        Role::Text,
        Role::Muted,
        Role::Comment,
        Role::Accent,
        Role::Success,
        Role::Warning,
        Role::Error,
        Role::Blue,
        Role::Magenta,
        Role::Cyan,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Background => "background",
            Role::Pane => "pane",
            Role::Container => "container",
            Role::Text => "text",
            Role::Muted => "muted",
            Role::Comment => "comment",
            Role::Accent => "accent",
            Role::Success => "success",
            Role::Warning => "warning",
            Role::Error => "error",
            Role::Blue => "blue",
            Role::Magenta => "magenta",
            Role::Cyan => "cyan",
        }
    }

    pub fn group(self) -> RoleGroup {
        match self {
            Role::Background | Role::Pane | Role::Container => RoleGroup::Surfaces,
            Role::Text | Role::Muted | Role::Comment => RoleGroup::Text,
            Role::Accent => RoleGroup::Accent,
            Role::Success | Role::Warning | Role::Error => RoleGroup::Status,
            Role::Blue | Role::Magenta | Role::Cyan => RoleGroup::Hues,
        }
    }

    /// Minimum contrast ratio against the surfaces, if the role has one.
    pub fn min_contrast(self) -> Option<f64> {
        match self {
            Role::Background | Role::Pane | Role::Container | Role::Comment => None,
            Role::Muted => Some(3.0),
            _ => Some(4.5),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

impl FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Role::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| format!("unknown role: {s:?}"))
    }
}
