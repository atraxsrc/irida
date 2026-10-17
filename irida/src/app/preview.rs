//! The preview scene view: draws a `Scene` with libcosmic widgets. It only
//! reads the scene; every colour is decided in `scene.rs`.

use std::path::Path;

use cosmic::Element;
use cosmic::iced::font::{Family, Style as FontStyle, Weight};
use cosmic::iced::widget::{rich_text, span, text::Span as RichSpan};
use cosmic::iced::{Alignment, Background, Border, Color, ContentFit, Font, Length};
use cosmic::widget::{self, container, text};
use image::ImageReader;
use irida_core::color::Rgb;

use super::scene::{CardColors, Scene, Span};
use super::state::Msg;

/// A decoded wallpaper, or the reason the flat background is used instead.
/// `handle` is made once here, so the renderer keeps its decoded copy
/// across redraws instead of uploading the bitmap again every frame.
#[derive(Debug)]
pub enum ImageStatus {
    Ready { handle: widget::image::Handle },
    Missing,
}

fn ready(width: u32, height: u32, rgba: Vec<u8>) -> ImageStatus {
    ImageStatus::Ready {
        handle: widget::image::Handle::from_rgba(width, height, rgba),
    }
}

const PREVIEW_LONGEST: u32 = 1280;

/// Decode `path` once. Missing, unreadable, or not an image is `Missing`.
/// A side longer than 1280 px is scaled down so the cache stays small.
pub fn load_preview_image(path: &Path) -> ImageStatus {
    let Ok(reader) = ImageReader::open(path) else {
        return ImageStatus::Missing;
    };
    let Ok(reader) = reader.with_guessed_format() else {
        return ImageStatus::Missing;
    };
    let Ok(img) = reader.decode() else {
        return ImageStatus::Missing;
    };
    let img = img.to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return ImageStatus::Missing;
    }
    let longest = w.max(h);
    if longest <= PREVIEW_LONGEST {
        return ready(w, h, img.into_raw());
    }
    let scale = |side: u32| -> u32 {
        let scaled = (u64::from(side) * u64::from(PREVIEW_LONGEST) + u64::from(longest) / 2)
            / u64::from(longest);
        u32::try_from(scaled).unwrap_or(PREVIEW_LONGEST).max(1)
    };
    let width = scale(w);
    let height = scale(h);
    let rgba = image::imageops::resize(&img, width, height, image::imageops::FilterType::Triangle)
        .into_raw();
    ready(width, height, rgba)
}

/// libcosmic embeds Noto Sans Mono, so the box drawing always lines up.
const MONO: Font = Font {
    family: Family::Name("Noto Sans Mono"),
    ..Font::MONOSPACE
};
const MONO_SIZE: f32 = 13.;

fn color(c: Rgb) -> Color {
    Color::from_rgb8(c.r, c.g, c.b)
}

fn color_a(c: Rgb, alpha: f32) -> Color {
    let mut color = color(c);
    color.a = alpha;
    color
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
fn pane<'a>(
    content: impl Into<Element<'a, Msg>>,
    bg: Rgb,
    alpha: f32,
    width: Length,
) -> Element<'a, Msg> {
    container(content)
        .style(move |_| container::Style {
            background: Some(Background::Color(color_a(bg, alpha))),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .padding([10, 12])
        .width(width)
        .into()
}

/// The settings window's width on the preview desktop: beside Neovim at the
/// default window size, under it when the preview is narrower.
const CARD_WIDTH: f32 = 230.;

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
    pane(window, c.window, 1., Length::Fixed(CARD_WIDTH))
}

/// The whole preview scene for one palette, drawn as a small desktop: the
/// panel across the top, the terminal under it on the left, then Neovim with
/// the settings window beside it, and the background showing to the right,
/// between and below the windows. On a narrow preview the settings window
/// wraps under Neovim. `image` draws behind it all when ready, and the
/// terminal and Neovim let a little of it through.
pub fn view(s: Scene, image: Option<&ImageStatus>) -> Element<'static, Msg> {
    let over = matches!(image, Some(ImageStatus::Ready { .. }));
    let panel_a = if over { 0.92 } else { 1. };
    let term_a = if over { 0.88 } else { 1. };
    let terminal = lines(s.terminal)
        .push(widget::Space::new().height(Length::Fixed(10.)))
        .push(btop(s.btop, s.terminal_fg))
        .push(widget::Space::new().height(Length::Fixed(10.)))
        .push(cava(s.cava));
    let bottom = widget::flex_row(vec![
        pane(lines(s.nvim), s.nvim_bg, term_a, Length::Shrink),
        card(s.card),
    ])
    .column_spacing(16)
    .row_spacing(16)
    .align_items(Alignment::Start)
    .width(Length::Fill);
    let desktop = widget::Column::new()
        .spacing(16)
        .padding([16, 24, 24, 24])
        .push(pane(terminal, s.terminal_bg, term_a, Length::Shrink))
        .push(bottom)
        .width(Length::Fill);
    let column = widget::Column::new()
        .push(pane(line(s.panel), s.panel_bg, panel_a, Length::Fill))
        .push(desktop)
        .width(Length::Fill);
    let Some(ImageStatus::Ready { handle, .. }) = image else {
        return monitor(column);
    };
    let photo = widget::image(handle.clone())
        .content_fit(ContentFit::Cover)
        .width(Length::Fill)
        .height(Length::Fill);
    // The column sets the size. The image sits under it and covers that area;
    // `clip` keeps the covered image from drawing past it, since the image
    // widget itself does not cut the overflow.
    let screen = cosmic::iced::widget::stack([column.into()])
        .push_under(photo)
        .clip(true)
        .width(Length::Fill);
    monitor(screen)
}

/// The widest the monitor gets; it is centred when the column is wider.
const MONITOR_MAX_WIDTH: f32 = 1280.;
/// The bezel around the screen.
const BEZEL: f32 = 8.;
const BEZEL_COLOR: Color = Color::from_rgb(0.04, 0.04, 0.04);

/// The preview as a monitor: the screen inside a thin dark bezel with
/// rounded corners, centred and no wider than `MONITOR_MAX_WIDTH`. Its height
/// follows the windows on the screen, so it is about 4:3 on a wide column and
/// taller on a narrow one rather than clipping them.
fn monitor<'a>(screen: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    let framed = container(screen)
        .padding(BEZEL)
        .max_width(MONITOR_MAX_WIDTH)
        .style(|_| container::Style {
            background: Some(Background::Color(BEZEL_COLOR)),
            border: Border {
                radius: 14.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
    container(framed)
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::ImageStatus;
    use super::load_preview_image;

    #[test]
    fn load_preview_image_reads_a_png_and_rejects_text() {
        let tmp = tempfile::TempDir::new().unwrap();
        let png = tmp.path().join("two.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
            .save(&png)
            .unwrap();
        assert!(matches!(
            load_preview_image(&png),
            ImageStatus::Ready { .. }
        ));
        let text = tmp.path().join("note.txt");
        std::fs::write(&text, "not an image").unwrap();
        assert!(matches!(load_preview_image(&text), ImageStatus::Missing));
        assert!(matches!(
            load_preview_image(&tmp.path().join("gone.png")),
            ImageStatus::Missing
        ));
    }
}
