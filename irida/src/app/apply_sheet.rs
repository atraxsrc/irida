//! The Apply sheet: every installed target with a toggle, broken targets
//! with their error, and the rest folded into one line. Views only read
//! `State` and emit `Msg`.

use cosmic::iced::Length;
use cosmic::widget::{self, button, text};

use super::state::{ApplySheet, Msg, State, sheet_groups};

pub fn view<'a>(state: &'a State, sheet: &'a ApplySheet) -> widget::Dialog<'a, Msg> {
    let groups = sheet_groups(&sheet.targets, &state.settings);
    let not_installed = groups.not_installed_line();
    let mut list = widget::Column::new().spacing(8);
    if let Some(note) = &sheet.pair_note {
        list = list.push(text::caption(note.clone()));
    }
    for (name, on) in groups.toggles {
        let toggle = widget::toggler(on).on_toggle({
            let name = name.clone();
            move |v| Msg::SetTarget(name.clone(), v)
        });
        list = list.push(
            widget::Row::new()
                .spacing(12)
                .push(text::body(name).width(Length::Fill))
                .push(toggle),
        );
    }
    for (name, error) in groups.broken {
        let row = widget::Row::new()
            .spacing(12)
            .push(
                widget::Column::new()
                    .width(Length::Fill)
                    .push(text::body(name))
                    .push(text::caption(error)),
            )
            .push(widget::toggler(false));
        list = list.push(row);
    }
    if let Some(line) = not_installed {
        list = list.push(text::caption(line));
    }
    widget::dialog()
        .title("Apply")
        .body(sheet.applies.as_str())
        .control(widget::scrollable(list).height(Length::Shrink))
        .primary_action(button::suggested("Apply").on_press(Msg::ConfirmApply))
        .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog))
}
