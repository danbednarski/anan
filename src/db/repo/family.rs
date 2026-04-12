//! Family write helpers.
//!
//! Phase 5 only needs a `save` entry point so the Person delete
//! cascade can rewrite a family's row (father/mother cleared,
//! child_ref_list pruned) and refresh the `reference` table. Phase
//! 6a will grow this with create/update/delete driven by its own UI.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{now_unix, rewrite_references, to_json};
use crate::gramps::Family;

/// Overwrite the row for an existing Family. Bumps `change`, rewrites
/// the secondary columns, and refreshes the `reference` table.
pub fn save(txn: &Transaction, family: &mut Family) -> Result<()> {
    family.change = now_unix();
    let json = to_json(family)?;
    let father = family
        .father_handle
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned();
    let mother = family
        .mother_handle
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned();
    let updated = txn
        .execute(
            "UPDATE family SET json_data = ?2, father_handle = ?3, mother_handle = ?4, change = ?5, private = ?6 \
             WHERE handle = ?1",
            params![
                &family.handle,
                &json,
                father,
                mother,
                family.change,
                family.private as i32,
            ],
        )
        .context("update family row")?;
    if updated == 0 {
        bail!("no family with handle {}", family.handle);
    }
    rewrite_references(txn, &family.handle, "Family", &outbound_refs(family))?;
    Ok(())
}

/// Every outbound reference a Family contributes to the `reference`
/// table. Shared with Phase 6a.
pub fn outbound_refs(family: &Family) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(h) = family.father_handle.as_ref().filter(|s| !s.is_empty()) {
        out.push((h.clone(), "Person".to_string()));
    }
    if let Some(h) = family.mother_handle.as_ref().filter(|s| !s.is_empty()) {
        out.push((h.clone(), "Person".to_string()));
    }
    for cr in &family.child_ref_list {
        out.push((cr.r#ref.clone(), "Person".to_string()));
    }
    for er in &family.event_ref_list {
        out.push((er.r#ref.clone(), "Event".to_string()));
    }
    for m in &family.media_list {
        out.push((m.r#ref.clone(), "Media".to_string()));
    }
    for h in &family.citation_list {
        out.push((h.clone(), "Citation".to_string()));
    }
    for h in &family.note_list {
        out.push((h.clone(), "Note".to_string()));
    }
    for h in &family.tag_list {
        out.push((h.clone(), "Tag".to_string()));
    }
    out
}
