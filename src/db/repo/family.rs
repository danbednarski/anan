//! Family CRUD.
//!
//! The Family is a cross-ref hub: it points at father/mother persons,
//! child persons, events, media, and tags, and is pointed at by every
//! person in those roles. Create and update rewrite the row and the
//! `reference` table. Delete is cascade-aware — it walks every
//! referenced person and removes the family from their `family_list`
//! or `parent_family_list` before deleting the family row.

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, Transaction};

use super::common::{new_handle, next_gramps_id, now_unix, rewrite_references, to_json};
use crate::gramps::common::Typed;
use crate::gramps::Family;

/// Create a minimal Family — father + mother + type. Children are
/// added later by re-linking from the Person side (which updates
/// the family's child_ref_list via the Person update path), or by
/// extending this module with a child-add helper (TODO in 6a UI).
pub fn create(
    txn: &Transaction,
    father_handle: Option<String>,
    mother_handle: Option<String>,
    type_value: i32,
) -> Result<Family> {
    let mut family = Family {
        class: Some("Family".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "family", 'F')?,
        change: now_unix(),
        private: false,
        father_handle: father_handle.filter(|s| !s.is_empty()),
        mother_handle: mother_handle.filter(|s| !s.is_empty()),
        child_ref_list: Vec::new(),
        r#type: Typed {
            class: Some("FamilyRelType".to_string()),
            value: type_value,
            string: String::new(),
        },
        event_ref_list: Vec::new(),
        media_list: Vec::new(),
        attribute_list: Vec::new(),
        lds_ord_list: Vec::new(),
        citation_list: Vec::new(),
        note_list: Vec::new(),
        tag_list: Vec::new(),
        complete: 0,
    };

    insert(txn, &family)?;
    // Also add the new family to each parent's family_list so the
    // reverse link is consistent.
    if let Some(h) = family.father_handle.clone() {
        append_parent_family(txn, &h, &family.handle)?;
    }
    if let Some(h) = family.mother_handle.clone() {
        append_parent_family(txn, &h, &family.handle)?;
    }

    // family.change was already set, but insert may have bumped in
    // helpers; re-read for the return value would be pedantic.
    family.change = now_unix();
    tracing::info!(handle = %family.handle, gramps_id = %family.gramps_id, "created family");
    Ok(family)
}

/// Update the editable fields (father_handle / mother_handle / type).
/// Child list, event list, and other cross-refs are preserved.
///
/// When a parent handle changes, we also update the old and new
/// parents' `family_list` so the reverse link stays consistent.
pub fn update(
    txn: &Transaction,
    handle: &str,
    father_handle: Option<String>,
    mother_handle: Option<String>,
    type_value: i32,
) -> Result<Family> {
    let existing: String = txn
        .query_row(
            "SELECT json_data FROM family WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load family {handle}"))?;
    let mut family: Family =
        serde_json::from_str(&existing).context("parse existing family")?;

    let old_father = family.father_handle.clone();
    let old_mother = family.mother_handle.clone();

    family.father_handle = father_handle.filter(|s| !s.is_empty());
    family.mother_handle = mother_handle.filter(|s| !s.is_empty());
    family.r#type.value = type_value;
    family.r#type.string = String::new();

    save(txn, &mut family)?;

    // Reverse links on parents.
    if old_father != family.father_handle {
        if let Some(h) = &old_father {
            remove_parent_family(txn, h, handle)?;
        }
        if let Some(h) = &family.father_handle {
            append_parent_family(txn, h, handle)?;
        }
    }
    if old_mother != family.mother_handle {
        if let Some(h) = &old_mother {
            remove_parent_family(txn, h, handle)?;
        }
        if let Some(h) = &family.mother_handle {
            append_parent_family(txn, h, handle)?;
        }
    }

    Ok(family)
}

/// Delete a Family with cascade — rewrites every linked person to
/// remove the family from their `family_list` / `parent_family_list`,
/// then deletes the family row and its reference entries.
pub fn delete_with_cascade(txn: &Transaction, handle: &str) -> Result<()> {
    let existing: String = txn
        .query_row(
            "SELECT json_data FROM family WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load family {handle}"))?;
    let family: Family =
        serde_json::from_str(&existing).context("parse family")?;

    if let Some(h) = &family.father_handle {
        remove_parent_family(txn, h, handle)?;
    }
    if let Some(h) = &family.mother_handle {
        remove_parent_family(txn, h, handle)?;
    }
    for cr in &family.child_ref_list {
        remove_child_family(txn, &cr.r#ref, handle)?;
    }

    let removed = txn
        .execute("DELETE FROM family WHERE handle = ?1", params![handle])
        .context("delete family row")?;
    if removed == 0 {
        bail!("no family with handle {handle}");
    }
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete family outbound refs")?;
    tracing::info!(handle, "deleted family with cascade");
    Ok(())
}

/// Overwrite the row for an existing Family. Also used by the Person
/// delete cascade (Phase 5). Bumps `change`, rewrites the secondary
/// columns, and refreshes the `reference` table.
pub fn save(conn: &Connection, family: &mut Family) -> Result<()> {
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
    let updated = conn
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
    rewrite_references(conn, &family.handle, "Family", &outbound_refs(family))?;
    Ok(())
}

fn insert(txn: &Transaction, family: &Family) -> Result<()> {
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
    txn.execute(
        "INSERT INTO family (handle, json_data, gramps_id, father_handle, mother_handle, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            &family.handle,
            &json,
            &family.gramps_id,
            father,
            mother,
            family.change,
            family.private as i32,
        ],
    )
    .context("insert family row")?;
    rewrite_references(txn, &family.handle, "Family", &outbound_refs(family))?;
    Ok(())
}

/// Add `family_handle` to a person's `family_list` if it isn't
/// already there. Rewrites the person row + reference rows.
fn append_parent_family(txn: &Transaction, person_handle: &str, family_handle: &str) -> Result<()> {
    let json: String = match txn.query_row(
        "SELECT json_data FROM person WHERE handle = ?1",
        params![person_handle],
        |r| r.get(0),
    ) {
        Ok(s) => s,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(e) => return Err(anyhow!("load person {person_handle}: {e}")),
    };
    let mut person: crate::gramps::Person =
        serde_json::from_str(&json).context("parse person")?;
    if !person.family_list.iter().any(|h| h == family_handle) {
        person.family_list.push(family_handle.to_string());
        super::person::save_row(txn, &mut person)?;
    }
    Ok(())
}

/// Drop `family_handle` from a person's `family_list`. Rewrites row.
fn remove_parent_family(txn: &Transaction, person_handle: &str, family_handle: &str) -> Result<()> {
    let json: String = match txn.query_row(
        "SELECT json_data FROM person WHERE handle = ?1",
        params![person_handle],
        |r| r.get(0),
    ) {
        Ok(s) => s,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(e) => return Err(anyhow!("load person {person_handle}: {e}")),
    };
    let mut person: crate::gramps::Person =
        serde_json::from_str(&json).context("parse person")?;
    let before = person.family_list.len();
    person.family_list.retain(|h| h != family_handle);
    if person.family_list.len() != before {
        super::person::save_row(txn, &mut person)?;
    }
    Ok(())
}

/// Drop `family_handle` from a person's `parent_family_list`. Rewrites row.
fn remove_child_family(txn: &Transaction, person_handle: &str, family_handle: &str) -> Result<()> {
    let json: String = match txn.query_row(
        "SELECT json_data FROM person WHERE handle = ?1",
        params![person_handle],
        |r| r.get(0),
    ) {
        Ok(s) => s,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(e) => return Err(anyhow!("load person {person_handle}: {e}")),
    };
    let mut person: crate::gramps::Person =
        serde_json::from_str(&json).context("parse person")?;
    let before = person.parent_family_list.len();
    person
        .parent_family_list
        .retain(|h| h != family_handle);
    if person.parent_family_list.len() != before {
        super::person::save_row(txn, &mut person)?;
    }
    Ok(())
}

/// Every outbound reference a Family contributes to the `reference`
/// table. Used by `save` and by Phase 5's Person delete cascade.
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
