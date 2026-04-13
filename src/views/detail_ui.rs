//! Tiny layout primitives shared across every detail view: chips,
//! labelled key/value fields, and section headers.

use iced::widget::{column, container, text};
use iced::{Element, Theme};

use crate::app::Message;

/// Pill-style chip for a short inline fact.
pub fn chip<'a>(label: String) -> Element<'a, Message> {
    container(text(label).size(12))
        .padding([4, 10])
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced::Background::Color(palette.background.weak.color)),
                border: iced::Border {
                    color: iced::Color::TRANSPARENT,
                    width: 0.0,
                    radius: 999.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

/// "Label above value" two-line field used in detail headers.
pub fn field<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    column![
        text(label).size(11).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        text(value).size(15),
    ]
    .spacing(2)
    .into()
}

/// Section header + body, used to group related rows in a detail view.
pub fn section<'a>(title: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    column![
        text(title).size(14).color(iced::Color::from_rgb(0.4, 0.4, 0.4)),
        body,
    ]
    .spacing(6)
    .into()
}

/// Utility: render a `Vec<String>` as a column of small `text` widgets,
/// or a single "(none)" line when the vec is empty.
pub fn string_list<'a>(items: Vec<String>) -> Element<'a, Message> {
    if items.is_empty() {
        return text("(none)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for item in items {
        col = col.push(text(item).size(13));
    }
    col.into()
}

/// Placeholder for empty detail pane.
pub fn empty<'a>(msg: &'a str) -> Element<'a, Message> {
    container(
        text(msg.to_string())
            .size(16)
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
    )
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
    .center_x(iced::Length::Fill)
    .center_y(iced::Length::Fill)
    .into()
}
