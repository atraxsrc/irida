//! The New from wallpaper dialog: the image, the colours found in it with
//! the accent's source outlined, Dark and Light, the name, and Create.
//! Views only read `State` and emit `Msg`.

use cosmic::Element;
use cosmic::iced::{Background, Border, Color, ContentFit, Length};
use cosmic::widget::{self, button, text};
use irida_core::color::Rgb;
use irida_core::palette::{Mode, Palette};
use irida_core::role::Role;

use super::caption;
use super::preview;
use super::roles::swatch;
use super::scene;
use super::state::{Msg, NO_WALLPAPER, State, WallpaperDraft};

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
fn status(d: &WallpaperDraft) -> Option<String> {
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

pub fn view<'a>(state: &'a State, d: &'a WallpaperDraft) -> Element<'a, Msg> {
    let mut control = widget::Column::new().spacing(12);

    if let Some(path) = &d.image {
        let photo = widget::image(widget::image::Handle::from_path(path))
            .content_fit(ContentFit::Contain)
            .width(Length::Fill)
            .height(Length::Fixed(160.));
        let name = text::caption(file_name(path));
        control = control.push(cosmic::iced::widget::stack([
            photo.into(),
            widget::container(name).padding([8, 8]).into(),
        ]));
    }
    if let Some(s) = status(d) {
        control = control.push(text::body(s));
    }
    if let Some(e) = &d.error {
        let c = cosmic::theme::active().cosmic().destructive_text_color();
        control = control.push(text::body(e.as_str()).class(cosmic::theme::Text::Color(c.into())));
    }

    let choose_label = if d.image.is_some() {
        "Choose another…"
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
        let caption = "Every role passes its contrast minimum.";
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
            .on_submit(|_| Msg::EditRoles);
        input = match state.name_check(&d.name) {
            Ok(slug) => input.helper_text(format!("Saved as {slug}")),
            Err(e) if !e.is_empty() => input.error(e),
            Err(_) => input.helper_text("Letters and digits, with single spaces"),
        };
        control = control.push(input);
    }

    let apply = button::suggested("Apply")
        .on_press_maybe(state.wallpaper_ready().then_some(Msg::WallpaperApply));
    let edit = button::standard("Edit roles")
        .on_press_maybe(state.wallpaper_ready().then_some(Msg::EditRoles));
    let mut buttons = widget::Row::new().spacing(8).push(apply).push(edit);
    if d.cancel {
        buttons = buttons.push(button::standard("Cancel").on_press(Msg::CloseWallpaper));
    }
    buttons = buttons.push(widget::Space::new().width(Length::Fill));
    buttons = buttons.push(text::caption("Undo puts every file back."));
    control = control.push(buttons);

    let left = widget::Column::new()
        .spacing(12)
        .push(text::heading("Your wallpaper, your palette."))
        .push(text::body(
            "Irida reads your wallpaper and builds a palette. Apply writes it into the apps on the right.",
        ))
        .push(control);

    let right: Element<'a, Msg> = match &d.roles {
        Some(roles) => {
            let mut preview_palette = Palette {
                name: "Wallpaper".to_string(),
                slug: "wallpaper".to_string(),
                mode: d.mode,
                pair: None,
                notes: None,
                image: d
                    .image
                    .as_ref()
                    .and_then(|p| p.to_str().map(str::to_string)),
                roles: roles.clone(),
                names: Default::default(),
                pins: Default::default(),
            };
            if state.name_check(&d.name).is_ok() {
                preview_palette.name = d.name.clone();
                preview_palette.slug = irida_core::palette::slug_from_name(&d.name);
            }
            widget::Column::new()
                .spacing(12)
                .push(preview::view(
                    scene::scene(&preview_palette),
                    state.image_status(&preview_palette.image),
                ))
                .push(caption::line(&state.targets))
                .into()
        }
        None => text::body("").into(),
    };

    widget::Row::new()
        .spacing(24)
        .push(left.width(Length::FillPortion(2)))
        .push(widget::scrollable(right).width(Length::FillPortion(3)))
        .into()
}
