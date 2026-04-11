//! Left-hand pane: filterable person list.
//!
//! Displays every person in the currently-loaded snapshot, sorted by the
//! primary surname (then given name). A text input at the top filters by
//! substring match on given + surname. Clicking a row selects the person
//! and the detail pane updates.

use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Theme};

use crate::app::{Message, SEARCH_INPUT_ID};
use crate::gramps::Person;

/// Build the list pane.
///
/// - `persons` is the full unsorted snapshot.
/// - `order` is the list of indices into `persons` in display order
///   (already filtered and sorted by `app::App`).
/// - `selected` is the position *within `order`* of the highlighted row,
///   or `None` if nothing is selected.
pub fn view<'a>(
    persons: &'a [Person],
    order: &'a [usize],
    selected: Option<usize>,
    query: &'a str,
) -> Element<'a, Message> {
    let header = row![
        text(format!("{} persons", order.len())).size(14),
    ]
    .padding(6);

    let search = text_input("Search name…  (⌘F)", query)
        .id(SEARCH_INPUT_ID)
        .on_input(Message::SearchChanged)
        .padding(6);

    let mut list = column![].spacing(2).padding(6);
    for (display_idx, &person_idx) in order.iter().enumerate() {
        let person = &persons[person_idx];
        let label = format!(
            "{}  ·  {}",
            person.primary_name.display(),
            person.gramps_id
        );
        let is_selected = selected == Some(display_idx);
        let btn = button(text(label).size(13))
            .width(Length::Fill)
            .on_press(Message::SelectPerson(display_idx))
            .style(move |theme: &Theme, status| row_style(theme, status, is_selected));
        list = list.push(btn);
    }

    let body = scrollable(list).height(Length::Fill).width(Length::Fill);

    container(
        column![header, search, body]
            .spacing(4)
            .align_x(Alignment::Start),
    )
    .width(Length::Fixed(320.0))
    .height(Length::Fill)
    .into()
}

/// Button styling: highlight the selected row, subtle hover otherwise.
fn row_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: None,
        text_color: palette.background.base.text,
        border: iced::Border {
            color: iced::Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: iced::Shadow::default(),
    };
    if selected {
        return button::Style {
            background: Some(iced::Background::Color(palette.primary.base.color)),
            text_color: palette.primary.base.text,
            ..base
        };
    }
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(iced::Background::Color(palette.background.weak.color)),
            ..base
        },
        _ => base,
    }
}
