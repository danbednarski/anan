//! Place list and detail views.
//!
//! Gramps places form a tree: every place can point at an enclosing
//! parent via `placeref_list`. In the SQLite schema this is denormalized
//! to an `enclosed_by` column, but we walk the in-memory `placeref_list`
//! so we don't need to add that column to the Rust model yet.
//!
//! The detail view renders a breadcrumb up the parent chain
//! (City → County → State → Country) and a list of immediate children.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use crate::app::Message;
use crate::db::Snapshot;
use crate::gramps::enums::place_type_label;
use crate::gramps::Place;

pub fn row_label(p: &Place) -> String {
    let kind = place_type_label(p.place_type.value).unwrap_or("Custom");
    let name = if p.name.value.is_empty() {
        p.title.as_str()
    } else {
        p.name.value.as_str()
    };
    format!("{name}  ·  {kind}  ·  {}", p.gramps_id)
}

fn matches(p: &Place, q: &str) -> bool {
    let hay = format!(
        "{} {} {} {}",
        p.name.value, p.title, p.code, p.gramps_id
    )
    .to_lowercase();
    hay.contains(q)
}

fn sort_cmp(a: &Place, b: &Place) -> Ordering {
    let na = if a.name.value.is_empty() { &a.title } else { &a.name.value };
    let nb = if b.name.value.is_empty() { &b.title } else { &b.name.value };
    na.to_lowercase().cmp(&nb.to_lowercase())
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.places, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.places[i]))
        .collect();
    list_pane::view(
        "Places",
        rows,
        state.selected,
        &state.query,
        "Search place…",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, place: &'a Place) -> Element<'a, Message> {
    let name = if place.name.value.is_empty() {
        place.title.clone()
    } else {
        place.name.value.clone()
    };
    let title = text(name.clone()).size(24);

    let kind = place_type_label(place.place_type.value).unwrap_or("Custom");
    let meta = row![
        chip(format!("ID {}", place.gramps_id)),
        chip(format!("Type: {}", kind)),
    ]
    .spacing(8);

    let lat_long = if place.lat.is_empty() && place.long.is_empty() {
        None
    } else {
        Some(format!("{}, {}", place.lat, place.long))
    };
    let code = if place.code.is_empty() { None } else { Some(place.code.clone()) };

    let vitals = row![
        field("Coordinates", lat_long.unwrap_or_else(|| "—".to_string())),
        field("Code", code.unwrap_or_else(|| "—".to_string())),
    ]
    .spacing(24);

    let breadcrumb = section("Enclosing places", breadcrumb_block(place, snap));
    let children = section("Contained places", children_block(place, snap));
    let events_here = section("Events here", events_here_block(place, snap));

    let body = column![title, meta, vitals, breadcrumb, children, events_here]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Walk the `placeref_list` chain upward. Cycle-safe via a visited set.
fn breadcrumb_block<'a>(place: &'a Place, snap: &'a Snapshot) -> Element<'a, Message> {
    let mut chain: Vec<String> = Vec::new();
    let mut visited: std::collections::HashSet<&str> = std::collections::HashSet::new();
    visited.insert(place.handle.as_str());

    let mut current = place;
    loop {
        let Some(parent_ref) = current.placeref_list.first() else {
            break;
        };
        if !visited.insert(parent_ref.r#ref.as_str()) {
            break;
        }
        let Some(parent) = snap.place(&parent_ref.r#ref) else {
            break;
        };
        let name = if parent.name.value.is_empty() {
            parent.title.clone()
        } else {
            parent.name.value.clone()
        };
        let kind = place_type_label(parent.place_type.value).unwrap_or("Custom");
        chain.push(format!("{name}  ·  {kind}"));
        current = parent;
    }

    if chain.is_empty() {
        return text("(top-level)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for (i, line) in chain.iter().enumerate() {
        col = col.push(text(format!("{}└ {}", "  ".repeat(i), line)).size(13));
    }
    col.into()
}

fn children_block<'a>(place: &'a Place, snap: &'a Snapshot) -> Element<'a, Message> {
    let mut col = column![].spacing(4);
    let mut any = false;
    for child in &snap.places {
        if child
            .placeref_list
            .first()
            .map(|pr| pr.r#ref == place.handle)
            .unwrap_or(false)
        {
            any = true;
            let name = if child.name.value.is_empty() {
                child.title.clone()
            } else {
                child.name.value.clone()
            };
            let kind = place_type_label(child.place_type.value).unwrap_or("Custom");
            col = col.push(text(format!("{name}  ·  {kind}")).size(13));
        }
    }
    if !any {
        return text("(none)").size(13).into();
    }
    col.into()
}

fn events_here_block<'a>(place: &'a Place, snap: &'a Snapshot) -> Element<'a, Message> {
    let mut col = column![].spacing(4);
    let mut any = false;
    for ev in &snap.events {
        if ev.place == place.handle {
            any = true;
            let kind =
                crate::gramps::enums::event_type_label(ev.r#type.value).unwrap_or("Custom");
            let when = ev
                .date
                .as_ref()
                .map(super::widgets::date_display::format)
                .unwrap_or_default();
            let line = if when.is_empty() {
                format!("{kind}  ·  {}", ev.gramps_id)
            } else {
                format!("{kind}  ·  {when}  ·  {}", ev.gramps_id)
            };
            col = col.push(text(line).size(13));
        }
    }
    if !any {
        return text("(none)").size(13).into();
    }
    col.into()
}
