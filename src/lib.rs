//! gramps-desktop — Rust native desktop UI for Gramps family trees.
//!
//! The crate is split into a library (this file, importable by
//! `examples/*.rs` and eventually integration tests) and a binary
//! (`src/main.rs`) that hosts the iced application.

pub mod app;
pub mod db;
pub mod gramps;
pub mod theme;
pub mod views;
