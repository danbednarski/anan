//! Citation CRUD.
//!
//! A Citation is a specific reference into a `Source` — for example
//! "page 47" of the source's book, or a particular URL within a
//! website. For Phase 6b the form edits source_handle (via Gramps
//! ID in the UI), page / URL, confidence (0..4), and the
//! attached date.
//!
//! Delete is refuse-on-use: any Person/Family/Event/Place/Media/Note
//! pointing at this citation via its `citation_list` blocks the
//! delete. Callers should remove those references first.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{
    inbound_ref_count, new_handle, next_gramps_id, now_unix, rewrite_references, to_json,
};
use crate::gramps::date::Date;
use crate::gramps::Citation;

pub fn create(
    txn: &Transaction,
    source_handle: &str,
    page: &str,
    confidence: i32,
    date: Option<Date>,
) -> Result<Citation> {
    let citation = Citation {
        class: Some("Citation".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "citation", 'C')?,
        change: now_unix(),
        private: false,
        source_handle: source_handle.to_string(),
        page: page.to_string(),
        confidence,
        date,
        note_list: Vec::new(),
        media_list: Vec::new(),
        attribute_list: Vec::new(),
        tag_list: Vec::new(),
    };

    insert(txn, &citation)?;
    tracing::info!(
        handle = %citation.handle,
        gramps_id = %citation.gramps_id,
        "created citation"
    );
    Ok(citation)
}

pub fn update(
    txn: &Transaction,
    handle: &str,
    source_handle: &str,
    page: &str,
    confidence: i32,
    date: Option<Date>,
) -> Result<Citation> {
    let existing: String = txn
        .query_row(
            "SELECT json_data FROM citation WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load citation {handle}"))?;
    let mut citation: Citation =
        serde_json::from_str(&existing).context("parse existing citation")?;

    citation.source_handle = source_handle.to_string();
    citation.page = page.to_string();
    citation.confidence = confidence;
    citation.date = date;
    citation.change = now_unix();

    update_row(txn, &citation)?;
    tracing::info!(handle = %citation.handle, "updated citation");
    Ok(citation)
}

pub fn delete(txn: &Transaction, handle: &str) -> Result<()> {
    let refs = inbound_ref_count(txn, handle)?;
    if refs > 0 {
        bail!(
            "cannot delete citation {handle}: still referenced by {refs} object(s)."
        );
    }
    let removed = txn
        .execute("DELETE FROM citation WHERE handle = ?1", params![handle])
        .context("delete citation row")?;
    if removed == 0 {
        bail!("no citation with handle {handle}");
    }
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete citation outbound refs")?;
    tracing::info!(handle, "deleted citation");
    Ok(())
}

fn insert(txn: &Transaction, citation: &Citation) -> Result<()> {
    let json = to_json(citation)?;
    let source_handle = if citation.source_handle.is_empty() {
        None
    } else {
        Some(citation.source_handle.clone())
    };
    txn.execute(
        "INSERT INTO citation (handle, json_data, gramps_id, page, confidence, source_handle, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            &citation.handle,
            &json,
            &citation.gramps_id,
            &citation.page,
            citation.confidence,
            source_handle,
            citation.change,
            citation.private as i32,
        ],
    )
    .context("insert citation row")?;
    rewrite_references(txn, &citation.handle, "Citation", &outbound_refs(citation))?;
    Ok(())
}

fn update_row(txn: &Transaction, citation: &Citation) -> Result<()> {
    let json = to_json(citation)?;
    let source_handle = if citation.source_handle.is_empty() {
        None
    } else {
        Some(citation.source_handle.clone())
    };
    let updated = txn
        .execute(
            "UPDATE citation SET json_data = ?2, page = ?3, confidence = ?4, source_handle = ?5, change = ?6, private = ?7 \
             WHERE handle = ?1",
            params![
                &citation.handle,
                &json,
                &citation.page,
                citation.confidence,
                source_handle,
                citation.change,
                citation.private as i32,
            ],
        )
        .context("update citation row")?;
    if updated == 0 {
        bail!("no citation with handle {}", citation.handle);
    }
    rewrite_references(txn, &citation.handle, "Citation", &outbound_refs(citation))?;
    Ok(())
}

fn outbound_refs(citation: &Citation) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !citation.source_handle.is_empty() {
        out.push((citation.source_handle.clone(), "Source".to_string()));
    }
    for h in &citation.note_list {
        out.push((h.clone(), "Note".to_string()));
    }
    for m in &citation.media_list {
        out.push((m.r#ref.clone(), "Media".to_string()));
    }
    for h in &citation.tag_list {
        out.push((h.clone(), "Tag".to_string()));
    }
    out
}
