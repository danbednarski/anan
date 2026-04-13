//! Media CRUD.
//!
//! A Gramps `Media` record describes a file on disk that the tree
//! references — photographs, scanned documents, etc. Phase 6b
//! edits the reference fields (path, mime, description) and the
//! optional date; the file itself is not uploaded or copied
//! anywhere. Thumbnail rendering is deferred until a tree with
//! real media rows is available.
//!
//! Delete is refuse-on-use.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{
    inbound_ref_count, new_handle, next_gramps_id, now_unix, rewrite_references, to_json,
};
use crate::gramps::date::Date;
use crate::gramps::Media;

pub fn create(
    txn: &Transaction,
    path: &str,
    mime: &str,
    desc: &str,
    date: Option<Date>,
) -> Result<Media> {
    let media = Media {
        class: Some("Media".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "media", 'O')?,
        change: now_unix(),
        private: false,
        path: path.to_string(),
        mime: mime.to_string(),
        desc: desc.to_string(),
        checksum: String::new(),
        date,
        citation_list: Vec::new(),
        note_list: Vec::new(),
        attribute_list: Vec::new(),
        tag_list: Vec::new(),
    };

    insert(txn, &media)?;
    tracing::info!(handle = %media.handle, gramps_id = %media.gramps_id, "created media");
    Ok(media)
}

pub fn update(
    txn: &Transaction,
    handle: &str,
    path: &str,
    mime: &str,
    desc: &str,
    date: Option<Date>,
) -> Result<Media> {
    let existing: String = txn
        .query_row(
            "SELECT json_data FROM media WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load media {handle}"))?;
    let mut media: Media = serde_json::from_str(&existing).context("parse existing media")?;

    media.path = path.to_string();
    media.mime = mime.to_string();
    media.desc = desc.to_string();
    media.date = date;
    media.change = now_unix();

    update_row(txn, &media)?;
    tracing::info!(handle = %media.handle, "updated media");
    Ok(media)
}

pub fn delete(txn: &Transaction, handle: &str) -> Result<()> {
    let refs = inbound_ref_count(txn, handle)?;
    if refs > 0 {
        bail!(
            "cannot delete media {handle}: still referenced by {refs} object(s)."
        );
    }
    let removed = txn
        .execute("DELETE FROM media WHERE handle = ?1", params![handle])
        .context("delete media row")?;
    if removed == 0 {
        bail!("no media with handle {handle}");
    }
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete media outbound refs")?;
    tracing::info!(handle, "deleted media");
    Ok(())
}

fn insert(txn: &Transaction, media: &Media) -> Result<()> {
    let json = to_json(media)?;
    txn.execute(
        "INSERT INTO media (handle, json_data, gramps_id, path, mime, desc, checksum, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            &media.handle,
            &json,
            &media.gramps_id,
            &media.path,
            &media.mime,
            &media.desc,
            &media.checksum,
            media.change,
            media.private as i32,
        ],
    )
    .context("insert media row")?;
    rewrite_references(txn, &media.handle, "Media", &outbound_refs(media))?;
    Ok(())
}

fn update_row(txn: &Transaction, media: &Media) -> Result<()> {
    let json = to_json(media)?;
    let updated = txn
        .execute(
            "UPDATE media SET json_data = ?2, path = ?3, mime = ?4, desc = ?5, checksum = ?6, change = ?7, private = ?8 \
             WHERE handle = ?1",
            params![
                &media.handle,
                &json,
                &media.path,
                &media.mime,
                &media.desc,
                &media.checksum,
                media.change,
                media.private as i32,
            ],
        )
        .context("update media row")?;
    if updated == 0 {
        bail!("no media with handle {}", media.handle);
    }
    rewrite_references(txn, &media.handle, "Media", &outbound_refs(media))?;
    Ok(())
}

fn outbound_refs(media: &Media) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for h in &media.citation_list {
        out.push((h.clone(), "Citation".to_string()));
    }
    for h in &media.note_list {
        out.push((h.clone(), "Note".to_string()));
    }
    for h in &media.tag_list {
        out.push((h.clone(), "Tag".to_string()));
    }
    out
}
