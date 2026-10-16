//! The header (palette menu, mode chip, pair chip, Compare, try live,
//! Apply) and the dialogs. Views only read `State` and emit `Msg`.

use cosmic::Element;
use cosmic::iced::Length;
use cosmic::widget::{self, button, popover, text};
use irida_core::palette::Mode;

use super::state::{Dialog, HeaderItem, Leave, MenuItem, Msg, NameFor, State};

/// The palette name with its menu.
pub fn start(state: &State) -> Vec<Element<'_, Msg>> {
    let name = button::text(state.title())
        .trailing_icon(widget::icon::from_name("pan-down-symbolic"))
        .on_press(Msg::Menu(!state.menu_open));
    let mut menu = popover(name)
        .position(popover::Position::Bottom)
        .on_close(Msg::Menu(false));
    if state.menu_open {
        menu = menu.popup(menu_items(state));
    }
    vec![menu.into()]
}

fn menu_items(state: &State) -> Element<'_, Msg> {
    let items = MenuItem::ALL
        .iter()
        .fold(widget::Column::new().spacing(2), |col, item| {
            col.push(
                button::text(item.label())
                    .width(Length::Fill)
                    .on_press_maybe(state.menu_enabled(*item).then(|| item.msg())),
            )
        });
    widget::container(items)
        .padding(8)
        .width(Length::Fixed(240.))
        .class(cosmic::theme::Container::Dropdown)
        .into()
}

fn chip(label: String, enabled: bool, msg: Msg) -> Element<'static, Msg> {
    button::standard(label)
        .on_press_maybe(enabled.then_some(msg))
        .into()
}

/// The mode chip, the pair chip, Compare, the try-live toggle and Apply.
pub fn end(state: &State) -> Vec<Element<'_, Msg>> {
    let mode = state
        .current
        .as_ref()
        .and_then(|c| c.working())
        .map(|p| p.mode);
    let (mode_label, other) = match mode {
        Some(Mode::Light) => ("Light", Mode::Dark),
        _ => ("Dark", Mode::Light),
    };
    let live = widget::toggler(false)
        .label("Try it live".to_string())
        .on_toggle_maybe(
            state
                .header_enabled(HeaderItem::TryLive)
                .then_some(|_| Msg::NotYet("Try it live")),
        );
    vec![
        chip(
            mode_label.to_string(),
            state.header_enabled(HeaderItem::Mode),
            Msg::SetMode(other),
        ),
        chip(
            "Pair".to_string(),
            state.header_enabled(HeaderItem::Pair),
            Msg::NotYet("Pair"),
        ),
        chip(
            "Compare".to_string(),
            state.header_enabled(HeaderItem::Compare),
            Msg::NotYet("Compare"),
        ),
        live.into(),
        button::suggested("Apply")
            .on_press_maybe(
                state
                    .header_enabled(HeaderItem::Apply)
                    .then_some(Msg::NotYet("Apply")),
            )
            .into(),
    ]
}

/// The open dialog, if any.
pub fn dialog(state: &State) -> Option<Element<'_, Msg>> {
    let d = match state.dialog.as_ref()? {
        Dialog::Switch => switch(state),
        Dialog::Name {
            purpose,
            name,
            mode,
        } => name_dialog(state, *purpose, name, *mode),
        Dialog::Delete { label, .. } => widget::dialog()
            .title(format!("Delete {label}?"))
            .body("Its saved versions and snapshots are deleted too. This cannot be undone.")
            .primary_action(button::destructive("Delete").on_press(Msg::ConfirmDelete))
            .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog)),
        Dialog::Unsaved(leave) => {
            let body = match leave {
                Leave::Exit => "Save your changes before closing?",
                _ => "Save your changes first?",
            };
            widget::dialog()
                .title("Unsaved changes")
                .body(body)
                .primary_action(button::suggested("Save").on_press(Msg::UnsavedSave))
                .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog))
                .tertiary_action(button::destructive("Discard").on_press(Msg::UnsavedDiscard))
        }
    };
    Some(d.into())
}

fn switch(state: &State) -> widget::Dialog<'_, Msg> {
    let open = state.current.as_ref().map(|c| c.slug());
    let list = state
        .entries
        .iter()
        .fold(widget::Column::new().spacing(2), |col, e| {
            let mut label = match &e.palette {
                Ok((name, mode)) => format!("{name} ({mode})"),
                Err(_) => format!("{} (unreadable)", e.slug),
            };
            if open == Some(e.slug.as_str()) {
                label.push_str(", open");
            }
            col.push(
                button::text(label)
                    .width(Length::Fill)
                    .on_press(Msg::Choose(e.slug.clone())),
            )
        });
    widget::dialog()
        .title("Switch palette")
        .control(widget::scrollable(list).height(Length::Fixed(320.)))
        .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog))
}

fn name_dialog<'a>(
    state: &'a State,
    purpose: NameFor,
    name: &'a str,
    mode: Mode,
) -> widget::Dialog<'a, Msg> {
    let check = state.name_check(name);
    let mut input = widget::text_input("Name", name)
        .on_input(Msg::NameInput)
        .on_submit(|_| Msg::NameConfirm);
    input = match &check {
        Ok(slug) => input.helper_text(format!("Saved as {slug}")),
        Err(e) if !e.is_empty() => input.error(e.clone()),
        Err(_) => input.helper_text("Letters and digits, with single spaces"),
    };
    let mut control = widget::Column::new().spacing(12).push(input);
    if purpose == NameFor::New {
        let modes =
            [Mode::Dark, Mode::Light]
                .into_iter()
                .fold(widget::Row::new().spacing(8), |r, m| {
                    let label = match m {
                        Mode::Dark => "Dark",
                        Mode::Light => "Light",
                    };
                    let b = if m == mode {
                        button::suggested(label)
                    } else {
                        button::standard(label)
                    };
                    r.push(b.on_press(Msg::NameMode(m)))
                });
        control = control.push(modes);
    }
    let (title, action) = match purpose {
        NameFor::New => ("New palette", "Create"),
        NameFor::Duplicate => ("Duplicate palette", "Duplicate"),
    };
    widget::dialog()
        .title(title)
        .control(control)
        .primary_action(
            button::suggested(action).on_press_maybe(check.is_ok().then_some(Msg::NameConfirm)),
        )
        .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog))
}

/// The main area for a palette that cannot be read: its error and text.
pub fn unreadable<'a>(slug: &'a str, error: &'a str, raw: &'a str) -> Element<'a, Msg> {
    let body = widget::Column::new()
        .spacing(12)
        .push(text::title4(format!("{slug} cannot be read")))
        .push(text::body(error))
        .push(text::body(
            "Irida never writes this file. Fix it in a text editor, or delete it from the palette menu.",
        ))
        .push(widget::container(text::monotext(raw)).padding(12).class(cosmic::theme::Container::Card));
    widget::scrollable(body).into()
}
