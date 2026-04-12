//! Family list and detail views.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, section};
use super::list_pane::{self, ListState};
use super::person;
use super::widgets::date_display;
use crate::app::Message;
use crate::db::Snapshot;
use crate::gramps::enums::{child_ref_label, event_type_label, family_rel_label};
use crate::gramps::{Family, Person};

pub fn row_label(fam: &Family, snap: &Snapshot) -> String {
    let name_of = |h: &Option<String>| {
        h.as_ref()
            .and_then(|h| snap.person(h))
            .map(|p| p.primary_name.display())
            .unwrap_or_else(|| "?".to_string())
    };
    format!(
        "{} & {}  ·  {}",
        name_of(&fam.father_handle),
        name_of(&fam.mother_handle),
        fam.gramps_id
    )
}

fn sort_key(fam: &Family, snap: &Snapshot) -> String {
    // Sort by father's surname then family id for a stable, readable order.
    let father_surname = fam
        .father_handle
        .as_ref()
        .and_then(|h| snap.person(h))
        .map(|p| person::primary_surname(p).to_lowercase())
        .unwrap_or_default();
    format!("{father_surname}:{}", fam.gramps_id)
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    let q = state.query.trim().to_lowercase();
    state.order = snap
        .families
        .iter()
        .enumerate()
        .filter(|(_, f)| q.is_empty() || row_label(f, snap).to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect();
    state.order.sort_by(|&a, &b| {
        sort_key(&snap.families[a], snap).cmp(&sort_key(&snap.families[b], snap))
    });
    state.clamp_selection();
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.families[i], snap))
        .collect();
    list_pane::view(
        "Families",
        rows,
        state.selected,
        &state.query,
        "Search family…",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, fam: &'a Family) -> Element<'a, Message> {
    let title = text(format!("Family {}", fam.gramps_id)).size(24);

    let rel_label = family_rel_label(fam.r#type.value)
        .map(String::from)
        .unwrap_or_else(|| {
            if fam.r#type.string.is_empty() {
                format!("type {}", fam.r#type.value)
            } else {
                fam.r#type.string.clone()
            }
        });
    let meta = row![
        chip(format!("ID {}", fam.gramps_id)),
        chip(format!("Rel: {}", rel_label)),
    ]
    .spacing(8);

    let parents = section("Parents", parents_block(fam, snap));
    let children = section("Children", children_block(fam, snap));
    let events = section("Family events", events_block(fam, snap));

    let body = column![title, meta, parents, children, events]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn parents_block<'a>(fam: &'a Family, snap: &'a Snapshot) -> Element<'a, Message> {
    let line = |label: &str, h: &Option<String>| -> String {
        let p: Option<&Person> = h.as_ref().and_then(|h| snap.person(h));
        match p {
            Some(p) => format!("{label}: {}  ·  {}", p.primary_name.display(), p.gramps_id),
            None => format!("{label}: —"),
        }
    };
    column![
        text(line("Father", &fam.father_handle)).size(13),
        text(line("Mother", &fam.mother_handle)).size(13),
    ]
    .spacing(4)
    .into()
}

fn children_block<'a>(fam: &'a Family, snap: &'a Snapshot) -> Element<'a, Message> {
    if fam.child_ref_list.is_empty() {
        return text("(none)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for child_ref in &fam.child_ref_list {
        let name = snap
            .person(&child_ref.r#ref)
            .map(|p| p.primary_name.display())
            .unwrap_or_else(|| "?".to_string());
        let frel = child_ref_label(child_ref.frel.value).unwrap_or("Custom");
        let mrel = child_ref_label(child_ref.mrel.value).unwrap_or("Custom");
        col = col.push(text(format!("{name}  ·  {frel}/{mrel}")).size(13));
    }
    col.into()
}

fn events_block<'a>(fam: &'a Family, snap: &'a Snapshot) -> Element<'a, Message> {
    if fam.event_ref_list.is_empty() {
        return text("(none)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for ev_ref in &fam.event_ref_list {
        let Some(ev) = snap.event(&ev_ref.r#ref) else {
            continue;
        };
        let kind = event_type_label(ev.r#type.value).unwrap_or("Custom").to_string();
        let when = ev.date.as_ref().map(date_display::format).unwrap_or_default();
        let where_ = snap.place(&ev.place).map(|p| p.name.value.clone()).unwrap_or_default();
        let line = match (when.is_empty(), where_.is_empty()) {
            (true, true) => kind,
            (true, false) => format!("{kind}  ·  {where_}"),
            (false, true) => format!("{kind}  ·  {when}"),
            (false, false) => format!("{kind}  ·  {when}  ·  {where_}"),
        };
        col = col.push(text(line).size(13));
    }
    col.into()
}

/// Shared ordering helper for other views that need "family → rough name"
/// — used by citation views that link back to reporting sources, etc.
#[allow(dead_code)]
pub fn _family_cmp(a: &Family, b: &Family) -> Ordering {
    a.gramps_id.cmp(&b.gramps_id)
}
