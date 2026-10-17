mod common;

use common::palette;
use irida_core::template::{Renderer, TemplateError, check_syntax};

fn render(slug: &str, src: &str) -> String {
    Renderer::new(&palette(slug))
        .render("t", src)
        .expect("renders")
}

#[test]
fn roles_and_filters() {
    assert_eq!(render("ember", "{{ accent }}"), "#E8713A");
    assert_eq!(render("ember", "{{ accent|strip }}"), "E8713A");
    assert_eq!(render("ember", "{{ accent|rgb }}"), "232,113,58");
    assert_eq!(
        render("ember", "{{ accent|rgba(0.8) }}"),
        "rgba(232,113,58,0.8)"
    );
    assert_eq!(render("ember", "{{ accent|ansi }}"), "38;2;232;113;58");
    assert_eq!(render("ember", "{{ '#e8713a'|hex }}"), "#E8713A");
}

#[test]
fn trailing_newline_kept() {
    assert_eq!(render("ember", "{{ accent }}\n"), "#E8713A\n");
}

#[test]
fn term_and_cosmic_namespaces() {
    assert_eq!(
        render(
            "ember",
            "{{ background }} {{ term.background }} {{ term.bright.red }} {{ term.normal.black }} {{ cosmic.neutral_10 }}"
        ),
        "#15171C #1B1E24 #DD6E6E #2A2F38 #E6E8EE"
    );
}

#[test]
fn names_and_meta() {
    assert_eq!(
        render(
            "ember",
            "{{ names.success|lower }} {{ name }} {{ slug }} {{ stem }} {{ mode }}"
        ),
        "moss Ember ember Ember dark"
    );
    assert_eq!(render("noir", "{{ names.success }}"), "success");
}

#[test]
fn mix_function() {
    assert_eq!(
        render("emberday", "{{ mix('text', 'pane', 0.70) }}"),
        "#A8A6A2"
    );
    assert_eq!(
        render("ember", "{{ mix('bright.red', '#000000', 0) }}"),
        "#DD6E6E"
    );
    assert_eq!(
        render("ember", "{{ mix('background', '#000000', 0) }}"),
        "#15171C"
    );
}

#[test]
fn ramp_function() {
    let out = render(
        "ember",
        "{% for c in ramp('success', 'error', 3) %}{{ c }} {% endfor %}",
    );
    assert!(out.starts_with("#8FB58A"), "{out}");
    assert!(out.ends_with("#D85A5A "), "{out}");
}

#[test]
fn errors_name_the_template() {
    let r = Renderer::new(&palette("ember"));
    for src in ["{{ nope }}", "{{ mix('nope', 'text', 0.5) }}"] {
        let e = r.render("t", src).expect_err("fails");
        assert!(
            matches!(&e, TemplateError { template, .. } if template == "t"),
            "{e}"
        );
    }
    assert!(check_syntax("t", "{% for %}").is_err());
    assert!(check_syntax("t", "{{ nope }}").is_ok());
}
