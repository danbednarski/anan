//! Person list and detail views.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use super::widgets::date_display;
use crate::app::Message;
use crate::db::Snapshot;
use crate::gramps::enums::{event_type_label, gender_label};
use crate::gramps::{Event, Family, Person};

pub fn row_label(p: &Person) -> String {
    format!("{}  ·  {}", p.primary_name.display(), p.gramps_id)
}

pub fn primary_surname(p: &Person) -> &str {
    p.primary_name
        .surname_list
        .iter()
        .find(|s| s.primary)
        .or_else(|| p.primary_name.surname_list.first())
        .map(|s| s.surname.as_str())
        .unwrap_or("")
}

fn matches(p: &Person, q: &str) -> bool {
    let hay = format!(
        "{} {} {}",
        p.primary_name.first_name,
        p.primary_name
            .surname_list
            .iter()
            .map(|s| s.surname.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        p.gramps_id,
    )
    .to_lowercase();
    hay.contains(q)
}

fn sort_cmp(a: &Person, b: &Person) -> Ordering {
    let sa = primary_surname(a).to_lowercase();
    let sb = primary_surname(b).to_lowercase();
    sa.cmp(&sb).then_with(|| {
        a.primary_name
            .first_name
            .to_lowercase()
            .cmp(&b.primary_name.first_name.to_lowercase())
    })
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.persons, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.persons[i]))
        .collect();
    list_pane::view(
        "Persons",
        rows,
        state.selected,
        &state.query,
        "Search name…  (⌘F)",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, person: &'a Person) -> Element<'a, Message> {
    let name = text(person.primary_name.display()).size(24);

    let meta = row![
        chip(format!("ID {}", person.gramps_id)),
        chip(format!("Gender: {}", gender_label(person.gender))),
    ]
    .spacing(8);

    let (birth, death) = birth_death(person, snap);
    let vitals = row![
        field("Born", birth.unwrap_or_else(|| "—".to_string())),
        field("Died", death.unwrap_or_else(|| "—".to_string())),
    ]
    .spacing(24);

    let events_block = section("Events", render_event_refs(person, snap));
    let families_block = section("Families", render_family_refs(person, snap));

    let body = column![name, meta, vitals, events_block, families_block]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn birth_death(person: &Person, snap: &Snapshot) -> (Option<String>, Option<String>) {
    let pick = |idx: i32| -> Option<String> {
        if idx < 0 {
            return None;
        }
        let ev_ref = person.event_ref_list.get(idx as usize)?;
        let ev = snap.event(&ev_ref.r#ref)?;
        let d = ev.date.as_ref()?;
        let rendered = date_display::format(d);
        if rendered.is_empty() { None } else { Some(rendered) }
    };
    (pick(person.birth_ref_index), pick(person.death_ref_index))
}

fn render_event_refs<'a>(person: &'a Person, snap: &'a Snapshot) -> Element<'a, Message> {
    if person.event_ref_list.is_empty() {
        return text("(none)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for ev_ref in &person.event_ref_list {
        let Some(ev) = snap.event(&ev_ref.r#ref) else {
            col = col.push(text(format!("(missing event {})", ev_ref.r#ref)).size(13));
            continue;
        };
        col = col.push(text(event_summary(ev, snap)).size(13));
    }
    col.into()
}

fn event_summary(ev: &Event, snap: &Snapshot) -> String {
    let kind = event_type_label(ev.r#type.value)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if ev.r#type.string.is_empty() {
                format!("type {}", ev.r#type.value)
            } else {
                ev.r#type.string.clone()
            }
        });
    let when = ev
        .date
        .as_ref()
        .map(date_display::format)
        .unwrap_or_default();
    let where_ = snap
        .place(&ev.place)
        .map(|p| p.name.value.clone())
        .unwrap_or_default();
    match (when.is_empty(), where_.is_empty()) {
        (true, true) => kind,
        (true, false) => format!("{kind}  ·  {where_}"),
        (false, true) => format!("{kind}  ·  {when}"),
        (false, false) => format!("{kind}  ·  {when}  ·  {where_}"),
    }
}

fn render_family_refs<'a>(person: &'a Person, snap: &'a Snapshot) -> Element<'a, Message> {
    if person.family_list.is_empty() && person.parent_family_list.is_empty() {
        return text("(none)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for handle in &person.parent_family_list {
        if let Some(fam) = snap.family(handle) {
            col = col.push(text(format!("Parents: {}", family_summary(fam, snap))).size(13));
        }
    }
    for handle in &person.family_list {
        if let Some(fam) = snap.family(handle) {
            col = col.push(text(format!("Family: {}", family_summary(fam, snap))).size(13));
        }
    }
    col.into()
}

fn family_summary(fam: &Family, snap: &Snapshot) -> String {
    let name_of = |h: &Option<String>| -> String {
        h.as_ref()
            .and_then(|h| snap.person(h))
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
