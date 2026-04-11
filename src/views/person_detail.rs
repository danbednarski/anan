//! Right-hand pane: detail view for the selected person.
//!
//! Shows name, gramps id, gender, birth/death dates, every event ref
//! resolved against `snapshot.events`, and every family ref resolved
//! against `snapshot.families`. No editing controls — this is a read-only
//! view for Phase 2.

use std::collections::HashMap;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::gramps::enums::{event_type_label, gender_label};
use crate::gramps::{Event, Family, Person, Place};

pub fn view<'a>(
    person: &'a Person,
    events: &'a HashMap<String, Event>,
    families: &'a HashMap<String, Family>,
    places: &'a HashMap<String, Place>,
    persons: &'a [Person],
    persons_by_handle: &'a HashMap<String, usize>,
) -> Element<'a, Message> {
    let name = text(person.primary_name.display()).size(24);

    let meta = row![
        chip(format!("ID {}", person.gramps_id)),
        chip(format!("Gender: {}", gender_label(person.gender))),
    ]
    .spacing(8);

    let (birth, death) = birth_death(person, events);
    let vitals = row![
        labelled("Born", birth.unwrap_or_else(|| "—".to_string())),
        labelled("Died", death.unwrap_or_else(|| "—".to_string())),
    ]
    .spacing(24);

    let events_block = section(
        "Events",
        render_event_list(person, events, places),
    );

    let families_block = section(
        "Families",
        render_family_list(person, families, persons, persons_by_handle),
    );

    let body = column![name, meta, vitals, events_block, families_block]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Build the "no person selected" placeholder.
pub fn placeholder<'a>() -> Element<'a, Message> {
    container(
        text("Select a person from the list to see details")
            .size(16)
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

fn chip<'a>(label: String) -> Element<'a, Message> {
    container(text(label).size(12))
        .padding([4, 10])
        .style(|theme: &iced::Theme| {
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

fn labelled<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    column![
        text(label).size(11).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        text(value).size(15),
    ]
    .spacing(2)
    .into()
}

fn section<'a>(title: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    column![
        text(title).size(14).color(iced::Color::from_rgb(0.4, 0.4, 0.4)),
        body,
    ]
    .spacing(6)
    .into()
}

fn birth_death(
    person: &Person,
    events: &HashMap<String, Event>,
) -> (Option<String>, Option<String>) {
    let pick = |idx: i32| -> Option<String> {
        if idx < 0 {
            return None;
        }
        let ev_ref = person.event_ref_list.get(idx as usize)?;
        let ev = events.get(&ev_ref.r#ref)?;
        Some(format_event_date(ev))
    };
    (pick(person.birth_ref_index), pick(person.death_ref_index))
}

fn format_event_date(ev: &Event) -> String {
    match &ev.date {
        Some(d) if !d.is_empty() => {
            let year = d.primary_year();
            if year != 0 {
                year.to_string()
            } else if !d.text.is_empty() {
                d.text.clone()
            } else {
                "?".to_string()
            }
        }
        _ => "?".to_string(),
    }
}

fn render_event_list<'a>(
    person: &'a Person,
    events: &'a HashMap<String, Event>,
    places: &'a HashMap<String, Place>,
) -> Element<'a, Message> {
    if person.event_ref_list.is_empty() {
        return text("(none)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for ev_ref in &person.event_ref_list {
        let Some(ev) = events.get(&ev_ref.r#ref) else {
            col = col.push(text(format!("(missing event {})", ev_ref.r#ref)).size(13));
            continue;
        };
        let kind = event_type_label(ev.r#type.value)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                if ev.r#type.string.is_empty() {
                    format!("type {}", ev.r#type.value)
                } else {
                    ev.r#type.string.clone()
                }
            });
        let when = format_event_date(ev);
        let where_ = places
            .get(&ev.place)
            .map(|p| p.name.value.clone())
            .unwrap_or_default();
        let line = if where_.is_empty() {
            format!("{}  ·  {}", kind, when)
        } else {
            format!("{}  ·  {}  ·  {}", kind, when, where_)
        };
        col = col.push(text(line).size(13));
    }
    col.into()
}

fn render_family_list<'a>(
    person: &'a Person,
    families: &'a HashMap<String, Family>,
    persons: &'a [Person],
    persons_by_handle: &'a HashMap<String, usize>,
) -> Element<'a, Message> {
    if person.family_list.is_empty() && person.parent_family_list.is_empty() {
        return text("(none)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for handle in &person.parent_family_list {
        if let Some(fam) = families.get(handle) {
            col = col.push(text(format!(
                "Parents: {}",
                family_summary(fam, persons, persons_by_handle)
            )).size(13));
        }
    }
    for handle in &person.family_list {
        if let Some(fam) = families.get(handle) {
            col = col.push(text(format!(
                "Family: {}",
                family_summary(fam, persons, persons_by_handle)
            )).size(13));
        }
    }
    col.into()
}

fn family_summary(
    fam: &Family,
    persons: &[Person],
    persons_by_handle: &HashMap<String, usize>,
) -> String {
    let name_of = |h: &Option<String>| -> String {
        h.as_ref()
            .and_then(|h| persons_by_handle.get(h))
            .and_then(|&i| persons.get(i))
            .map(|p| p.primary_name.display())
            .unwrap_or_else(|| "?".to_string())
    };
    format!(
        "{}  &  {}  ({} children)",
        name_of(&fam.father_handle),
        name_of(&fam.mother_handle),
        fam.child_ref_list.len()
    )
}
