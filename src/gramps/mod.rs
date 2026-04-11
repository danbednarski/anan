//! Gramps 6.x JSON object model.
//!
//! Every primary Gramps object is stored in SQLite as `json_data TEXT`.
//! Each JSON object carries a `_class` tag for self-description. These
//! modules define `serde`-deserializable Rust structs that mirror the
//! shapes observed in the Gramps 6.0.x codebase (`gen/lib/*.py`) and in
//! `test-fixtures/sample.db`.
//!
//! Design notes:
//!
//! - Optional and potentially-missing fields use `#[serde(default)]` so
//!   that forward-compat with minor schema churn is the default.
//! - Tagged enum fields in Gramps (`{_class, value, string}`) are modeled
//!   with the shared `Typed<T>` wrapper to keep struct definitions readable.
//! - We accept any extra unknown fields silently for now; if we start
//!   writing objects back we must round-trip them (Phase 2+).

pub mod citation;
pub mod common;
pub mod date;
pub mod enums;
pub mod event;
pub mod family;
pub mod media;
pub mod note;
pub mod person;
pub mod place;
pub mod repository;
pub mod source;
pub mod tag;

pub use citation::Citation;
pub use common::{Attribute, MediaRef, Typed, Url};
pub use date::Date;
pub use event::{Event, EventRef};
pub use family::{ChildRef, Family};
pub use media::Media;
pub use note::{Note, StyledText};
pub use person::{Name, Person, PersonRef, Surname};
pub use place::{Place, PlaceName, PlaceRef};
pub use repository::Repository;
pub use source::Source;
pub use tag::Tag;
