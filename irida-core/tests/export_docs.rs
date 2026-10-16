mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::{TestEnv, fixture_text, palette};
use irida_core::export::{ExportReport, ExportRequest, ExportStatus, export};
use irida_core::export_docs::{
    README_BEGIN, README_END, SHOWCASE_BEGIN, SHOWCASE_END, readme_block, showcase_block,
};
use irida_core::palette::Palette;
use irida_core::write::{BlockError, replace_marked};

fn run(env: &TestEnv, p: &Palette, pair: Option<&Palette>, folder: &Path) -> ExportReport {
    export(
        &env.dirs,
        &ExportRequest {
            palette: p,
            pair,
            folder,
        },
    )
    .unwrap()
}

fn status<'a>(r: &'a ExportReport, path: &str) -> Vec<&'a ExportStatus> {
    r.lines
        .iter()
        .filter(|l| l.path == path)
        .map(|l| &l.status)
        .collect()
}

/// The lines from the role table header through the last pair-table row.
fn tables(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "| Role | Color | Hex |")
        .expect("role table");
    let pair = start
        + lines[start..]
            .iter()
            .position(|l| l.starts_with("| Role | ") && *l != "| Role | Color | Hex |")
            .expect("pair table");
    let end = pair
        + lines[pair..]
            .iter()
            .position(|l| !l.starts_with('|'))
            .unwrap_or(lines.len() - pair);
    lines[start..end].iter().map(|l| l.to_string()).collect()
}

#[test]
fn readme_block_matches() {
    for (slug, other) in [("ember", "emberday"), ("emberday", "ember")] {
        let want: Vec<String> = tables(&fixture_text(&format!(
            "rices/cosmic-{slug}-theme/README.md"
        )))
        .into_iter()
        .map(|l| {
            if l == "`python3 scripts/contrast.py`." {
                "`irida check .`.".to_string()
            } else {
                l
            }
        })
        .collect();
        let got = tables(&readme_block(&palette(slug), Some(&palette(other))));
        assert_eq!(got, want, "{slug}");
    }
}

#[test]
fn readme_block_transparent_and_alone() {
    let mut p = palette("ember");
    p.pins.set(
        irida_core::slot::Slot::Background,
        irida_core::slot::PinValue::None,
    );
    let block = readme_block(&p, None);
    assert!(
        block.contains(" Pane, transparent | `#1B1E24` |"),
        "{block}"
    );
    assert!(!block.contains("| Role | Ember |"), "{block}");
    assert!(block.ends_with("`irida check .`.\n"), "{block}");
}

#[test]
fn showcase_rows() {
    let ember = showcase_block(&palette("ember"));
    assert!(
        ember.starts_with(
            "Every text colour is checked against the window background `#15171C` and the\nterminal pane `#1B1E24`.\n\n| Role | Hex | On background | On pane | Minimum |\n|------|-----|---------------|---------|---------|\n"
        ),
        "{ember}"
    );
    assert!(
        ember.contains("| Brick, error | `#D85A5A` | 4.7:1 | 4.4:1 (fail) | 4.5:1 |\n"),
        "{ember}"
    );
    assert!(
        ember.contains("| Comment | `#5A6070` | 2.9:1 | 2.7:1 | - |\n"),
        "{ember}"
    );
    let dossier = showcase_block(&palette("dossier"));
    assert!(
        dossier.contains("| Success | `#51573B` | 4.9:1 | 4.45:1 (fail) | 4.5:1 |\n"),
        "{dossier}"
    );
}

/// The fixture `switch.sh` with exactly the three changes Irida's version makes.
fn changed_switch(fixture: &str) -> String {
    let changes = [
        (
            "# Terminal tools; bat and starship only exist in some repos\nfor tool in fastfetch lsd bat starship; do\n",
            "# Terminal tools; bat, starship and cava only exist in some repos\nfor tool in fastfetch lsd bat starship cava; do\n",
        ),
        (
            "# Updater banner\nln -sfn \"$R/scripts/update_system.sh\" \"$HOME/update_system.sh\"\necho \"  ~/update_system.sh -> $NAME\"\n\n",
            "",
        ),
        (
            "./claude-desktop/install.sh | sed '/^$/d; s/^/  claude-desktop: /'\nclaude_theme=\"$(basename \"$(ls claude-desktop/*.jsonc | head -n1)\" .jsonc)\"\n",
            "./claude-desktop/install.sh | sed '/^$/d; s/^/  claude-desktop: /' || echo \"  claude-desktop: skipped (see above)\"\nclaude_theme=\"$(basename \"$(ls claude-desktop/*.jsonc 2>/dev/null | head -n1)\" .jsonc)\"\n",
        ),
    ];
    let mut out = fixture.to_string();
    for (from, to) in changes {
        assert_eq!(out.matches(from).count(), 1, "{from}");
        out = out.replace(from, to);
    }
    out
}

#[test]
fn switch_sh() {
    let env = TestEnv::new();
    let parent = env.tmp.path().join("out");
    for slug in ["ember", "emberday"] {
        let report = run(&env, &palette(slug), None, &parent);
        let path = report.repo.join("scripts/switch.sh");
        let got = fs::read_to_string(&path).unwrap();
        let want = changed_switch(&fixture_text(&format!(
            "rices/cosmic-{slug}-theme/scripts/switch.sh"
        )));
        assert_eq!(got, want, "{slug}");
        assert_eq!(
            status(&report, "scripts/switch.sh"),
            [&ExportStatus::Created]
        );
        let syntax = Command::new("/bin/bash")
            .arg("-n")
            .arg(&path)
            .status()
            .unwrap();
        assert!(syntax.success(), "{slug}");
    }

    let mut copper = palette("ember");
    copper.name = "Copper Tide".into();
    copper.slug = "coppertide".into();
    let report = run(&env, &copper, None, &parent);
    let text = fs::read_to_string(report.repo.join("scripts/switch.sh")).unwrap();
    assert!(
        text.contains("\nNAME=\"Copper Tide\"                  # shown in messages\n"),
        "{text}"
    );
    let rices = text
        .lines()
        .find(|l| l.starts_with("RICES="))
        .expect("RICES line");
    assert!(rices.ends_with(" coppertide\""), "{rices}");
    assert!(text.contains("\nRON=\"CopperTide.ron\" "), "{text}");
}

#[test]
fn replace_marked_rules() {
    let (b, e) = (README_BEGIN, README_END);
    assert_eq!(replace_marked("# Title\n", b, e, "x\n"), Ok(None));
    assert_eq!(
        replace_marked(&format!("top\n{b}\nold\nold\n{e}\nbottom"), b, e, "new\n"),
        Ok(Some(format!("top\n{b}\nnew\n{e}\nbottom")))
    );
    for broken in [
        format!("{b}\nno end\n"),
        format!("no begin\n{e}\n"),
        format!("{e}\n{b}\n"),
        format!("{b}\n{e}\n{b}\n{e}\n"),
    ] {
        assert_eq!(
            replace_marked(&broken, b, e, "new\n"),
            Err(BlockError::Broken),
            "{broken}"
        );
    }
}

#[test]
fn docs_in_export() {
    let env = TestEnv::new();
    let ember = palette("ember");
    let emberday = palette("emberday");
    let parent = env.tmp.path().join("out");
    let report = run(&env, &ember, Some(&emberday), &parent);
    let repo = report.repo.clone();

    let readme = fs::read_to_string(repo.join("README.md")).unwrap();
    let block = readme_block(&ember, Some(&emberday));
    assert!(readme.starts_with("# COSMIC · Ember\n"), "{readme}");
    assert!(
        readme.contains(&format!("\n\n{README_BEGIN}\n\n{block}\n{README_END}\n\n")),
        "{readme}"
    );
    assert!(readme.contains("./scripts/switch.sh"), "{readme}");
    assert!(readme.contains("## License"), "{readme}");
    let showcase = fs::read_to_string(repo.join("docs/showcase.md")).unwrap();
    assert!(
        showcase.starts_with("# Ember\n\n> The dark half of Ember / Ember Day.\n\n## Palette\n\n"),
        "{showcase}"
    );
    assert!(
        showcase.ends_with(&format!(
            "{SHOWCASE_BEGIN}\n\n{}\n{SHOWCASE_END}\n",
            showcase_block(&ember)
        )),
        "{showcase}"
    );
    for rel in ["README.md", "docs/showcase.md"] {
        assert_eq!(status(&report, rel), [&ExportStatus::Created], "{rel}");
    }

    let again = run(&env, &ember, Some(&emberday), &parent);
    for rel in ["README.md", "docs/showcase.md", "scripts/switch.sh"] {
        assert_eq!(status(&again, rel), [&ExportStatus::Unchanged], "{rel}");
    }

    let plain = "# Mine\n\nNo markers here.\n";
    fs::write(repo.join("README.md"), plain).unwrap();
    let report = run(&env, &ember, Some(&emberday), &parent);
    assert_eq!(fs::read_to_string(repo.join("README.md")).unwrap(), plain);
    assert_eq!(
        status(&report, "README.md"),
        [&ExportStatus::Skipped("no irida markers".into())]
    );

    let broken = format!("# Mine\n\n{README_BEGIN}\nold\n");
    fs::write(repo.join("README.md"), &broken).unwrap();
    let report = run(&env, &ember, Some(&emberday), &parent);
    assert_eq!(fs::read_to_string(repo.join("README.md")).unwrap(), broken);
    assert_eq!(
        status(&report, "README.md"),
        [&ExportStatus::Skipped("irida markers are broken".into())]
    );

    let mine = format!("# Mine\n\nintro\n\n{README_BEGIN}\nold\n{README_END}\n\nfooter\n");
    fs::write(repo.join("README.md"), &mine).unwrap();
    let report = run(&env, &ember, None, &parent);
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).unwrap(),
        format!(
            "# Mine\n\nintro\n\n{README_BEGIN}\n\n{}\n{README_END}\n\nfooter\n",
            readme_block(&ember, None)
        )
    );
    assert_eq!(status(&report, "README.md"), [&ExportStatus::Updated]);
}

#[test]
fn user_target_cannot_take_an_unmarked_readme() {
    let env = TestEnv::new();
    let dir = env.dirs.config.join("irida/targets/mine");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("target.toml"),
        "name = \"mine\"\n\n[[rice]]\npath = \"README.md\"\ntemplate = \"t.tmpl\"\n",
    )
    .unwrap();
    fs::write(dir.join("t.tmpl"), "mine\n").unwrap();
    let repo = env.tmp.path().join("cosmic-ember-theme");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("README.md"), "# Mine\n").unwrap();

    let report = run(&env, &palette("ember"), None, &repo);
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).unwrap(),
        "# Mine\n"
    );
    let st = status(&report, "README.md");
    assert!(
        st.contains(&&ExportStatus::Skipped("no irida markers".into())),
        "{st:?}"
    );
    assert!(
        st.contains(&&ExportStatus::Skipped("already written by irida".into())),
        "{st:?}"
    );
}
