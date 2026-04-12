//! Repository layer — CRUD operations organized by primary type.
//!
//! Every function here takes a `&Transaction` so that the caller (the
//! [`crate::db::Database::write_txn`] closure) controls atomicity:
//!
//! ```ignore
//! db.write_txn(|txn| {
//!     let tag = tag::create(txn, "Reviewed", "#1f77b4", 0)?;
//!     Ok(tag.handle)
//! })?;
//! ```
//!
//! Write helpers also refresh the `reference` denormalization table
//! and bump the `change` column to the current unix time.

pub mod common;
pub mod note;
pub mod repository;
pub mod tag;
