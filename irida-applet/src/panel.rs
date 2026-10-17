//! Remove from panel: take this applet out of the panel's applet lists, the
//! same edit COSMIC Settings makes (spec 5.3).

use cosmic_config::{Config, ConfigGet, ConfigSet};

use crate::copy::{NOT_ON_PANEL, panel_failed};

/// The applet's desktop id, which is how the panel lists it.
pub const APPLET_ID: &str = "io.github.atraxsrc.IridaApplet";

/// The panel's applet lists: the two wings and the centre, each absent when
/// the panel's config has no such key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lists {
    pub wings: Option<(Vec<String>, Vec<String>)>,
    pub center: Option<Vec<String>>,
}

/// The lists without `id`, or `None` when `id` is in none of them.
pub fn without(lists: &Lists, id: &str) -> Option<Lists> {
    let strip = |v: &[String]| -> Vec<String> { v.iter().filter(|s| *s != id).cloned().collect() };
    let out = Lists {
        wings: lists.wings.as_ref().map(|(l, r)| (strip(l), strip(r))),
        center: lists.center.as_ref().map(|c| strip(c)),
    };
    (out != *lists).then_some(out)
}

/// A list key, `None` when the panel's config has no such key.
fn list<T: serde::de::DeserializeOwned>(config: &Config, key: &str) -> Result<Option<T>, String> {
    match config.get::<T>(key) {
        Ok(v) => Ok(Some(v)),
        Err(cosmic_config::Error::NotFound) => Ok(None),
        Err(cosmic_config::Error::GetKey(_, e)) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(None)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Take this applet off the panel named by `COSMIC_PANEL_NAME`, writing back
/// only the lists that change. `Err` is the status line to show.
pub fn remove_self(panel_name: Option<&str>) -> Result<(), String> {
    let name = panel_name.ok_or_else(|| NOT_ON_PANEL.to_string())?;
    let config = Config::new(&format!("com.system76.CosmicPanel.{name}"), 1)
        .map_err(|e| panel_failed(&e.to_string()))?;
    let lists = Lists {
        wings: list(&config, "plugins_wings").map_err(|e| panel_failed(&e))?,
        center: list(&config, "plugins_center").map_err(|e| panel_failed(&e))?,
    };
    let out = without(&lists, APPLET_ID).ok_or_else(|| NOT_ON_PANEL.to_string())?;
    if out.wings != lists.wings {
        config
            .set("plugins_wings", &out.wings)
            .map_err(|e| panel_failed(&e.to_string()))?;
    }
    if out.center != lists.center {
        config
            .set("plugins_center", &out.center)
            .map_err(|e| panel_failed(&e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lists(left: &[&str], right: &[&str], center: Option<&[&str]>) -> Lists {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        Lists {
            wings: Some((v(left), v(right))),
            center: center.map(v),
        }
    }

    #[test]
    fn removes_from_left_wing() {
        let l = lists(&["a", APPLET_ID, "b"], &["c"], None);
        let out = without(&l, APPLET_ID).unwrap();
        assert_eq!(out, lists(&["a", "b"], &["c"], None));
    }

    #[test]
    fn removes_from_center() {
        let l = lists(&["a"], &["c"], Some(&[APPLET_ID, "d"]));
        let out = without(&l, APPLET_ID).unwrap();
        assert_eq!(out, lists(&["a"], &["c"], Some(&["d"])));
    }

    #[test]
    fn removes_every_occurrence() {
        let l = lists(&[APPLET_ID], &["c", APPLET_ID], Some(&[APPLET_ID]));
        let out = without(&l, APPLET_ID).unwrap();
        assert_eq!(out, lists(&[], &["c"], Some(&[])));
    }

    #[test]
    fn none_when_absent() {
        let l = lists(&["a"], &["c"], Some(&["d"]));
        assert_eq!(without(&l, APPLET_ID), None);
    }

    #[test]
    fn none_when_lists_are_none() {
        let l = Lists {
            wings: None,
            center: None,
        };
        assert_eq!(without(&l, APPLET_ID), None);
    }
}
