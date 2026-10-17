//! What the popup lists: the saved palettes as pair, dark and light rows,
//! with the tick on the one applied last. Pure; the store is read elsewhere.

use irida_core::palette::{Mode, Palette};
use irida_core::store::StoreEntry;

/// A row's palettes: a linked pair, or one palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Pair {
        dark: Box<Palette>,
        light: Box<Palette>,
    },
    Single(Box<Palette>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    /// The palette applied last is this row.
    pub ticked: bool,
}

/// The rows, in the popup's order: pairs, then dark, then light.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Menu {
    pub pairs: Vec<Row>,
    pub dark: Vec<Row>,
    pub light: Vec<Row>,
}

impl Menu {
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty() && self.dark.is_empty() && self.light.is_empty()
    }
}

impl Kind {
    /// The row's text: the palette's name, or `{dark} · {light}` for a pair.
    pub fn label(&self) -> String {
        match self {
            Kind::Pair { dark, light } => format!("{} \u{b7} {}", dark.name, light.name),
            Kind::Single(p) => p.name.clone(),
        }
    }

    pub fn slugs(&self) -> Vec<&str> {
        match self {
            Kind::Pair { dark, light } => vec![&dark.slug, &light.slug],
            Kind::Single(p) => vec![&p.slug],
        }
    }

    /// The `palette` and `pair` of an apply: for a pair, the half whose mode
    /// is `mode` and the other half; the dark half when the mode is unknown.
    pub fn pick(&self, mode: Option<Mode>) -> (&Palette, Option<&Palette>) {
        match self {
            Kind::Pair { dark, light } => match mode {
                Some(Mode::Light) => (light, Some(dark)),
                _ => (dark, Some(light)),
            },
            Kind::Single(p) => (p, None),
        }
    }

    fn ticked(&self, latest: Option<&str>) -> bool {
        latest.is_some_and(|l| self.slugs().contains(&l))
    }
}

/// Group the store's entries. Entries are visited in the order given;
/// unreadable ones are skipped. A pair row joins `p` with the palette its
/// `pair` names, or else with the first later palette whose `pair` names
/// `p`, when that palette loads, has the opposite mode and is not yet in a
/// row. Everything else is a single under its mode. `latest` is the slug of
/// the last apply record.
pub fn menu(entries: Vec<StoreEntry>, latest: Option<&str>) -> Menu {
    let palettes: Vec<Palette> = entries.into_iter().filter_map(|e| e.result.ok()).collect();
    let mut placed = vec![false; palettes.len()];
    let mut m = Menu::default();
    for i in 0..palettes.len() {
        if placed[i] {
            continue;
        }
        let p = &palettes[i];
        let free = |j: usize| !placed[j] && j != i && palettes[j].mode != p.mode;
        let partner = p
            .pair
            .as_deref()
            .and_then(|slug| palettes.iter().position(|q| q.slug == slug))
            .filter(|&j| free(j))
            .or_else(|| {
                (0..palettes.len())
                    .find(|&j| free(j) && palettes[j].pair.as_deref() == Some(&p.slug))
            });
        placed[i] = true;
        let kind = match partner {
            Some(j) => {
                placed[j] = true;
                let (dark, light) = if p.mode == Mode::Dark {
                    (p.clone(), palettes[j].clone())
                } else {
                    (palettes[j].clone(), p.clone())
                };
                Kind::Pair {
                    dark: Box::new(dark),
                    light: Box::new(light),
                }
            }
            None => Kind::Single(Box::new(p.clone())),
        };
        let row = Row {
            ticked: kind.ticked(latest),
            kind,
        };
        match &row.kind {
            Kind::Pair { .. } => m.pairs.push(row),
            Kind::Single(p) if p.mode == Mode::Dark => m.dark.push(row),
            Kind::Single(_) => m.light.push(row),
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use irida_core::palette::{Mode, Palette};
    use irida_core::store::{StoreEntry, StoreError, starters};

    /// The four starters with every `pair` cleared, in slug order.
    fn unpaired() -> Vec<Palette> {
        let mut v: Vec<Palette> = starters()
            .into_iter()
            .map(|mut p| {
                p.pair = None;
                p
            })
            .collect();
        v.sort_by(|a, b| a.slug.cmp(&b.slug));
        v
    }

    fn set_pair(v: &mut [Palette], slug: &str, pair: &str) {
        let p = v.iter_mut().find(|p| p.slug == slug).unwrap();
        p.pair = Some(pair.to_string());
    }

    fn entries(v: Vec<Palette>) -> Vec<StoreEntry> {
        v.into_iter()
            .map(|p| StoreEntry {
                slug: p.slug.clone(),
                result: Ok(p),
            })
            .collect()
    }

    fn slugs(rows: &[Row]) -> Vec<Vec<&str>> {
        rows.iter().map(|r| r.kind.slugs()).collect()
    }

    #[test]
    fn pair_row_when_one_half_names_the_other() {
        let mut v = unpaired();
        set_pair(&mut v, "ember", "emberday");
        let m = menu(entries(v), None);
        assert_eq!(slugs(&m.pairs), vec![vec!["ember", "emberday"]]);
        assert_eq!(slugs(&m.dark), vec![vec!["noir"]]);
        assert_eq!(slugs(&m.light), vec![vec!["dossier"]]);
    }

    #[test]
    fn light_half_naming_the_dark_pairs_them() {
        let mut v = unpaired();
        set_pair(&mut v, "emberday", "ember");
        let m = menu(entries(v), None);
        assert_eq!(slugs(&m.pairs), vec![vec!["ember", "emberday"]]);
        assert_eq!(slugs(&m.dark), vec![vec!["noir"]]);
        assert_eq!(slugs(&m.light), vec![vec!["dossier"]]);
    }

    #[test]
    fn same_mode_pair_is_two_singles() {
        let mut v = unpaired();
        set_pair(&mut v, "ember", "noir");
        let m = menu(entries(v), None);
        assert!(m.pairs.is_empty());
        assert_eq!(slugs(&m.dark), vec![vec!["ember"], vec!["noir"]]);
    }

    #[test]
    fn missing_pair_is_a_single() {
        let mut v = unpaired();
        set_pair(&mut v, "ember", "gone");
        let m = menu(entries(v), None);
        assert!(m.pairs.is_empty());
        assert_eq!(slugs(&m.dark), vec![vec!["ember"], vec!["noir"]]);
    }

    #[test]
    fn third_palette_naming_a_paired_half_is_single() {
        let mut v = unpaired();
        set_pair(&mut v, "ember", "emberday");
        set_pair(&mut v, "noir", "emberday");
        let m = menu(entries(v), None);
        assert_eq!(slugs(&m.pairs), vec![vec!["ember", "emberday"]]);
        assert_eq!(slugs(&m.dark), vec![vec!["noir"]]);
    }

    #[test]
    fn unreadable_entries_are_skipped() {
        let mut e = entries(unpaired());
        e.insert(
            0,
            StoreEntry {
                slug: "broken".to_string(),
                result: Err(StoreError::Unreadable {
                    path: "broken/palette.toml".into(),
                    message: "bad".to_string(),
                }),
            },
        );
        let m = menu(e, None);
        assert_eq!(m.pairs.len() + m.dark.len() + m.light.len(), 4);
    }

    #[test]
    fn empty_store_is_empty_menu() {
        assert!(menu(Vec::new(), None).is_empty());
        assert!(!menu(entries(unpaired()), None).is_empty());
    }

    #[test]
    fn tick_on_single_and_on_either_pair_half() {
        let mut v = unpaired();
        set_pair(&mut v, "ember", "emberday");
        let m = menu(entries(v.clone()), Some("emberday"));
        assert!(m.pairs[0].ticked);
        assert!(!m.dark[0].ticked);
        let m = menu(entries(v.clone()), Some("noir"));
        assert!(!m.pairs[0].ticked);
        assert!(m.dark[0].ticked);
        let m = menu(entries(v), Some("gone"));
        assert!(!m.pairs[0].ticked && !m.dark[0].ticked && !m.light[0].ticked);
    }

    #[test]
    fn label_and_slugs() {
        let mut v = unpaired();
        set_pair(&mut v, "ember", "emberday");
        let m = menu(entries(v), None);
        assert_eq!(m.pairs[0].kind.label(), "Ember · Ember Day");
        assert_eq!(m.pairs[0].kind.slugs(), vec!["ember", "emberday"]);
        assert_eq!(m.dark[0].kind.label(), "Noir");
        assert_eq!(m.dark[0].kind.slugs(), vec!["noir"]);
    }

    #[test]
    fn pick_follows_the_mode_and_defaults_to_dark() {
        let mut v = unpaired();
        set_pair(&mut v, "ember", "emberday");
        let m = menu(entries(v), None);
        let pair = &m.pairs[0].kind;
        let (p, q) = pair.pick(Some(Mode::Light));
        assert_eq!(
            (p.slug.as_str(), q.map(|q| q.slug.as_str())),
            ("emberday", Some("ember"))
        );
        let (p, q) = pair.pick(Some(Mode::Dark));
        assert_eq!(
            (p.slug.as_str(), q.map(|q| q.slug.as_str())),
            ("ember", Some("emberday"))
        );
        let (p, q) = pair.pick(None);
        assert_eq!(
            (p.slug.as_str(), q.map(|q| q.slug.as_str())),
            ("ember", Some("emberday"))
        );
        let (p, q) = m.dark[0].kind.pick(Some(Mode::Light));
        assert_eq!((p.slug.as_str(), q.is_none()), ("noir", true));
    }
}
