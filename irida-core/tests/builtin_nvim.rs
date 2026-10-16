mod common;

use common::{TestEnv, apply_only, palette, render_apply, render_export, rice_text};
use irida_core::apply::TargetStatus;
use irida_core::reload::ReloadStatus;

const HINT: &str =
    "run :colorscheme irida in Neovim (add vim.cmd.colorscheme('irida') to your config to keep it)";

#[test]
fn matches_rice_files() {
    for slug in ["ember", "emberday"] {
        for (template, rel) in [
            ("colors.lua.tmpl", format!("nvim/colors/{slug}.lua")),
            ("plugin.lua.tmpl", format!("nvim/lua/plugins/{slug}.lua")),
            ("ron.vim.tmpl", "nvim/syntax/ron.vim".to_string()),
            ("install.sh.tmpl", "nvim/install.sh".to_string()),
        ] {
            assert_eq!(
                render_export(slug, "nvim", template),
                rice_text(slug, &rel),
                "{slug} {rel}"
            );
        }
    }
}

#[test]
fn apply_names_scheme_irida() {
    let applied = render_apply("ember", "nvim", "colors.lua.tmpl");
    let rice = rice_text("ember", "nvim/colors/ember.lua");
    let a: Vec<&str> = applied.split('\n').collect();
    let r: Vec<&str> = rice.split('\n').collect();
    assert_eq!(a.len(), r.len());
    let differ: Vec<usize> = (0..a.len())
        .filter(|&i| a[i] != r[i])
        .map(|i| i + 1)
        .collect();
    assert_eq!(differ, [5, 19]);
    for n in [5, 19] {
        assert_eq!(a[n - 1].replace("irida", "ember"), r[n - 1], "line {n}");
    }
}

#[test]
fn bg2_formula() {
    let src = "{{ mix(pane, '#FFFFFF' if mode == 'dark' else '#000000', 0.08) }}";
    for (slug, want) in [("ember", "#2D3036"), ("emberday", "#CBC8C0")] {
        let out = irida_core::template::Renderer::new(&palette(slug))
            .render("t", src)
            .unwrap();
        assert_eq!(out, want, "{slug}");
        assert!(
            render_export(slug, "nvim", "colors.lua.tmpl").contains(&format!("bg2 = \"{want}\""))
        );
    }
}

#[test]
fn apply_writes_colors_only() {
    let env = TestEnv::new();
    let nvim = env.dirs.config.join("nvim");
    std::fs::create_dir_all(&nvim).unwrap();
    let report = apply_only(&env, &palette("ember"), "nvim").unwrap();
    assert_eq!(
        report.get("nvim").unwrap().status,
        TargetStatus::Applied(ReloadStatus::Restart(HINT.into()))
    );
    let mut files: Vec<String> = walk(&nvim);
    files.sort();
    assert_eq!(files, ["colors/irida.lua"]);
    assert_eq!(
        std::fs::read_to_string(nvim.join("colors/irida.lua")).unwrap(),
        render_apply("ember", "nvim", "colors.lua.tmpl")
    );
}

fn walk(root: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    out
}
