//! The New from wallpaper dialog: the image, the colours found in it with
//! the accent's source outlined, Dark and Light, the name, and Create.
//! Views only read `State` and emit `Msg`.

use cosmic::Element;
use cosmic::iced::{Background, Border, Color, ContentFit, Length};
use cosmic::widget::{self, button, text};
use irida_core::color::Rgb;
use irida_core::palette::Mode;
use irida_core::role::Role;

use super::roles::swatch;
use super::state::{Msg, NO_WALLPAPER, State, WallpaperDialog};

/// A cluster's colour, outlined in the theme's accent when the palette's
/// accent came from it.
fn cluster_swatch(c: Rgb, accent: bool) -> Element<'static, Msg> {
    let outline = accent.then(|| Color::from(cosmic::theme::active().cosmic().accent_color()));
    widget::container(text::body(""))
        .width(Length::Fixed(40.))
        .height(Length::Fixed(40.))
        .style(move |_| widget::container::Style {
            background: Some(Background::Color(Color::from_rgb8(c.r, c.g, c.b))),
            border: Border {
                radius: 8.0.into(),
                width: if outline.is_some() { 3.0 } else { 1.0 },
                color: outline.unwrap_or(Color::from_rgba8(128, 128, 128, 0.5)),
            },
            ..Default::default()
        })
        .into()
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// What the dialog says about the image: being found, being read, or
/// missing.
fn status(d: &WallpaperDialog) -> Option<String> {
    if let Some(path) = &d.reading {
        Some(format!("Reading {}", file_name(path)))
    } else if d.finding {
        Some("Looking for your wallpaper".to_string())
    } else if d.image.is_none() && d.error.is_none() {
        Some(NO_WALLPAPER.to_string())
    } else {
        None
    }
}

pub fn view<'a>(state: &'a State, d: &'a WallpaperDialog) -> widget::Dialog<'a, Msg> {
    let mut control = widget::Column::new().spacing(12);

    if let Some(path) = &d.image {
        control = control.push(
            widget::container(
                widget::image(widget::image::Handle::from_path(path))
                    .content_fit(ContentFit::Contain)
                    .width(Length::Fill)
                    .height(Length::Fixed(160.)),
            )
            .center_x(Length::Fill),
        );
    }
    if let Some(s) = status(d) {
        control = control.push(text::body(s));
    }
    if let Some(e) = &d.error {
        let c = cosmic::theme::active().cosmic().destructive_text_color();
        control = control.push(text::body(e.as_str()).class(cosmic::theme::Text::Color(c.into())));
    }

    let choose_label = if d.image.is_some() {
        "Choose another image"
    } else {
        "Choose an image"
    };
    let choose = if d.image.is_some() {
        button::standard(choose_label)
    } else {
        button::suggested(choose_label)
    };
    control = control.push(choose.on_press(Msg::ChooseImage));

    if let Some(roles) = &d.roles {
        let found = d
            .clusters
            .iter()
            .enumerate()
            .fold(widget::Row::new().spacing(8), |row, (i, c)| {
                row.push(cluster_swatch(c.rgb, d.accent_from == Some(i)))
            });
        let caption = if d.accent_from.is_some() {
            "Colours found. The outlined one gave the accent."
        } else {
            "Colours found. None is vivid, so the accent is Irida's own."
        };
        let drafted = Role::ALL
            .into_iter()
            .fold(widget::Row::new().spacing(4), |row, role| {
                row.push(swatch(Some(roles.get(role)), 20.))
            });
        let modes =
            [Mode::Dark, Mode::Light]
                .into_iter()
                .fold(widget::Row::new().spacing(8), |r, m| {
                    let label = match m {
                        Mode::Dark => "Dark",
                        Mode::Light => "Light",
                    };
                    let b = if m == d.mode {
                        button::suggested(label)
                    } else {
                        button::standard(label)
                    };
                    r.push(b.on_press(Msg::WallpaperMode(m)))
                });
        control = control
            .push(found)
            .push(text::caption(caption))
            .push(modes)
            .push(drafted);

        let mut input = widget::text_input("Name", d.name.as_str())
            .on_input(Msg::WallpaperName)
            .on_submit(|_| Msg::WallpaperCreate);
        input = match state.name_check(&d.name) {
            Ok(slug) => input.helper_text(format!("Saved as {slug}")),
            Err(e) if !e.is_empty() => input.error(e),
            Err(_) => input.helper_text("Letters and digits, with single spaces"),
        };
        control = control.push(input);
    }

    widget::dialog()
        .title("New from wallpaper")
        .control(control)
        .primary_action(
            button::suggested("Create")
                .on_press_maybe(state.wallpaper_ready().then_some(Msg::WallpaperCreate)),
        )
        .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog))
}
