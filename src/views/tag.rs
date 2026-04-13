//! Tag list and detail views.
//!
//! Tags in Gramps are simple name/color/priority records that can be
//! attached to any primary object via its `tag_list`. The detail view
//! shows the tag's own fields plus a reverse lookup of every primary
//! object tagged with it.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use crate::app::{Message, TagDraft};
use crate::db::Snapshot;
use crate::gramps::Tag;

pub fn row_label(t: &Tag) -> String {
    format!("{}  ·  {}", t.name, t.color)
}

fn matches(t: &Tag, q: &str) -> bool {
    t.name.to_lowercase().contains(q)
}

fn sort_cmp(a: &Tag, b: &Tag) -> Ordering {
    b.priority
        .cmp(&a.priority)
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.tags, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.tags[i]))
        .collect();
    list_pane::view(
        "Tags",
        rows,
        state.selected,
        &state.query,
        "Search tag…",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, tag: &'a Tag) -> Element<'a, Message> {
    let title = text(tag.name.clone()).size(24);
    let meta = row![
        chip(format!("Color {}", tag.color)),
        chip(format!("Priority {}", tag.priority)),
    ]
    .spacing(8);

    let usage = section("Tagged objects", tagged_objects(tag, snap));

    let body = column![
        title,
        meta,
        field("Name", tag.name.clone()),
        usage
    ]
    .spacing(18)
    .padding(24)
    .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn tagged_objects<'a>(tag: &'a Tag, snap: &'a Snapshot) -> Element<'a, Message> {
    let mut col = column![].spacing(4);
    let mut any = false;
    let h = tag.handle.as_str();

    for p in &snap.persons {
        if p.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(
                text(format!(
                    "Person  ·  {}  ·  {}",
                    p.primary_name.display(),
                    p.gramps_id
                ))
                .size(13),
            );
        }
    }
    for f in &snap.families {
        if f.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(text(format!("Family  ·  {}", f.gramps_id)).size(13));
        }
    }
    for e in &snap.events {
        if e.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(text(format!("Event  ·  {}", e.gramps_id)).size(13));
        }
    }
    for pl in &snap.places {
        if pl.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(
                text(format!(
                    "Place  ·  {}  ·  {}",
                    if pl.name.value.is_empty() { &pl.title } else { &pl.name.value },
                    pl.gramps_id
                ))
                .size(13),
            );
        }
    }
    for s in &snap.sources {
        if s.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(text(format!("Source  ·  {}  ·  {}", s.title, s.gramps_id)).size(13));
        }
    }
    for c in &snap.citations {
        if c.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(text(format!("Citation  ·  {}", c.gramps_id)).size(13));
        }
    }
    for m in &snap.media {
        if m.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(text(format!("Media  ·  {}", m.gramps_id)).size(13));
        }
    }
    for n in &snap.notes {
        if n.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(text(format!("Note  ·  {}", n.gramps_id)).size(13));
        }
    }
    for r in &snap.repositories {
        if r.tag_list.iter().any(|t| t == h) {
            any = true;
            col = col.push(text(format!("Repository  ·  {}", r.gramps_id)).size(13));
        }
    }

    if !any {
        return text("(none)").size(13).into();
    }
    col.into()
}

/// Edit-form view for a Tag. Shown in place of the detail pane while
/// the app has an active Tag edit session.
pub fn edit_view<'a>(draft: &'a TagDraft, creating: bool) -> Element<'a, Message> {
    let title = text(if creating { "New tag" } else { "Edit tag" }).size(24);

    let name_field = column![
        text("Name").size(11).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        text_input("Tag name", &draft.name)
            .on_input(Message::EditTagName)
            .padding(6),
    ]
    .spacing(4);

    let color_field = column![
        text("Color (hex)").size(11).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        text_input("#rrggbb", &draft.color)
            .on_input(Message::EditTagColor)
            .padding(6),
    ]
    .spacing(4);

    let priority_field = column![
        text("Priority").size(11).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        text_input("0", &draft.priority_s)
            .on_input(Message::EditTagPriority)
            .padding(6),
    ]
    .spacing(4);

    let body = column![title, name_field, color_field, priority_field]
        .spacing(14)
        .padding(24)
        .align_x(Alignment::Start);
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
