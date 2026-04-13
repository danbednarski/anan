//! Tag CRUD.
//!
//! Tags are the simplest primary type: no `gramps_id`, no outbound
//! references, just `name / color / priority`. Deletion is allowed
//! only when no other object carries the tag in its `tag_list`
//! (inbound reference count must be zero).

use anyhow::{bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{inbound_ref_count, new_handle, now_unix, to_json};
use crate::gramps::Tag;

/// Insert a new Tag and return the created record (with the freshly
/// allocated handle).
pub fn create(txn: &Transaction, name: &str, color: &str, priority: i32) -> Result<Tag> {
    let tag = Tag {
        class: Some("Tag".to_string()),
        handle: new_handle(),
        change: now_unix(),
        name: name.to_string(),
        color: color.to_string(),
        priority,
    };

    let json = to_json(&tag)?;
    txn.execute(
        "INSERT INTO tag (handle, json_data, name, color, priority, change) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            &tag.handle,
            &json,
            &tag.name,
            &tag.color,
            tag.priority,
            tag.change,
        ],
    )
    .context("insert tag row")?;

    // Tags have no outbound refs — nothing to write to `reference`.
    tracing::info!(handle = %tag.handle, name = %tag.name, "created tag");
    Ok(tag)
}

/// Replace the mutable fields of an existing Tag. Fails if the handle
/// doesn't exist.
pub fn update(
    txn: &Transaction,
    handle: &str,
    name: &str,
    color: &str,
    priority: i32,
) -> Result<Tag> {
    let tag = Tag {
        class: Some("Tag".to_string()),
        handle: handle.to_string(),
        change: now_unix(),
        name: name.to_string(),
        color: color.to_string(),
        priority,
    };
    let json = to_json(&tag)?;
    let updated = txn
        .execute(
            "UPDATE tag SET json_data = ?2, name = ?3, color = ?4, priority = ?5, change = ?6 \
             WHERE handle = ?1",
            params![
                &tag.handle,
                &json,
                &tag.name,
                &tag.color,
                tag.priority,
                tag.change,
            ],
        )
        .context("update tag row")?;
    if updated == 0 {
        bail!("no tag with handle {handle}");
    }
    tracing::info!(handle = %tag.handle, "updated tag");
    Ok(tag)
}

/// Delete a tag. Refuses if any primary object still references the
/// tag in its `tag_list` (via the `reference` table).
pub fn delete(txn: &Transaction, handle: &str) -> Result<()> {
    let refs = inbound_ref_count(txn, handle)?;
    if refs > 0 {
        bail!(
            "cannot delete tag {handle}: still referenced by {refs} object(s). \
             Remove the tag from those objects first."
        );
    }
    let removed = txn
        .execute("DELETE FROM tag WHERE handle = ?1", params![handle])
        .context("delete tag row")?;
    if removed == 0 {
        bail!("no tag with handle {handle}");
    }
    tracing::info!(handle, "deleted tag");
    Ok(())
}
