//! Event list and detail views.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use super::widgets::date_display;
use super::widgets::date_edit::{self, DateMessages};
use crate::app::{EventDraft, Message};
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

/// Edit-form view for an Event. Type value is numeric for now
/// (see `gramps::enums::event_type_label` for the mapping);
/// description is free-form; place is referenced by Gramps ID; the
/// date uses the shared `date_edit` widget so Citation in Phase 6b
/// can reuse it unchanged.
pub fn edit_view<'a>(draft: &'a EventDraft, creating: bool) -> Element<'a, Message> {
    let title = text(if creating { "New event" } else { "Edit event" }).size(24);
    let label_color = iced::Color::from_rgb(0.5, 0.5, 0.5);
    let label = |s: &'static str| text(s).size(11).color(label_color);

    let type_field = column![
        label("Type value (12=Birth · 13=Death · 15=Baptism · 19=Burial · 42=Residence · …)"),
        text_input("12", &draft.type_value_s)
            .on_input(Message::EditEventType)
            .padding(6)
            .width(Length::Fixed(90.0)),
    ]
    .spacing(4);

    let description_field = column![
        label("Description"),
        text_input("", &draft.description)
            .on_input(Message::EditEventDescription)
            .padding(6),
    ]
    .spacing(4);

    let place_field = column![
        label("Place — Gramps ID (e.g. P0001), blank for none"),
        text_input("P####", &draft.place_gid)
            .on_input(Message::EditEventPlace)
            .padding(6),
    ]
    .spacing(4);

    let date_widget = date_edit::view(
        &draft.date,
        &DateMessages {
            on_year: Message::EditEventDateYear,
            on_month: Message::EditEventDateMonth,
            on_day: Message::EditEventDateDay,
            on_modifier: Message::EditEventDateModifier,
            on_quality: Message::EditEventDateQuality,
            on_text: Message::EditEventDateText,
        },
    );

    let body = column![
        title,
        type_field,
        description_field,
        place_field,
        date_widget,
    ]
    .spacing(16)
    .padding(24)
    .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
