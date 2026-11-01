mod common;

use std::fs;
use std::process::Command;

use common::{TestEnv, apply_only, fixture_text, palette, render_apply, render_export, undo};
use irida_core::apply::TargetStatus;
use irida_core::template::Renderer;

#[test]
fn snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "gtk", "irida.css.tmpl"),
            fixture_text(&format!("expected/gtk/{slug}.css")),
            "{slug}"
        );
    }
}

#[test]
fn install_snapshot() {
    for slug in ["ember", "emberday"] {
        assert_eq!(
            render_export(slug, "gtk", "install.sh.tmpl"),
            fixture_text(&format!("expected/gtk/{slug}-install.sh")),
            "{slug}"
        );
    }
}

#[test]
fn install_script_is_valid() {
    let env = TestEnv::new();
    let script = env.tmp.path().join("install.sh");
    fs::write(&script, render_export("ember", "gtk", "install.sh.tmpl")).unwrap();
    let status = Command::new("/bin/bash")
        .arg("-n")
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn import_line_value() {
    let source = common::target_file("gtk", "import.tmpl");
    let p = palette("ember");
    assert_eq!(
        Renderer::new(&p).render("import.tmpl", &source).unwrap(),
        "url(\"irida.css\");"
    );
    assert_eq!(
        Renderer::for_export(&p)
            .render("import.tmpl", &source)
            .unwrap(),
        "url(\"ember.css\");"
    );
}

/// Every rule is `@define-color name value;` with a `#RRGGBB` or `rgba()` value.
#[test]
fn stylesheet_is_define_color_lines() {
    let out = render_export("ember", "gtk", "irida.css.tmpl");
    let mut n = 0;
    for line in out.lines() {
        if line.is_empty() || line.starts_with("/*") || line.starts_with(" *") {
            continue;
        }
        let rest = line
            .strip_prefix("@define-color ")
            .and_then(|r| r.strip_suffix(';'))
            .unwrap_or_else(|| panic!("{line:?}"));
        let (_, value) = rest.split_once(' ').unwrap_or_else(|| panic!("{line:?}"));
        assert!(
            value.starts_with("rgba(")
                || (value.len() == 7
                    && value.starts_with('#')
                    && value[1..].bytes().all(|b| b.is_ascii_hexdigit())),
            "{line:?}"
        );
        n += 1;
    }
    assert_eq!(n, 73, "{out}");
}

/// The import goes after COSMIC's in gtk-4.0, gtk-3.0 is left alone when its
/// folder is missing, and undo puts gtk.css back byte for byte.
#[test]
fn apply_imports_after_cosmic_and_skips_a_missing_gtk3() {
    let env = TestEnv::new();
    let cosmic = "@import url(\"cosmic.css\");\n";
    let gtk4 = env.write(".config/gtk-4.0/gtk.css", cosmic);
    let report = apply_only(&env, &palette("ember"), "gtk").unwrap();
    let t = report.get("gtk").unwrap();
    assert!(matches!(t.status, TargetStatus::Applied(_)), "{t:?}");
    assert_eq!(
        fs::read_to_string(&gtk4).unwrap(),
        "@import url(\"cosmic.css\");\n@import url(\"irida.css\");\n"
    );
    assert_eq!(
        fs::read_to_string(env.dirs.home.join(".config/gtk-4.0/irida.css")).unwrap(),
        render_apply("ember", "gtk", "irida.css.tmpl")
    );
    assert!(!env.dirs.home.join(".config/gtk-3.0").exists());
    apply_only(&env, &palette("emberday"), "gtk").unwrap();
    let text = fs::read_to_string(&gtk4).unwrap();
    assert_eq!(text.matches("irida.css").count(), 1, "{text}");
    undo(&env);
    undo(&env);
    assert_eq!(fs::read_to_string(&gtk4).unwrap(), cosmic);
    assert!(!env.dirs.home.join(".config/gtk-4.0/irida.css").exists());
}

/// With both folders present and no gtk.css yet, each gets a one-line gtk.css
/// that undo removes again.
#[test]
fn apply_creates_gtk_css_in_both_folders() {
    let env = TestEnv::new();
    for v in ["gtk-3.0", "gtk-4.0"] {
        fs::create_dir_all(env.dirs.home.join(".config").join(v)).unwrap();
    }
    apply_only(&env, &palette("ember"), "gtk").unwrap();
    for v in ["gtk-3.0", "gtk-4.0"] {
        let dir = env.dirs.home.join(".config").join(v);
        assert_eq!(
            fs::read_to_string(dir.join("gtk.css")).unwrap(),
            "@import url(\"irida.css\");\n",
            "{v}"
        );
        assert!(dir.join("irida.css").is_file(), "{v}");
    }
    undo(&env);
    for v in ["gtk-3.0", "gtk-4.0"] {
        let dir = env.dirs.home.join(".config").join(v);
        assert!(!dir.join("gtk.css").exists(), "{v}");
        assert!(!dir.join("irida.css").exists(), "{v}");
    }
}

/// The exported install script puts the import after the last live one
/// (COSMIC's), turns another rice's import off, and leaves a gtk.css that
/// already has it alone.
#[test]
fn install_script_orders_imports() {
    let env = TestEnv::new();
    let rice = env.tmp.path().join("rice/gtk");
    fs::create_dir_all(&rice).unwrap();
    fs::write(
        rice.join("install.sh"),
        render_export("ember", "gtk", "install.sh.tmpl"),
    )
    .unwrap();
    fs::write(rice.join("ember.css"), "x\n").unwrap();
    let gtk4 = env.write(
        ".config/gtk-4.0/gtk.css",
        "@import url(\"cosmic.css\");\n@import url(\"noir.css\");\nwindow { color: red; }\n",
    );
    let run = || {
        let out = Command::new("/bin/bash")
            .arg(rice.join("install.sh"))
            .env_clear()
            .env("HOME", &env.dirs.home)
            .env("TMPDIR", env.tmp.path())
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let stdout = run();
    assert!(stdout.contains("backup: gtk.css.bak"), "{stdout}");
    assert_eq!(
        fs::read_to_string(&gtk4).unwrap(),
        "@import url(\"cosmic.css\");\n@import url(\"ember.css\");\n/* @import url(\"noir.css\"); */\nwindow { color: red; }\n"
    );
    let gtk3 = env.dirs.home.join(".config/gtk-3.0/gtk.css");
    assert_eq!(
        fs::read_to_string(&gtk3).unwrap(),
        "@import url(\"ember.css\");\n"
    );
    let stdout = run();
    assert!(
        stdout.contains("gtk-4.0: gtk.css already imports ember.css"),
        "{stdout}"
    );
    assert!(!stdout.contains("backup"), "{stdout}");
}
