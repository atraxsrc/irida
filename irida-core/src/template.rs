//! Template rendering with colour filters, `mix` and `ramp`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use minijinja::value::{Value, ValueKind};
use minijinja::{AutoEscape, Environment, Error, ErrorKind, UndefinedBehavior};

use crate::color::{self, Rgb};
use crate::derive::{Derived, derive};
use crate::palette::Palette;
use crate::ramp;
use crate::role::Role;
use crate::slot::{Ansi, Slot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateError {
    pub template: String,
    pub message: String,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "template {}: {}", self.template, self.message)
    }
}

impl std::error::Error for TemplateError {}

fn base_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_auto_escape_callback(|_| AutoEscape::None);
    // Booleans print as `true` and `false`, the way the files Irida writes spell them.
    env.set_formatter(|out, state, value| {
        if value.kind() == ValueKind::Bool {
            let b = value.is_true();
            out.write_str(if b { "true" } else { "false" })?;
            return Ok(());
        }
        minijinja::escape_formatter(out, state, value)
    });
    env
}

fn to_error(template: &str, e: Error) -> TemplateError {
    let mut message = e.to_string();
    let mut source = std::error::Error::source(&e);
    while let Some(s) = source {
        message.push_str(&format!(": {s}"));
        source = s.source();
    }
    TemplateError {
        template: template.to_string(),
        message,
    }
}

/// Compile a template without rendering it.
pub fn check_syntax(template_name: &str, source: &str) -> Result<(), TemplateError> {
    base_env()
        .template_from_named_str(template_name, source)
        .map(|_| ())
        .map_err(|e| to_error(template_name, e))
}

struct Resolver {
    roles: BTreeMap<String, Rgb>,
    derived: Derived,
}

impl Resolver {
    /// A role name first, then a slot name, then a `#RRGGBB` string.
    fn resolve(&self, s: &str) -> Result<Rgb, Error> {
        if let Some(c) = self.roles.get(s) {
            return Ok(*c);
        }
        if let Ok(slot) = Slot::from_str(s) {
            return self.derived.slot(slot).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidOperation,
                    format!("slot {s} has no colour"),
                )
            });
        }
        Rgb::from_hex(s).map_err(|_| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("unknown colour: {s:?}"),
            )
        })
    }
}

fn hex_arg(s: &str) -> Result<Rgb, Error> {
    Rgb::from_hex(s).map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))
}

pub struct Renderer {
    env: Environment<'static>,
    ctx: Value,
    fields: BTreeMap<String, Value>,
}

fn ansi_map(get: impl Fn(Ansi) -> Rgb) -> BTreeMap<&'static str, String> {
    Ansi::ALL
        .iter()
        .map(|a| (a.as_str(), get(*a).to_hex()))
        .collect()
}

impl Renderer {
    /// A renderer for applying to this machine: `export` is `false`.
    pub fn new(p: &Palette) -> Renderer {
        Renderer::build(p, false)
    }

    /// The same renderer with `config_dir`, which is `~/.config` until set,
    /// naming `dirs`' config folder.
    pub fn with_dirs(mut self, dirs: &crate::dirs::Dirs) -> Renderer {
        self.fields
            .insert("config_dir".into(), Value::from(dirs.config_display()));
        self.ctx = Value::from_serialize(&self.fields);
        self
    }

    /// A renderer for exporting a rice repo: `export` is `true`.
    pub fn for_export(p: &Palette) -> Renderer {
        Renderer::build(p, true)
    }

    fn build(p: &Palette, export: bool) -> Renderer {
        let derived = derive(p);
        let roles: BTreeMap<String, Rgb> = Role::ALL
            .iter()
            .map(|r| (r.as_str().to_string(), p.roles.get(*r)))
            .collect();

        let mut ctx: BTreeMap<String, Value> = BTreeMap::new();
        for (name, c) in &roles {
            ctx.insert(name.clone(), Value::from(c.to_hex()));
        }
        ctx.insert("name".into(), Value::from(p.name.clone()));
        ctx.insert("slug".into(), Value::from(p.slug.clone()));
        ctx.insert("stem".into(), Value::from(p.stem()));
        ctx.insert("mode".into(), Value::from(p.mode.to_string()));
        ctx.insert("export".into(), Value::from(export));
        // Where config files point at each other; `with_dirs` sets this machine's.
        ctx.insert("config_dir".into(), Value::from("~/.config"));
        ctx.insert(
            "notes".into(),
            Value::from(p.notes.clone().unwrap_or_default()),
        );
        let names: BTreeMap<&str, String> = Role::ALL
            .iter()
            .map(|r| {
                (
                    r.as_str(),
                    p.names
                        .get(r)
                        .cloned()
                        .unwrap_or_else(|| r.as_str().to_string()),
                )
            })
            .collect();
        ctx.insert("names".into(), Value::from_serialize(&names));

        let t = &derived.term;
        let mut term: BTreeMap<&str, Value> = BTreeMap::new();
        term.insert("foreground", Value::from(t.foreground.to_hex()));
        term.insert(
            "background",
            Value::from(t.background.map(|c| c.to_hex()).unwrap_or_default()),
        );
        term.insert("cursor", Value::from(t.cursor.to_hex()));
        term.insert(
            "bright_foreground",
            Value::from(t.bright_foreground.to_hex()),
        );
        term.insert("dim_foreground", Value::from(t.dim_foreground.to_hex()));
        term.insert(
            "normal",
            Value::from_serialize(ansi_map(|a| t.normal.get(a))),
        );
        term.insert(
            "bright",
            Value::from_serialize(ansi_map(|a| t.bright.get(a))),
        );
        term.insert("dim", Value::from_serialize(ansi_map(|a| t.dim.get(a))));
        ctx.insert("term".into(), Value::from_serialize(&term));

        let cosmic: BTreeMap<&str, String> = BTreeMap::from([
            ("neutral_10", derived.neutral_10.to_hex()),
            ("gray_2", derived.gray_2.to_hex()),
        ]);
        ctx.insert("cosmic".into(), Value::from_serialize(&cosmic));

        let mut env = base_env();
        env.add_filter("hex", |s: &str| hex_arg(s).map(|c| c.to_hex()));
        env.add_filter("strip", |s: &str| {
            hex_arg(s).map(|c| c.to_hex().trim_start_matches('#').to_string())
        });
        env.add_filter("rgb", |s: &str| {
            hex_arg(s).map(|c| format!("{},{},{}", c.r, c.g, c.b))
        });
        env.add_filter("rgba", |s: &str, alpha: f64| {
            hex_arg(s).map(|c| format!("rgba({},{},{},{alpha})", c.r, c.g, c.b))
        });
        env.add_filter("ansi", |s: &str| {
            hex_arg(s).map(|c| format!("38;2;{};{};{}", c.r, c.g, c.b))
        });

        let resolver = Arc::new(Resolver { roles, derived });
        let r = resolver.clone();
        env.add_function("mix", move |a: &str, b: &str, t: f64| {
            Ok(color::mix(r.resolve(a)?, r.resolve(b)?, t).to_hex())
        });
        let r = resolver;
        env.add_function("ramp", move |from: &str, to: &str, steps: usize| {
            let colours = ramp::ramp(r.resolve(from)?, r.resolve(to)?, steps)
                .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;
            Ok(colours.iter().map(Rgb::to_hex).collect::<Vec<_>>())
        });

        Renderer {
            env,
            ctx: Value::from_serialize(&ctx),
            fields: ctx,
        }
    }

    pub fn render(&self, template_name: &str, source: &str) -> Result<String, TemplateError> {
        self.env
            .render_named_str(template_name, source, &self.ctx)
            .map_err(|e| to_error(template_name, e))
    }

    /// Render with the palette context plus `extra` values.
    pub fn render_with(
        &self,
        template_name: &str,
        source: &str,
        extra: &[(&str, String)],
    ) -> Result<String, TemplateError> {
        let mut fields = self.fields.clone();
        for (k, v) in extra {
            fields.insert(k.to_string(), Value::from(v.clone()));
        }
        self.env
            .render_named_str(template_name, source, Value::from_serialize(&fields))
            .map_err(|e| to_error(template_name, e))
    }
}
