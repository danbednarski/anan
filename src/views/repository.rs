//! Repository list and detail views.
//!
//! The Phase 1 fixture has zero repository rows; the view is defined
//! for parity with Gramps core and will exercise once a fuller tree is
//! loaded.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use crate::app::{Message, RepoDraft};
use crate::db::Snapshot;
use crate::gramps::enums::repository_type_label;
use crate::gramps::Repository;

pub fn row_label(r: &Repository) -> String {
    let kind = repository_type_label(r.r#type.value).unwrap_or("Custom");
    format!("{}  ·  {kind}  ·  {}", r.name, r.gramps_id)
}

fn matches(r: &Repository, q: &str) -> bool {
    format!("{} {}", r.name, r.gramps_id).to_lowercase().contains(q)
}

fn sort_cmp(a: &Repository, b: &Repository) -> Ordering {
    a.name.to_lowercase().cmp(&b.name.to_lowercase())
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.repositories, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.repositories[i]))
        .collect();
    list_pane::view(
        "Repositories",
        rows,
        state.selected,
        &state.query,
        "Search repository…",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, repo: &'a Repository) -> Element<'a, Message> {
    let title = text(if repo.name.is_empty() {
        format!("Repository {}", repo.gramps_id)
    } else {
        repo.name.clone()
    })
    .size(24);

    let kind = repository_type_label(repo.r#type.value).unwrap_or("Custom");
    let meta = row![
        chip(format!("ID {}", repo.gramps_id)),
        chip(format!("Type: {}", kind)),
    ]
    .spacing(8);

    let urls_block: Element<'_, Message> = if repo.urls.is_empty() {
        text("").into()
    } else {
        let mut col = column![].spacing(4);
        for u in &repo.urls {
            col = col.push(text(format!("{}  ·  {}", u.path, u.desc)).size(13));
        }
        section("URLs", col.into())
    };

    let addr_block: Element<'_, Message> = if repo.address_list.is_empty() {
        text("").into()
    } else {
        let mut col = column![].spacing(4);
        for a in &repo.address_list {
            let line = [
                a.street.as_str(),
                a.city.as_str(),
                a.state.as_str(),
                a.country.as_str(),
                a.postal.as_str(),
            ]
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
            col = col.push(text(line).size(13));
        }
        section("Addresses", col.into())
    };

    let sources = section("Sources held here", sources_block(repo, snap));

    let body = column![
        title,
        meta,
        field("Name", repo.name.clone()),
        addr_block,
        urls_block,
        sources
    ]
    .spacing(18)
    .padding(24)
    .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn sources_block<'a>(repo: &'a Repository, snap: &'a Snapshot) -> Element<'a, Message> {
    let mut col = column![].spacing(4);
    let mut any = false;
    for src in &snap.sources {
        if src.reporef_list.iter().any(|r| r.r#ref == repo.handle) {
            any = true;
            col = col.push(text(format!("{}  ·  {}", src.title, src.gramps_id)).size(13));
        }
    }
    if !any {
        return text("(none)").size(13).into();
    }
    col.into()
}

/// Edit-form view for a Repository. Only `name` and `type` are
/// editable in Phase 4; address / url / note / tag lists are
/// preserved on update but not exposed in the UI yet.
pub fn edit_view<'a>(draft: &'a RepoDraft, creating: bool) -> Element<'a, Message> {
    let title = text(if creating { "New repository" } else { "Edit repository" }).size(24);

    let name_field = column![
        text("Name").size(11).color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        text_input("Repository name", &draft.name)
            .on_input(Message::EditRepoName)
            .padding(6),
    ]
    .spacing(4);

    let type_field = column![
        text("Type value (e.g. 1=Library)")
            .size(11)
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        text_input("1", &draft.type_value_s)
            .on_input(Message::EditRepoType)
            .padding(6),
    ]
    .spacing(4);

    let body = column![title, name_field, type_field]
        .spacing(14)
        .padding(24)
        .align_x(Alignment::Start);
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
