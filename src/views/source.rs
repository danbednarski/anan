//! Source list and detail views.

use std::cmp::Ordering;

use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use super::detail_ui::{chip, field, section};
use super::list_pane::{self, ListState};
use crate::app::{Message, SourceDraft};
use crate::db::Snapshot;
use crate::gramps::Source;

pub fn row_label(s: &Source) -> String {
    let author = if s.author.is_empty() {
        String::new()
    } else {
        format!("  ·  {}", s.author)
    };
    format!("{}{author}  ·  {}", s.title, s.gramps_id)
}

fn matches(s: &Source, q: &str) -> bool {
    format!("{} {} {} {}", s.title, s.author, s.pubinfo, s.gramps_id)
        .to_lowercase()
        .contains(q)
}

fn sort_cmp(a: &Source, b: &Source) -> Ordering {
    a.title.to_lowercase().cmp(&b.title.to_lowercase())
}

pub fn recompute(snap: &Snapshot, state: &mut ListState) {
    list_pane::recompute(&snap.sources, state, matches, sort_cmp);
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a ListState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .order
        .iter()
        .map(|&i| row_label(&snap.sources[i]))
        .collect();
    list_pane::view(
        "Sources",
        rows,
        state.selected,
        &state.query,
        "Search source…",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, src: &'a Source) -> Element<'a, Message> {
    let title = text(if src.title.is_empty() {
        format!("(untitled source {})", src.gramps_id)
    } else {
        src.title.clone()
    })
    .size(24);

    let meta = row![chip(format!("ID {}", src.gramps_id))].spacing(8);

    let vitals = row![
        field(
            "Author",
            if src.author.is_empty() {
                "—".to_string()
            } else {
                src.author.clone()
            }
        ),
        field(
            "Abbrev",
            if src.abbrev.is_empty() {
                "—".to_string()
            } else {
                src.abbrev.clone()
            }
        ),
    ]
    .spacing(24);

    let pubinfo_block: Element<'_, Message> = if src.pubinfo.is_empty() {
        text("").into()
    } else {
        section("Publication info", text(src.pubinfo.clone()).size(13).into())
    };

    let citations = section("Citations of this source", citations_block(src, snap));
    let repositories = section("Repositories", repositories_block(src, snap));

    let body = column![title, meta, vitals, pubinfo_block, citations, repositories]
        .spacing(18)
        .padding(24)
        .align_x(Alignment::Start);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn citations_block<'a>(src: &'a Source, snap: &'a Snapshot) -> Element<'a, Message> {
    let mut col = column![].spacing(4);
    let mut any = false;
    for cit in &snap.citations {
        if cit.source_handle == src.handle {
            any = true;
            let page = if cit.page.is_empty() {
                "(no page)".to_string()
            } else {
                cit.page.clone()
            };
            col = col.push(text(format!("{page}  ·  {}", cit.gramps_id)).size(13));
        }
    }
    if !any {
        return text("(none)").size(13).into();
    }
    col.into()
}

fn repositories_block<'a>(src: &'a Source, snap: &'a Snapshot) -> Element<'a, Message> {
    if src.reporef_list.is_empty() {
        return text("(none)").size(13).into();
    }
    let mut col = column![].spacing(4);
    for repo_ref in &src.reporef_list {
        let name = snap
            .repository(&repo_ref.r#ref)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| repo_ref.r#ref.clone());
        let call = if repo_ref.call_number.is_empty() {
            String::new()
        } else {
            format!("  ·  #{}", repo_ref.call_number)
        };
        col = col.push(text(format!("{name}{call}")).size(13));
    }
    col.into()
}

pub fn edit_view<'a>(draft: &'a SourceDraft, creating: bool) -> Element<'a, Message> {
    let title = text(if creating { "New source" } else { "Edit source" }).size(24);
    let label_color = iced::Color::from_rgb(0.5, 0.5, 0.5);
    let label = |s: &'static str| text(s).size(11).color(label_color);

    let title_field = column![
        label("Title"),
        text_input("Source title", &draft.title)
            .on_input(Message::EditSourceTitle)
            .padding(6),
    ]
    .spacing(4);
    let author_field = column![
        label("Author"),
        text_input("Author name", &draft.author)
            .on_input(Message::EditSourceAuthor)
            .padding(6),
    ]
    .spacing(4);
    let pubinfo_field = column![
        label("Publication info"),
        text_input("Publisher, year, etc.", &draft.pubinfo)
            .on_input(Message::EditSourcePubinfo)
            .padding(6),
    ]
    .spacing(4);
    let abbrev_field = column![
        label("Abbreviation"),
        text_input("Short label", &draft.abbrev)
            .on_input(Message::EditSourceAbbrev)
            .padding(6),
    ]
    .spacing(4);

    let body = column![title, title_field, author_field, pubinfo_field, abbrev_field]
        .spacing(14)
        .padding(24)
        .align_x(Alignment::Start);
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
