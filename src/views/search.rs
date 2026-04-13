//! Global search across all primary types.
//!
//! Pass: user types a query, each primary type runs its own `matches`
//! predicate over its items, and hits from all types are interleaved in
//! one flat list. Clicking a hit jumps to the appropriate type's view
//! and selects the object.
//!
//! Keeping the results as a flat `Vec<SearchHit>` means the existing
//! `Message::SelectIndex` can still be reused: for the search view,
//! "index" means position in the hit vector.

use iced::Element;

use super::list_pane::{self, ListState};
use crate::app::Message;
use crate::db::Snapshot;

/// Which primary type a hit belongs to. Index is into the matching
/// `Snapshot` vector (persons, families, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Person,
    Family,
    Event,
    Place,
    Source,
    Citation,
    Media,
    Note,
    Repository,
    Tag,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchHit {
    pub kind: HitKind,
    pub index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub list: ListState,
    pub hits: Vec<SearchHit>,
}

pub fn recompute(snap: &Snapshot, state: &mut SearchState) {
    state.hits.clear();
    state.list.order.clear();
    let q = state.list.query.trim().to_lowercase();
    if q.is_empty() {
        state.list.selected = None;
        return;
    }

    for (i, p) in snap.persons.iter().enumerate() {
        if super::person::row_label(p).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Person, index: i });
        }
    }
    for (i, f) in snap.families.iter().enumerate() {
        if super::family::row_label(f, snap).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Family, index: i });
        }
    }
    for (i, e) in snap.events.iter().enumerate() {
        if super::event::row_label(e).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Event, index: i });
        }
    }
    for (i, p) in snap.places.iter().enumerate() {
        if super::place::row_label(p).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Place, index: i });
        }
    }
    for (i, s) in snap.sources.iter().enumerate() {
        if super::source::row_label(s).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Source, index: i });
        }
    }
    for (i, c) in snap.citations.iter().enumerate() {
        if super::citation::row_label(c, snap).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Citation, index: i });
        }
    }
    for (i, m) in snap.media.iter().enumerate() {
        if super::media::row_label(m).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Media, index: i });
        }
    }
    for (i, n) in snap.notes.iter().enumerate() {
        if super::note::row_label(n).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Note, index: i });
        }
    }
    for (i, r) in snap.repositories.iter().enumerate() {
        if super::repository::row_label(r).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Repository, index: i });
        }
    }
    for (i, t) in snap.tags.iter().enumerate() {
        if super::tag::row_label(t).to_lowercase().contains(&q) {
            state.hits.push(SearchHit { kind: HitKind::Tag, index: i });
        }
    }

    state.list.order = (0..state.hits.len()).collect();
    state.list.clamp_selection();
}

pub fn list_view<'a>(snap: &'a Snapshot, state: &'a SearchState) -> Element<'a, Message> {
    let rows: Vec<String> = state
        .hits
        .iter()
        .map(|hit| hit_label(snap, *hit))
        .collect();
    list_pane::view(
        "Search",
        rows,
        state.list.selected,
        &state.list.query,
        "Search all tables…",
    )
}

pub fn detail_view<'a>(snap: &'a Snapshot, hit: SearchHit) -> Element<'a, Message> {
    match hit.kind {
        HitKind::Person => {
            let Some(p) = snap.persons.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::person::detail_view(snap, p)
        }
        HitKind::Family => {
            let Some(f) = snap.families.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::family::detail_view(snap, f)
        }
        HitKind::Event => {
            let Some(e) = snap.events.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::event::detail_view(snap, e)
        }
        HitKind::Place => {
            let Some(p) = snap.places.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::place::detail_view(snap, p)
        }
        HitKind::Source => {
            let Some(s) = snap.sources.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::source::detail_view(snap, s)
        }
        HitKind::Citation => {
            let Some(c) = snap.citations.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::citation::detail_view(snap, c)
        }
        HitKind::Media => {
            let Some(m) = snap.media.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::media::detail_view(snap, m)
        }
        HitKind::Note => {
            let Some(n) = snap.notes.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::note::detail_view(snap, n)
        }
        HitKind::Repository => {
            let Some(r) = snap.repositories.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::repository::detail_view(snap, r)
        }
        HitKind::Tag => {
            let Some(t) = snap.tags.get(hit.index) else {
                return super::detail_ui::empty("(gone)");
            };
            super::tag::detail_view(snap, t)
        }
    }
}

fn hit_label(snap: &Snapshot, hit: SearchHit) -> String {
    match hit.kind {
        HitKind::Person => snap
            .persons
            .get(hit.index)
            .map(super::person::row_label)
            .map(|s| format!("👤  {s}"))
            .unwrap_or_default(),
        HitKind::Family => snap
            .families
            .get(hit.index)
            .map(|f| format!("👪  {}", super::family::row_label(f, snap)))
            .unwrap_or_default(),
        HitKind::Event => snap
            .events
            .get(hit.index)
            .map(|e| format!("★  {}", super::event::row_label(e)))
            .unwrap_or_default(),
        HitKind::Place => snap
            .places
            .get(hit.index)
            .map(|p| format!("📍  {}", super::place::row_label(p)))
            .unwrap_or_default(),
        HitKind::Source => snap
            .sources
            .get(hit.index)
            .map(|s| format!("📖  {}", super::source::row_label(s)))
            .unwrap_or_default(),
        HitKind::Citation => snap
            .citations
            .get(hit.index)
            .map(|c| format!("❝  {}", super::citation::row_label(c, snap)))
            .unwrap_or_default(),
        HitKind::Media => snap
            .media
            .get(hit.index)
            .map(|m| format!("🖼  {}", super::media::row_label(m)))
            .unwrap_or_default(),
        HitKind::Note => snap
            .notes
            .get(hit.index)
            .map(|n| format!("📝  {}", super::note::row_label(n)))
            .unwrap_or_default(),
        HitKind::Repository => snap
            .repositories
            .get(hit.index)
            .map(|r| format!("🏛  {}", super::repository::row_label(r)))
            .unwrap_or_default(),
        HitKind::Tag => snap
            .tags
            .get(hit.index)
            .map(|t| format!("🏷  {}", super::tag::row_label(t)))
            .unwrap_or_default(),
    }
}

