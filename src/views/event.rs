//! Event list and detail views.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use super::widgets::date_display;
use crate::app::Message;
use crate::db::Snapshot;
use crate::gramps::enums::event_type_label;
use crate::gramps::Event;

pub fn row_label(ev: &Event) -> String {
    let kind = event_type_label(ev.r#type.value)
        .map(String::from)
        .unwrap_or_else(|| {
            if ev.r#type.string.is_empty() {
                format!("type {}", ev.r#type.value)
            } else {
                ev.r#type.string.clone()
            }
        });
    let when = ev.date.as_ref().map(date_display::format).unwrap_or_default();
    if when.is_empty() {
        format!("{kind}  ·  {}", ev.gramps_id)
    } else {
        format!("{kind}  ·  {when}  ·  {}", ev.gramps_id)
    }
}

fn sort_cmp(a: &Event, b: &Event) -> Ordering {
    let ya = a.date.as_ref().map(|d| d.primary_year()).unwrap_or(0);
    let yb = b.date.as_ref().map(|d| d.primary_year()).unwrap_or(0);
    ya.cmp(&yb).then_with(|| a.gramps_id.cmp(&b.gramps_id))
}

fn matches(ev: &Event, q: &str) -> bool {
    let label = event_type_label(ev.r#type.value).unwrap_or("");
    let hay = format!(
        "{} {} {} {}",
        label,
        ev.r#type.string,
        ev.description,
        ev.gramps_id
    )
    .to_lowercase();
    hay.contains(q)
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.events, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.events[i]))
        .collect();
    list_pane::view(
        "Events",
        rows,
        state.selected,
        &state.query,
        "Search event…",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, ev: &'a Event) -> Element<'a, Message> {
    let kind = event_type_label(ev.r#type.value)
        .map(String::from)
        .unwrap_or_else(|| {
            if ev.r#type.string.is_empty() {
                format!("type {}", ev.r#type.value)
            } else {
                ev.r#type.string.clone()
            }
        });
    let title = text(kind.clone()).size(24);

    let meta = row![
        chip(format!("ID {}", ev.gramps_id)),
        chip(kind.clone()),
    ]
    .spacing(8);

    let when = ev.date.as_ref().map(date_display::long_form).unwrap_or_default();
    let where_ = snap
        .place(&ev.place)
        .map(|p| {
            if p.name.value.is_empty() {
                p.title.clone()
            } else {
                p.name.value.clone()
            }
        })
        .unwrap_or_else(|| "—".to_string());
    let vitals = row![
        field("When", if when.is_empty() { "—".to_string() } else { when }),
        field("Where", where_),
    ]
    .spacing(24);

    let desc_block: Element<'_, Message> = if ev.description.is_empty() {
        text("").into()
    } else {
        section("Description", text(ev.description.clone()).size(13).into())
    };

    let participants = section("Participants", participants_block(ev, snap));

    let body = column![title, meta, vitals, desc_block, participants]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Walk the persons list and find every EventRef that points at this
/// event. Not indexed — O(persons * refs) — but the dataset is small.
fn participants_block<'a>(ev: &'a Event, snap: &'a Snapshot) -> Element<'a, Message> {
    let mut col = column![].spacing(4);
    let mut any = false;
    for person in &snap.persons {
        for ev_ref in &person.event_ref_list {
            if ev_ref.r#ref == ev.handle {
                any = true;
                let role =
                    crate::gramps::enums::event_role_label(ev_ref.role.value).unwrap_or("Custom");
                col = col.push(
                    text(format!(
                        "{}  ·  {}  ·  {}",
                        person.primary_name.display(),
                        person.gramps_id,
                        role
                    ))
                    .size(13),
                );
            }
        }
    }
    if !any {
        return text("(none)").size(13).into();
    }
    col.into()
}
