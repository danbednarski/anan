//! Note list and detail views.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, section};
use super::list_pane::{self, ListState};
use super::widgets::styled_text;
use crate::app::Message;
use crate::db::Snapshot;
use crate::gramps::enums::note_type_label;
use crate::gramps::Note;

pub fn row_label(n: &Note) -> String {
    let kind = note_type_label(n.r#type.value).unwrap_or("Custom");
    let preview: String = n.text.string.chars().take(50).collect();
    let preview = if n.text.string.chars().count() > 50 {
        format!("{preview}…")
    } else {
        preview
    };
    format!("[{kind}]  {preview}  ·  {}", n.gramps_id)
}

fn matches(n: &Note, q: &str) -> bool {
    format!("{} {}", n.text.string, n.gramps_id).to_lowercase().contains(q)
}

fn sort_cmp(a: &Note, b: &Note) -> Ordering {
    a.gramps_id.cmp(&b.gramps_id)
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.notes, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.notes[i]))
        .collect();
    list_pane::view(
        "Notes",
        rows,
        state.selected,
        &state.query,
        "Search note…",
    )
}

pub fn detail_view<'a>(_snap: &'a Snapshot, note: &'a Note) -> Element<'a, Message> {
    let kind = note_type_label(note.r#type.value)
        .map(String::from)
        .unwrap_or_else(|| {
            if note.r#type.string.is_empty() {
                format!("type {}", note.r#type.value)
            } else {
                note.r#type.string.clone()
            }
        });
    let title = text(format!("Note {}", note.gramps_id)).size(24);

    let meta = row![
        chip(format!("ID {}", note.gramps_id)),
        chip(format!("Type: {}", kind)),
    ]
    .spacing(8);

    // Plain-text projection; see widgets/styled_text.rs.
    let body_text = styled_text::render_plain(&note.text).to_string();
    let body_section = section("Text", text(body_text).size(14).into());

    let body = column![title, meta, body_section]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
