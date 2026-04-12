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

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use super::widgets::date_display;
use crate::app::Message;
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
