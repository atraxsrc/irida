//! The role list editor: the left column, grouped as in the palette model,
//! with each row opening in place. Views only read `State` and emit `Msg`.

use cosmic::Element;
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::widget::{self, button, text};
use irida_core::color::Rgb;
use irida_core::contrast::RoleContrast;
use irida_core::derive::derive;
use irida_core::palette::Palette;
use irida_core::role::RoleGroup;
use irida_core::slot::{PinValue, Slot};

use super::state::{Field, Msg, RoleRow, State, capitalised, ratio_text, role_rows, role_slots};

fn group_label(g: RoleGroup) -> &'static str {
    match g {
        RoleGroup::Surfaces => "Surfaces",
        RoleGroup::Text => "Text",
        RoleGroup::Accent => "Accent",
        RoleGroup::Status => "Status",
        RoleGroup::Hues => "Hues",
    }
}

/// A square of colour; an empty outline for `None` (no background).
pub fn swatch(c: Option<Rgb>, size: f32) -> Element<'static, Msg> {
    widget::container(text::body(""))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_| widget::container::Style {
            background: c.map(|c| Background::Color(Color::from_rgb8(c.r, c.g, c.b))),
            border: Border {
                radius: 6.0.into(),
                width: 1.0,
                color: Color::from_rgba8(128, 128, 128, 0.5),
            },
            ..Default::default()
        })
        .into()
}

/// Text in the theme's destructive colour when `failing`.
fn verdict<'a>(label: String, failing: bool) -> widget::Text<'a, cosmic::Theme> {
    let t = text::body(label);
    if failing {
        let c = cosmic::theme::active().cosmic().destructive_text_color();
        t.class(cosmic::theme::Text::Color(c.into()))
    } else {
        t
    }
}

/// The role list for `p`, the open palette's working copy.
pub fn view<'a>(state: &'a State, p: &'a Palette) -> Element<'a, Msg> {
    let mut col = widget::Column::new().spacing(4).padding([0, 12, 0, 0]);
    let mut group = None;
    for row in role_rows(p) {
        let g = row.role.group();
        if group != Some(g) {
            group = Some(g);
            col = col.push(widget::container(text::heading(group_label(g))).padding([12, 0, 4, 0]));
        }
        let open = state.expanded == Some(row.role);
        col = col.push(row_header(&row, open));
        if open {
            col = col.push(details(state, p, &row));
        }
    }
    widget::scrollable(col).height(Length::Fill).into()
}

fn row_header(row: &RoleRow, open: bool) -> Element<'static, Msg> {
    let mut line = widget::Row::new()
        .spacing(12)
        .align_y(Alignment::Center)
        .push(swatch(Some(row.color), 28.))
        .push(text::body(row.name.clone()).width(Length::Fill))
        .push(text::monotext(row.color.to_hex()));
    if let Some(label) = row.ratio_label() {
        line = line.push(verdict(label, row.fails()).width(Length::Fixed(64.)));
    } else {
        line = line.push(widget::Space::new().width(Length::Fixed(64.)));
    }
    let msg = Msg::Expand((!open).then_some(row.role));
    button::custom(line)
        .padding([6, 8])
        .width(Length::Fill)
        .class(if open {
            cosmic::theme::Button::Standard
        } else {
            cosmic::theme::Button::MenuItem
        })
        .on_press(msg)
        .into()
}

/// Both ratios with the minimum, each in red when under it.
fn ratios(c: &RoleContrast) -> Element<'static, Msg> {
    let need = match c.min {
        Some(m) => format!("Minimum {}", ratio_text(m)),
        None => "No minimum".to_string(),
    };
    widget::Column::new()
        .spacing(2)
        .push(verdict(
            format!("{} on background", ratio_text(c.on_background)),
            c.fails_on_background(),
        ))
        .push(verdict(
            format!("{} on pane", ratio_text(c.on_pane)),
            c.fails_on_pane(),
        ))
        .push(text::caption(need))
        .into()
}

/// The open row: picker, hex field, ratios, display name and slots.
fn details<'a>(state: &'a State, p: &'a Palette, row: &RoleRow) -> Element<'a, Msg> {
    let role = row.role;
    let picker: Element<'a, Msg> = state
        .picker
        .builder(Msg::Picker)
        .build("Recent colours", "Copy", "Copied")
        .into();

    let (hex, error) = match state.field(Field::Hex(role)) {
        Some(f) => (f.text.clone(), f.error.clone()),
        None => (row.color.to_hex(), None),
    };
    let mut hex_input = widget::text_input("#RRGGBB", hex)
        .label("Hex")
        .on_input(move |s| Msg::HexInput(role, s));
    if let Some(e) = error {
        hex_input = hex_input.error(e);
    }

    let name = p.names.get(&role).cloned().unwrap_or_default();
    let name_input = widget::text_input(capitalised(role), name)
        .label("Display name")
        .on_input(move |s| Msg::SetName(role, s));

    let mut col = widget::Column::new()
        .spacing(12)
        .push(picker)
        .push(hex_input);
    if let Some(c) = &row.contrast {
        col = col.push(ratios(c));
    }
    col = col.push(name_input);
    let slots = role_slots(role, p.mode);
    if !slots.is_empty() {
        col = col.push(text::heading("Terminal and desktop colours"));
        for slot in slots {
            col = col.push(slot_row(state, p, slot));
        }
    }
    widget::container(col)
        .padding(12)
        .class(cosmic::theme::Container::Card)
        .into()
}

/// A slot with its effective colour and a pin toggle; a pinned slot shows
/// a marker and an editable hex.
fn slot_row<'a>(state: &'a State, p: &'a Palette, slot: Slot) -> Element<'a, Msg> {
    let effective = derive(p).slot(slot);
    let pin = p.pins.get(slot);
    let mut line = widget::Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .push(swatch(effective, 20.))
        .push(text::body(slot.to_string()).width(Length::Fill));
    match pin {
        Some(PinValue::Color(c)) => {
            let (hex, error) = match state.field(Field::PinHex(slot)) {
                Some(f) => (f.text.clone(), f.error.clone()),
                None => (c.to_hex(), None),
            };
            let mut input = widget::text_input("#RRGGBB", hex)
                .width(Length::Fixed(110.))
                .on_input(move |s| Msg::PinHexInput(slot, s));
            if let Some(e) = error {
                input = input.error(e);
            }
            line = line
                .push(widget::icon::from_name("view-pin-symbolic").size(16))
                .push(input);
        }
        Some(PinValue::None) => {
            line = line
                .push(widget::icon::from_name("view-pin-symbolic").size(16))
                .push(text::monotext("none"));
        }
        None => {
            let hex = effective.map_or_else(|| "none".to_string(), |c| c.to_hex());
            line = line.push(text::monotext(hex));
        }
    }
    line = line.push(
        widget::toggler(pin.is_some())
            .label("Pin".to_string())
            .on_toggle(move |on| if on { Msg::Pin(slot) } else { Msg::Unpin(slot) }),
    );
    let mut col = widget::Column::new().spacing(4).push(line);
    if slot == Slot::Background {
        col = col.push(
            widget::toggler(pin == Some(PinValue::None))
                .label("No background (the terminal shows the desktop)".to_string())
                .on_toggle(Msg::NoBackground),
        );
    }
    col.into()
}

/// The design notes field, under the preview.
pub fn notes(p: &Palette) -> Element<'_, Msg> {
    widget::text_input("Design notes", p.notes.as_deref().unwrap_or_default())
        .label("Notes")
        .on_input(Msg::SetNotes)
        .into()
}
