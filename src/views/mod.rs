//! Page-level view builders. Each primary-type module owns a list view
//! and a detail view, plus a `recompute` function that rebuilds its
//! `ListState.order` from the current query and sort key. Views are
//! stateless over iced — all state lives in `app::App`.
//!
//! Shared building blocks:
//!
//! - [`list_pane`] — generic list pane used by every view
//! - [`detail_ui`] — layout primitives (chip/field/section) for detail panes
//! - [`widgets`] — rendering helpers for Date, StyledText, etc.

pub mod detail_ui;
pub mod list_pane;
pub mod widgets;

pub mod citation;
pub mod event;
pub mod family;
pub mod media;
pub mod note;
pub mod person;
pub mod place;
pub mod repository;
pub mod search;
pub mod canvas_tree;
pub mod network;
pub mod source;
pub mod tag;
pub mod tree;
