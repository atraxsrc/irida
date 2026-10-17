//! The split preview (Compare and the pair) and its dialogs: Pair with...
//! and the snapshot name. Views only read `State` and emit `Msg`.

use cosmic::Element;
use cosmic::iced::Length;
use cosmic::widget::{self, button, text};

use irida_core::apply::TargetInfo;

use super::caption;
use super::preview;
use super::scene::scene;
use super::state::{Msg, PairChoice, SplitColumn, SplitView, State, VersionChoice};

/// Two previews side by side, each under its label.
pub fn view(v: SplitView, targets: &[TargetInfo]) -> Element<'static, Msg> {
    let left_head = match v.versions {
        Some(choice) => versions(choice),
        None => text::heading(v.left.label.clone()).into(),
    };
    let right_head = text::heading(v.right.label.clone()).into();
    widget::Row::new()
        .spacing(16)
        .push(column(left_head, v.left, targets))
        .push(column(right_head, v.right, targets))
        .into()
}

fn column(
    head: Element<'static, Msg>,
    c: SplitColumn,
    targets: &[TargetInfo],
) -> Element<'static, Msg> {
    let body: Element<'static, Msg> = match c.palette {
        Some(p) => widget::Column::new()
            .spacing(8)
            .push(preview::view(scene(&p)))
            .push(caption::line(targets))
            .into(),
        None => text::body(c.note.unwrap_or_default()).into(),
    };
    widget::Column::new()
        .spacing(8)
        .width(Length::FillPortion(1))
        .push(head)
        .push(body)
        .into()
}

/// Compare's left label: the versions, newest first, and the snapshot
/// buttons for the shown one.
fn versions(choice: VersionChoice) -> Element<'static, Msg> {
    let named = choice.snapshot.is_some();
    let pin = if named {
        "Rename snapshot"
    } else {
        "Pin as snapshot"
    };
    let mut row = widget::Row::new()
        .spacing(8)
        .push(widget::dropdown(
            choice.labels,
            choice.selected,
            Msg::CompareVersion,
        ))
        .push(button::text(pin).on_press(Msg::ShowPinSnapshot));
    if named {
        row = row.push(button::text("Remove snapshot name").on_press(Msg::UnpinSnapshot));
    }
    row.into()
}

/// Pair with...: every palette of the other mode; one that is paired
/// already asks before it is taken.
pub fn pair_dialog<'a>(
    state: &'a State,
    choices: &'a [PairChoice],
    confirm: Option<&'a str>,
) -> widget::Dialog<'a, Msg> {
    let name = state
        .current
        .as_ref()
        .and_then(|c| c.working())
        .map_or("", |p| p.name.as_str());
    if let Some(chosen) = confirm.and_then(|s| choices.iter().find(|c| c.slug == s)) {
        let other = chosen.paired_with.as_deref().unwrap_or_default();
        return widget::dialog()
            .title(format!("Pair {name} with {}?", chosen.name))
            .body(format!(
                "{} is paired with {other}. Pairing it with {name} unpairs {other}.",
                chosen.name
            ))
            .primary_action(button::suggested("Pair").on_press(Msg::PairConfirm))
            .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog));
    }
    let list = choices
        .iter()
        .fold(widget::Column::new().spacing(2), |col, c| {
            col.push(
                button::text(c.label())
                    .width(Length::Fill)
                    .on_press(Msg::PairChoose(c.slug.clone())),
            )
        });
    widget::dialog()
        .title(format!("Pair {name} with..."))
        .body("Apply and export use both halves of a pair.")
        .control(widget::scrollable(list).height(Length::Shrink))
        .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog))
}

/// A name for saved version `n`.
pub fn snapshot_dialog<'a>(state: &'a State, n: u32, name: &'a str) -> widget::Dialog<'a, Msg> {
    let check = state.snapshot_check();
    let mut input = widget::text_input("Name", name)
        .on_input(Msg::SnapshotInput)
        .on_submit(|_| Msg::SnapshotConfirm);
    input = match &check {
        Err(e) if !e.is_empty() => input.error(e.clone()),
        _ => input.helper_text("Like Warm accent"),
    };
    widget::dialog()
        .title(format!("Pin version {n} as a snapshot"))
        .control(input)
        .primary_action(
            button::suggested("Pin").on_press_maybe(check.is_ok().then_some(Msg::SnapshotConfirm)),
        )
        .secondary_action(button::standard("Cancel").on_press(Msg::CloseDialog))
}
