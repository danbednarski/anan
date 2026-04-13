//! Repository CRUD.
//!
//! Repositories carry a name, a `RepositoryType`, and optional
//! addresses / urls / note refs / tag refs. For Phase 4 we only edit
//! `name` and `type`; addresses, urls and note/tag lists are
//! preserved as-is on update.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{
    inbound_ref_count, new_handle, next_gramps_id, now_unix, rewrite_references, to_json,
};
use crate::gramps::common::Typed;
use crate::gramps::Repository;

pub fn create(txn: &Transaction, name: &str, type_value: i32) -> Result<Repository> {
    let repo = Repository {
        class: Some("Repository".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "repository", 'R')?,
        change: now_unix(),
        private: false,
        name: name.to_string(),
        r#type: Typed {
            class: Some("RepositoryType".to_string()),
            value: type_value,
            string: String::new(),
        },
        address_list: Vec::new(),
        urls: Vec::new(),
        note_list: Vec::new(),
        tag_list: Vec::new(),
    };

    let json = to_json(&repo)?;
    txn.execute(
        "INSERT INTO repository (handle, json_data, gramps_id, name, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            &repo.handle,
            &json,
            &repo.gramps_id,
            &repo.name,
            repo.change,
            repo.private as i32,
        ],
    )
    .context("insert repository row")?;

    rewrite_references(
        txn,
        &repo.handle,
        "Repository",
        &repo_outbound_refs(&repo),
    )?;
    tracing::info!(handle = %repo.handle, gramps_id = %repo.gramps_id, "created repository");
    Ok(repo)
}

pub fn update(
    txn: &Transaction,
    handle: &str,
    name: &str,
    type_value: i32,
) -> Result<Repository> {
    let existing_json: String = txn
        .query_row(
            "SELECT json_data FROM repository WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load repository {handle}"))?;
    let mut repo: Repository =
        serde_json::from_str(&existing_json).context("parse existing repository")?;

    repo.name = name.to_string();
    repo.r#type.value = type_value;
    repo.r#type.string = String::new();
    repo.change = now_unix();

    let json = to_json(&repo)?;
    let updated = txn
        .execute(
            "UPDATE repository SET json_data = ?2, name = ?3, change = ?4, private = ?5 \
             WHERE handle = ?1",
            params![
                handle,
                &json,
                &repo.name,
                repo.change,
                repo.private as i32,
            ],
        )
        .context("update repository row")?;
    if updated == 0 {
        bail!("no repository with handle {handle}");
    }

    rewrite_references(txn, &repo.handle, "Repository", &repo_outbound_refs(&repo))?;
    tracing::info!(handle = %repo.handle, "updated repository");
    Ok(repo)
}

pub fn delete(txn: &Transaction, handle: &str) -> Result<()> {
    let refs = inbound_ref_count(txn, handle)?;
    if refs > 0 {
        bail!(
            "cannot delete repository {handle}: still referenced by {refs} object(s)."
        );
    }
    let removed = txn
        .execute(
            "DELETE FROM repository WHERE handle = ?1",
            params![handle],
        )
        .context("delete repository row")?;
    if removed == 0 {
        bail!("no repository with handle {handle}");
    }
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete own reference rows")?;
    tracing::info!(handle, "deleted repository");
    Ok(())
}

fn repo_outbound_refs(repo: &Repository) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for note_h in &repo.note_list {
        out.push((note_h.clone(), "Note".to_string()));
    }
    for tag_h in &repo.tag_list {
        out.push((tag_h.clone(), "Tag".to_string()));
    }
    out
}
