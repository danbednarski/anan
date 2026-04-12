//! Source CRUD.
//!
//! Sources in Gramps represent "the thing being cited" — a book,
//! website, document collection, etc. Every `Citation` points at a
//! `Source` via `source_handle`. For Phase 6b the form edits title,
//! author, pubinfo, abbrev; reporef_list / media_list / note_list /
//! attribute_list are preserved on update.
//!
//! Delete is refuse-on-use: any citation pointing at the source
//! counts as an inbound reference and blocks the delete. The user
//! must remove or repoint those citations first.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{
    inbound_ref_count, new_handle, next_gramps_id, now_unix, rewrite_references, to_json,
};
use crate::gramps::Source;

pub fn create(
    txn: &Transaction,
    title: &str,
    author: &str,
    pubinfo: &str,
    abbrev: &str,
) -> Result<Source> {
    let source = Source {
        class: Some("Source".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "source", 'S')?,
        change: now_unix(),
        private: false,
        title: title.to_string(),
        author: author.to_string(),
        pubinfo: pubinfo.to_string(),
        abbrev: abbrev.to_string(),
        reporef_list: Vec::new(),
        media_list: Vec::new(),
        attribute_list: Vec::new(),
        note_list: Vec::new(),
        tag_list: Vec::new(),
    };

    insert(txn, &source)?;
    tracing::info!(handle = %source.handle, gramps_id = %source.gramps_id, "created source");
    Ok(source)
}

pub fn update(
    txn: &Transaction,
    handle: &str,
    title: &str,
    author: &str,
    pubinfo: &str,
    abbrev: &str,
) -> Result<Source> {
    let existing: String = txn
        .query_row(
            "SELECT json_data FROM source WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load source {handle}"))?;
    let mut source: Source =
        serde_json::from_str(&existing).context("parse existing source")?;

    source.title = title.to_string();
    source.author = author.to_string();
    source.pubinfo = pubinfo.to_string();
    source.abbrev = abbrev.to_string();
    source.change = now_unix();

    update_row(txn, &source)?;
    tracing::info!(handle = %source.handle, "updated source");
    Ok(source)
}

pub fn delete(txn: &Transaction, handle: &str) -> Result<()> {
    let refs = inbound_ref_count(txn, handle)?;
    if refs > 0 {
        bail!(
            "cannot delete source {handle}: still referenced by {refs} object(s). \
             Remove or repoint citations first."
        );
    }
    let removed = txn
        .execute("DELETE FROM source WHERE handle = ?1", params![handle])
        .context("delete source row")?;
    if removed == 0 {
        bail!("no source with handle {handle}");
    }
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete source outbound refs")?;
    tracing::info!(handle, "deleted source");
    Ok(())
}

fn insert(txn: &Transaction, source: &Source) -> Result<()> {
    let json = to_json(source)?;
    txn.execute(
        "INSERT INTO source (handle, json_data, gramps_id, title, author, pubinfo, abbrev, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            &source.handle,
            &json,
            &source.gramps_id,
            &source.title,
            &source.author,
            &source.pubinfo,
            &source.abbrev,
            source.change,
            source.private as i32,
        ],
    )
    .context("insert source row")?;
    rewrite_references(txn, &source.handle, "Source", &outbound_refs(source))?;
    Ok(())
}

fn update_row(txn: &Transaction, source: &Source) -> Result<()> {
    let json = to_json(source)?;
    let updated = txn
        .execute(
            "UPDATE source SET json_data = ?2, title = ?3, author = ?4, pubinfo = ?5, abbrev = ?6, change = ?7, private = ?8 \
             WHERE handle = ?1",
            params![
                &source.handle,
                &json,
                &source.title,
                &source.author,
                &source.pubinfo,
                &source.abbrev,
                source.change,
                source.private as i32,
            ],
        )
        .context("update source row")?;
    if updated == 0 {
        bail!("no source with handle {}", source.handle);
    }
    rewrite_references(txn, &source.handle, "Source", &outbound_refs(source))?;
    Ok(())
}

fn outbound_refs(source: &Source) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for rr in &source.reporef_list {
        out.push((rr.r#ref.clone(), "Repository".to_string()));
    }
    for m in &source.media_list {
        out.push((m.r#ref.clone(), "Media".to_string()));
    }
    for h in &source.note_list {
        out.push((h.clone(), "Note".to_string()));
    }
    for h in &source.tag_list {
        out.push((h.clone(), "Tag".to_string()));
    }
    out
}
