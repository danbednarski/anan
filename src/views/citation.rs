//! Citation list and detail views.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use super::widgets::{date_display, date_edit::{self, DateMessages}};
use crate::app::{CitationDraft, Message};
use crate::db::Snapshot;
use crate::gramps::Citation;

/// Labels mirror the web app's confidence scale (gen/lib/citation.py).
fn confidence_label(c: i32) -> &'static str {
    match c {
        0 => "very low",
        1 => "low",
        2 => "normal",
        3 => "high",
        4 => "very high",
        _ => "?",
    }
}

pub fn row_label(cit: &Citation, snap: &Snapshot) -> String {
    let src_title = snap
        .source(&cit.source_handle)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| "(orphan)".to_string());
    let page = if cit.page.is_empty() {
        "(no page)"
    } else {
        cit.page.as_str()
    };
    format!("{src_title}  ·  {page}  ·  {}", cit.gramps_id)
}

fn matches(cit: &Citation, q: &str) -> bool {
    format!("{} {}", cit.page, cit.gramps_id).to_lowercase().contains(q)
}

fn sort_cmp(a: &Citation, b: &Citation) -> Ordering {
    a.gramps_id.cmp(&b.gramps_id)
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.citations, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.citations[i], snap))
        .collect();
    list_pane::view(
        "Citations",
        rows,
        state.selected,
        &state.query,
        "Search citation…",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, cit: &'a Citation) -> Element<'a, Message> {
    let title = text(format!("Citation {}", cit.gramps_id)).size(24);

    let meta = row![
        chip(format!("ID {}", cit.gramps_id)),
        chip(format!("Conf: {}", confidence_label(cit.confidence))),
    ]
    .spacing(8);

    let src_title = snap
        .source(&cit.source_handle)
        .map(|s| {
            if s.title.is_empty() {
                format!("(untitled {})", s.gramps_id)
            } else {
                s.title.clone()
            }
        })
        .unwrap_or_else(|| "(orphan)".to_string());

    let when = cit
        .date
        .as_ref()
        .map(date_display::format)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "—".to_string());

    let vitals = row![
        field("Source", src_title),
        field("Date", when),
    ]
    .spacing(24);

    let page_block: Element<'_, Message> = if cit.page.is_empty() {
        text("").into()
    } else {
        section("Page / URL", text(cit.page.clone()).size(13).into())
    };

    let body = column![title, meta, vitals, page_block]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub fn edit_view<'a>(draft: &'a CitationDraft, creating: bool) -> Element<'a, Message> {
    let title = text(if creating { "New citation" } else { "Edit citation" }).size(24);
    let label_color = iced::Color::from_rgb(0.5, 0.5, 0.5);
    let label = |s: &'static str| text(s).size(11).color(label_color);

    let source_field = column![
        label("Source - Gramps ID (e.g. S0001)"),
        text_input("S####", &draft.source_gid)
            .on_input(Message::EditCitationSource)
            .padding(6),
    ]
    .spacing(4);

    let page_field = column![
        label("Page / URL"),
        text_input("e.g. p. 42 or https://...", &draft.page)
            .on_input(Message::EditCitationPage)
            .padding(6),
    ]
    .spacing(4);

    let confidence_field = column![
        label("Confidence (0 very low .. 4 very high)"),
        text_input("2", &draft.confidence_s)
            .on_input(Message::EditCitationConfidence)
            .padding(6)
            .width(Length::Fixed(60.0)),
    ]
    .spacing(4);

    let date_widget = date_edit::view(
        &draft.date,
        &DateMessages {
            on_year: Message::EditCitationDateYear,
            on_month: Message::EditCitationDateMonth,
            on_day: Message::EditCitationDateDay,
            on_modifier: Message::EditCitationDateModifier,
            on_quality: Message::EditCitationDateQuality,
            on_text: Message::EditCitationDateText,
        },
    );

    let body = column![title, source_field, page_field, confidence_field, date_widget]
        .spacing(14)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
