//! Flatten a Gramps [`StyledText`] to plain text.
//!
//! Gramps notes carry inline styling as a separate `tags` array of
//! `{name, value, ranges}` records (bold/italic/underline/link/...).
//! iced 0.13 doesn't have a first-class rich-text widget yet, so for
//! Phase 3 we render notes as plain text. When we add proper styling
//! in a later phase this module will grow a `render` function that
//! returns an `Element`; for now `render_plain` is the single entry
//! point.

use crate::gramps::note::StyledText;

/// Plain-text projection — ignores all styling tags.
pub fn render_plain(text: &StyledText) -> &str {
    text.string.as_str()
}
