//! Note CRUD.
//!
//! A Note carries a [`StyledText`] body and a [`Typed`] note type. It
//! may reference Tags via its `tag_list`; those refs are written to
//! the denormalized `reference` table on every write.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{
    inbound_ref_count, new_handle, next_gramps_id, now_unix, rewrite_references, to_json,
};
use crate::gramps::common::Typed;
use crate::gramps::note::{Note, StyledText};

/// Create a new empty note with the given type value and initial body.
pub fn create(txn: &Transaction, type_value: i32, body: &str) -> Result<Note> {
    let note = Note {
        class: Some("Note".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "note", 'N')?,
        change: now_unix(),
        private: false,
        format: 0,
        text: StyledText {
            class: Some("StyledText".to_string()),
            string: body.to_string(),
            tags: Vec::new(),
        },
        r#type: Typed {
            class: Some("NoteType".to_string()),
            value: type_value,
            string: String::new(),
        },
        tag_list: Vec::new(),
    };

    let json = to_json(&note)?;
    txn.execute(
        "INSERT INTO note (handle, json_data, gramps_id, format, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            &note.handle,
            &json,
            &note.gramps_id,
            note.format,
            note.change,
            note.private as i32,
        ],
    )
    .context("insert note row")?;

    rewrite_references(txn, &note.handle, "Note", &note_outbound_refs(&note))?;

    tracing::info!(handle = %note.handle, gramps_id = %note.gramps_id, "created note");
    Ok(note)
}

/// Update an existing note's body and type. Preserves handle,
/// gramps_id, tag_list, private flag. Bumps `change`.
pub fn update(txn: &Transaction, handle: &str, type_value: i32, body: &str) -> Result<Note> {
    let existing_json: String = txn
        .query_row(
            "SELECT json_data FROM note WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load note {handle}"))?;
    let mut note: Note =
        serde_json::from_str(&existing_json).context("parse existing note")?;

    note.r#type.value = type_value;
    note.r#type.string = String::new();
    note.text.string = body.to_string();
    note.change = now_unix();

    let json = to_json(&note)?;
    let updated = txn
        .execute(
            "UPDATE note SET json_data = ?2, format = ?3, change = ?4, private = ?5 \
             WHERE handle = ?1",
            params![
                handle,
                &json,
                note.format,
                note.change,
                note.private as i32,
            ],
        )
        .context("update note row")?;
    if updated == 0 {
        bail!("no note with handle {handle}");
    }

    rewrite_references(txn, &note.handle, "Note", &note_outbound_refs(&note))?;

    tracing::info!(handle = %note.handle, "updated note");
    Ok(note)
}

/// Delete a note. Refuses if any object still references it.
pub fn delete(txn: &Transaction, handle: &str) -> Result<()> {
    let refs = inbound_ref_count(txn, handle)?;
    if refs > 0 {
        bail!(
            "cannot delete note {handle}: still referenced by {refs} object(s)."
        );
    }
    let removed = txn
        .execute("DELETE FROM note WHERE handle = ?1", params![handle])
        .context("delete note row")?;
    if removed == 0 {
        bail!("no note with handle {handle}");
    }
    // Its own outbound references go too.
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete own reference rows")?;
    tracing::info!(handle, "deleted note");
    Ok(())
}

/// The set of `(ref_handle, ref_class)` pairs a note contributes to
/// the `reference` denormalization table.
fn note_outbound_refs(note: &Note) -> Vec<(String, String)> {
    note.tag_list
        .iter()
        .map(|h| (h.clone(), "Tag".to_string()))
        .collect()
}
