//! Media list and detail views.
//!
//! The Phase 1 fixture has zero media rows, so this view is unexercised
//! by real data at the moment. The structure compiles and renders
//! sensibly for an empty list; when the view is visible against a
//! populated tree the user will see rows and can click through to a
//! minimal detail pane.
//!
//! Thumbnail rendering (the "load image from /app/media/" bullet in the
//! plan) is deferred until a tree with media is available — the iced
//! `image` feature is not enabled yet.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use super::widgets::{date_display, date_edit::{self, DateMessages}};
use crate::app::{MediaDraft, Message};
use crate::db::Snapshot;
use crate::gramps::Media;

pub fn row_label(m: &Media) -> String {
    let desc = if m.desc.is_empty() { "(no description)" } else { m.desc.as_str() };
    format!("{desc}  ·  {}", m.gramps_id)
}

fn matches(m: &Media, q: &str) -> bool {
    format!("{} {} {} {}", m.desc, m.path, m.mime, m.gramps_id)
        .to_lowercase()
        .contains(q)
}

fn sort_cmp(a: &Media, b: &Media) -> Ordering {
    a.desc.to_lowercase().cmp(&b.desc.to_lowercase())
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.media, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.media[i]))
        .collect();
    list_pane::view(
        "Media",
        rows,
        state.selected,
        &state.query,
        "Search media…",
    )
}

pub fn detail_view<'a>(_snap: &'a Snapshot, m: &'a Media) -> Element<'a, Message> {
    let title = text(if m.desc.is_empty() {
        format!("Media {}", m.gramps_id)
    } else {
        m.desc.clone()
    })
    .size(24);

    let meta = row![
        chip(format!("ID {}", m.gramps_id)),
        chip(format!("MIME: {}", if m.mime.is_empty() { "?" } else { m.mime.as_str() })),
    ]
    .spacing(8);

    let when = m
        .date
        .as_ref()
        .map(date_display::format)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "—".to_string());

    let vitals = row![
        field("Path", if m.path.is_empty() { "—".to_string() } else { m.path.clone() }),
        field("Date", when),
    ]
    .spacing(24);

    let checksum_block: Element<'_, Message> = if m.checksum.is_empty() {
        text("").into()
    } else {
        section("Checksum", text(m.checksum.clone()).size(12).into())
    };

    let body = column![title, meta, vitals, checksum_block]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub fn edit_view<'a>(draft: &'a MediaDraft, creating: bool) -> Element<'a, Message> {
    let title = text(if creating { "New media" } else { "Edit media" }).size(24);
    let label_color = iced::Color::from_rgb(0.5, 0.5, 0.5);
    let label = |s: &'static str| text(s).size(11).color(label_color);

    let path_field = column![
        label("File path"),
        text_input("/path/to/file.jpg", &draft.path)
            .on_input(Message::EditMediaPath)
            .padding(6),
    ]
    .spacing(4);

    let mime_field = column![
        label("MIME type"),
        text_input("image/jpeg", &draft.mime)
            .on_input(Message::EditMediaMime)
            .padding(6)
            .width(Length::Fixed(180.0)),
    ]
    .spacing(4);

    let desc_field = column![
        label("Description"),
        text_input("Photo description", &draft.desc)
            .on_input(Message::EditMediaDesc)
            .padding(6),
    ]
    .spacing(4);

    let date_widget = date_edit::view(
        &draft.date,
        &DateMessages {
            on_year: Message::EditMediaDateYear,
            on_month: Message::EditMediaDateMonth,
            on_day: Message::EditMediaDateDay,
            on_modifier: Message::EditMediaDateModifier,
            on_quality: Message::EditMediaDateQuality,
            on_text: Message::EditMediaDateText,
        },
    );

    let body = column![title, path_field, mime_field, desc_field, date_widget]
        .spacing(14)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
