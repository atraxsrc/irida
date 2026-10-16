//! The preview scene view: draws a `Scene` with libcosmic widgets. It only
//! reads the scene; every colour is decided in `scene.rs`.

use cosmic::Element;
use cosmic::iced::font::{Family, Style as FontStyle, Weight};
use cosmic::iced::widget::{rich_text, span, text::Span as RichSpan};
use cosmic::iced::{Alignment, Background, Border, Color, Font, Length};
use cosmic::widget::{self, container, text};
use irida_core::color::Rgb;

use super::scene::{CardColors, Scene, Span};
use super::state::Msg;

/// libcosmic embeds Noto Sans Mono, so the box drawing always lines up.
const MONO: Font = Font {
    family: Family::Name("Noto Sans Mono"),
    ..Font::MONOSPACE
};
const MONO_SIZE: f32 = 13.;

fn color(c: Rgb) -> Color {
    Color::from_rgb8(c.r, c.g, c.b)
}

/// One line of styled monospace text.
fn line(spans: Vec<Span>) -> Element<'static, Msg> {
    let spans: Vec<RichSpan<'static, (), Font>> = spans
        .into_iter()
        .map(|s| {
            let font = Font {
                weight: if s.bold { Weight::Bold } else { Weight::Normal },
                style: if s.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
                ..MONO
            };
            span(s.text)
                .color(color(s.fg))
                .font(font)
                .underline(s.underline)
                .background_maybe(s.bg.map(color))
        })
        .collect();
    rich_text(spans).font(MONO).size(MONO_SIZE).into()
}

/// Plain text in one colour.
fn label(s: &str, c: Rgb) -> widget::Text<'static, cosmic::Theme> {
    text::body(s.to_string()).class(cosmic::theme::Text::Color(color(c)))
}

/// A filled, rounded rectangle.
fn fill<'a>(
    content: impl Into<Element<'a, Msg>>,
    bg: Rgb,
    radius: f32,
) -> widget::Container<'a, Msg, cosmic::Theme> {
    container(content).style(move |_| container::Style {
        background: Some(Background::Color(color(bg))),
        border: Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    })
}

/// A solid block with nothing in it.
fn block(bg: Rgb, w: f32, h: f32, radius: f32) -> Element<'static, Msg> {
    fill(widget::Space::new(), bg, radius)
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .into()
}

/// A window-like box: the panel, the terminal or Neovim.
fn pane<'a>(content: impl Into<Element<'a, Msg>>, bg: Rgb) -> Element<'a, Msg> {
    fill(content, bg, 8.)
        .padding([10, 12])
        .width(Length::Fill)
        .into()
}

fn lines(lines: Vec<Vec<Span>>) -> widget::Column<'static, Msg, cosmic::Theme> {
    lines
        .into_iter()
        .fold(widget::Column::new(), |col, l| col.push(line(l)))
}

/// btop's meter: ten cells from success through accent to error.
fn btop(cells: Vec<Rgb>, fg: Rgb) -> Element<'static, Msg> {
    let meter = cells
        .into_iter()
        .fold(widget::Row::new().spacing(2), |row, c| {
            row.push(block(c, 14., 10., 1.))
        });
    widget::Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .push(line(vec![Span {
            text: "cpu".into(),
            fg,
            bg: None,
            bold: true,
            italic: false,
            underline: false,
        }]))
        .push(meter)
        .into()
}

/// cava's bars, each in the gradient colour at its top.
fn cava(bars: Vec<(f32, Rgb)>) -> Element<'static, Msg> {
    const HEIGHT: f32 = 56.;
    bars.into_iter()
        .fold(
            widget::Row::new()
                .spacing(3)
                .height(Length::Fixed(HEIGHT))
                .align_y(Alignment::End),
            |row, (h, c)| row.push(block(c, 10., (h * HEIGHT).max(2.), 1.)),
        )
        .into()
}

/// A COSMIC toggle drawn in the card's colours.
fn toggle(on: bool, c: &CardColors) -> Element<'static, Msg> {
    let (track, knob) = if on {
        (c.accent, c.on_accent)
    } else {
        (c.divider, c.caption)
    };
    let row = widget::Row::new().padding(3).height(Length::Fill);
    let row = if on {
        row.push(widget::Space::new().width(Length::Fill))
            .push(block(knob, 16., 16., 8.))
    } else {
        row.push(block(knob, 16., 16., 8.))
    };
    fill(row, track, 11.)
        .width(Length::Fixed(40.))
        .height(Length::Fixed(22.))
        .into()
}

/// A button drawn in the card's colours.
fn pill(name: &str, bg: Rgb, fg: Rgb) -> Element<'static, Msg> {
    fill(label(name, fg), bg, 16.).padding([6, 16]).into()
}

fn setting(name: &str, caption: &str, on: bool, c: &CardColors) -> Element<'static, Msg> {
    widget::Row::new()
        .spacing(12)
        .align_y(Alignment::Center)
        .push(
            widget::Column::new()
                .width(Length::Fill)
                .push(label(name, c.text))
                .push(
                    text::caption(caption.to_string())
                        .class(cosmic::theme::Text::Color(color(c.caption))),
                ),
        )
        .push(toggle(on, c))
        .into()
}

/// The COSMIC settings card, in its window.
fn card(c: CardColors) -> Element<'static, Msg> {
    let divider_row = || {
        fill(widget::Space::new(), c.divider, 0.)
            .width(Length::Fill)
            .height(Length::Fixed(1.))
    };
    let inner = widget::Column::new()
        .spacing(10)
        .push(setting("Dark mode", "Use dark colours in apps", true, &c))
        .push(divider_row())
        .push(setting(
            "Frosted windows",
            "Blur what is behind windows",
            false,
            &c,
        ))
        .push(divider_row())
        .push(
            widget::Row::new()
                .spacing(8)
                .push(widget::Space::new().width(Length::Fill))
                .push(pill("Reset", c.destructive, c.on_destructive))
                .push(pill("Apply", c.accent, c.on_accent)),
        );
    let window = widget::Column::new()
        .spacing(10)
        .push(text::heading("Appearance").class(cosmic::theme::Text::Color(color(c.title))))
        .push(fill(inner, c.container, 8.).padding(12).width(Length::Fill));
    pane(window, c.window)
}

/// The whole preview scene for one palette.
pub fn view(s: Scene) -> Element<'static, Msg> {
    let terminal = lines(s.terminal)
        .push(widget::Space::new().height(Length::Fixed(10.)))
        .push(btop(s.btop, s.terminal_fg))
        .push(widget::Space::new().height(Length::Fixed(10.)))
        .push(cava(s.cava));
    widget::Column::new()
        .spacing(12)
        .push(pane(line(s.panel), s.panel_bg))
        .push(pane(terminal, s.terminal_bg))
        .push(pane(lines(s.nvim), s.nvim_bg))
        .push(card(s.card))
        .into()
}
