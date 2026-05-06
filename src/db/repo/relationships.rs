//! High-level "add person with relationship" helpers.
//!
//! These compose the lower-level Person and Family CRUD modules into
//! single-call operations for the tree view's context actions:
//!
//! - **Add child** to a person: creates a new Person, finds or creates
//!   the right Family, and wires the child into it.
//! - **Add parent** to a person: creates a new Person and a Family
//!   linking the new person as father/mother and the target as child.
//! - **Add sibling** to a person: creates a new Person and adds them
//!   to the target's parent_family_list[0] (the first parent family).

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Transaction};

use super::{family as family_repo, person as person_repo};
use crate::gramps::common::Typed;
use crate::gramps::family::{ChildRef, Family};
use crate::gramps::Person;

/// Create a new person and add them as a child of `parent_handle`.
///
/// If the parent already has a family (via `family_list`), the child
/// is appended to that family's `child_ref_list`. If not, a new
/// family is created with the parent as father (if male/unknown) or
/// mother (if female), and the child is added.
///
/// Returns the newly created person.
pub fn add_child(
    txn: &Transaction,
    parent_handle: &str,
    first_name: &str,
    surname: &str,
    gender: i32,
) -> Result<Person> {
    let child = person_repo::create(txn, first_name, surname, gender, None, None)?;

    let parent_json: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![parent_handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load parent {parent_handle}"))?;
    let parent: Person =
        serde_json::from_str(&parent_json).context("parse parent")?;

    let family_handle = if let Some(fh) = parent.family_list.first().cloned() {
        // Append child to existing family.
        fh
    } else {
        // Create a new family with the parent in the appropriate role.
        let (father, mother) = if parent.gender == 0 {
            (None, Some(parent_handle.to_string()))
        } else {
            (Some(parent_handle.to_string()), None)
        };
        let fam = family_repo::create(txn, father, mother, 0)?;
        fam.handle
    };

    // Append child to the family's child_ref_list.
    append_child_to_family(txn, &family_handle, &child.handle)?;

    // Add the family to the child's parent_family_list.
    let mut child_reloaded: Person = {
        let j: String = txn
            .query_row(
                "SELECT json_data FROM person WHERE handle = ?1",
                params![&child.handle],
                |r| r.get(0),
            )
            .context("reload child")?;
        serde_json::from_str(&j).context("parse child")?
    };
    if !child_reloaded
        .parent_family_list
        .iter()
        .any(|h| h == &family_handle)
    {
        child_reloaded
            .parent_family_list
            .push(family_handle.clone());
        person_repo::save_row(txn, &mut child_reloaded)?;
    }

    tracing::info!(
        parent = parent_handle,
        child = %child.handle,
        family = %family_handle,
        "added child to person"
    );
    Ok(child)
}

/// Create a new person and wire them as a parent (father or mother)
/// of `person_handle`.
///
/// Creates a new Family with the new person as father (if gender
/// male/unknown) or mother (if female), and adds `person_handle` as
/// a child of that family. If `person_handle` already has a parent
/// family without the requested role filled, we fill that slot
/// instead of creating a duplicate family.
///
/// Returns the newly created parent person.
pub fn add_parent(
    txn: &Transaction,
    person_handle: &str,
    first_name: &str,
    surname: &str,
    gender: i32,
) -> Result<Person> {
    let new_parent = person_repo::create(txn, first_name, surname, gender, None, None)?;

    let person_json: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![person_handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load person {person_handle}"))?;
    let mut person: Person =
        serde_json::from_str(&person_json).context("parse person")?;

    // Check if the person already has a parent family with an open slot.
    let existing_family = person.parent_family_list.first().and_then(|fh| {
        let j: String = txn
            .query_row(
                "SELECT json_data FROM family WHERE handle = ?1",
                params![fh],
                |r| r.get(0),
            )
            .ok()?;
        let fam: Family = serde_json::from_str(&j).ok()?;
        // Check if the appropriate slot is open for this gender.
        let slot_open = if gender == 0 {
            fam.mother_handle.is_none()
        } else {
            fam.father_handle.is_none()
        };
        if slot_open { Some(fam) } else { None }
    });

    if let Some(mut fam) = existing_family {
        // Fill the open slot.
        if gender == 0 {
            fam.mother_handle = Some(new_parent.handle.clone());
        } else {
            fam.father_handle = Some(new_parent.handle.clone());
        }
        family_repo::save(txn, &mut fam)?;

        // Add the family to the new parent's family_list.
        let mut np: Person = {
            let j: String = txn
                .query_row(
                    "SELECT json_data FROM person WHERE handle = ?1",
                    params![&new_parent.handle],
                    |r| r.get(0),
                )
                .context("reload new parent")?;
            serde_json::from_str(&j).context("parse new parent")?
        };
        if !np.family_list.iter().any(|h| h == &fam.handle) {
            np.family_list.push(fam.handle.clone());
            person_repo::save_row(txn, &mut np)?;
        }
    } else {
        // Create a new family.
        let (father, mother) = if gender == 0 {
            (None, Some(new_parent.handle.clone()))
        } else {
            (Some(new_parent.handle.clone()), None)
        };
        let fam = family_repo::create(txn, father, mother, 0)?;

        // Add person as child of this family.
        append_child_to_family(txn, &fam.handle, person_handle)?;

        // Add family to person's parent_family_list.
        if !person.parent_family_list.iter().any(|h| h == &fam.handle) {
            person.parent_family_list.push(fam.handle.clone());
            person_repo::save_row(txn, &mut person)?;
        }
    }

    tracing::info!(
        person = person_handle,
        new_parent = %new_parent.handle,
        "added parent to person"
    );
    Ok(new_parent)
}

/// Create a new person and add them as a sibling of `person_handle`.
///
/// The new person is added to the first family in
/// `person_handle`'s `parent_family_list`. If the person has no
/// parent family, returns an error — the user should add parents
/// first.
pub fn add_sibling(
    txn: &Transaction,
    person_handle: &str,
    first_name: &str,
    surname: &str,
    gender: i32,
) -> Result<Person> {
    let person_json: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![person_handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load person {person_handle}"))?;
    let person: Person =
        serde_json::from_str(&person_json).context("parse person")?;

    let family_handle = person
        .parent_family_list
        .first()
        .ok_or_else(|| anyhow!("no parent family to add a sibling to — add parents first"))?
        .clone();

    let sibling = person_repo::create(txn, first_name, surname, gender, None, None)?;

    // Append sibling to family's child_ref_list.
    append_child_to_family(txn, &family_handle, &sibling.handle)?;

    // Add family to sibling's parent_family_list.
    let mut sib_reloaded: Person = {
        let j: String = txn
            .query_row(
                "SELECT json_data FROM person WHERE handle = ?1",
                params![&sibling.handle],
                |r| r.get(0),
            )
            .context("reload sibling")?;
        serde_json::from_str(&j).context("parse sibling")?
    };
    if !sib_reloaded
        .parent_family_list
        .iter()
        .any(|h| h == &family_handle)
    {
        sib_reloaded
            .parent_family_list
            .push(family_handle.clone());
        person_repo::save_row(txn, &mut sib_reloaded)?;
    }

    tracing::info!(
        person = person_handle,
        sibling = %sibling.handle,
        family = %family_handle,
        "added sibling to person"
    );
    Ok(sibling)
}

/// Append a child ref to a family's `child_ref_list` and rewrite the
/// family row.
fn append_child_to_family(txn: &Transaction, family_handle: &str, child_handle: &str) -> Result<()> {
    let fam_json: String = txn
        .query_row(
            "SELECT json_data FROM family WHERE handle = ?1",
            params![family_handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load family {family_handle}"))?;
    let mut family: Family =
        serde_json::from_str(&fam_json).context("parse family")?;

    // Don't add duplicates.
    if family
        .child_ref_list
        .iter()
        .any(|cr| cr.r#ref == child_handle)
    {
        return Ok(());
    }

    family.child_ref_list.push(ChildRef {
        class: Some("ChildRef".to_string()),
        r#ref: child_handle.to_string(),
        private: false,
        citation_list: Vec::new(),
        note_list: Vec::new(),
        frel: Typed {
            class: Some("ChildRefType".to_string()),
            value: 1, // Birth
            string: String::new(),
        },
        mrel: Typed {
            class: Some("ChildRefType".to_string()),
            value: 1, // Birth
            string: String::new(),
        },
    });

    family_repo::save(txn, &mut family)?;
    Ok(())
}

// ---- "_existing" variants: wire an already-created person ----
//
// Used by the modal flow where the person is created with full
// name/dates first, then wired into the relationship separately.

/// Wire an existing child person into a parent's family. Same logic
/// as `add_child` but skips the person creation step.
pub fn add_child_existing(
    txn: &Transaction,
    parent_handle: &str,
    child_handle: &str,
) -> Result<()> {
    let parent_json: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![parent_handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load parent {parent_handle}"))?;
    let parent: Person =
        serde_json::from_str(&parent_json).context("parse parent")?;

    let family_handle = if let Some(fh) = parent.family_list.first().cloned() {
        fh
    } else {
        let (father, mother) = if parent.gender == 0 {
            (None, Some(parent_handle.to_string()))
        } else {
            (Some(parent_handle.to_string()), None)
        };
        let fam = family_repo::create(txn, father, mother, 0)?;
        fam.handle
    };

    append_child_to_family(txn, &family_handle, child_handle)?;

    let mut child: Person = {
        let j: String = txn
            .query_row(
                "SELECT json_data FROM person WHERE handle = ?1",
                params![child_handle],
                |r| r.get(0),
            )
            .context("reload child")?;
        serde_json::from_str(&j).context("parse child")?
    };
    if !child.parent_family_list.iter().any(|h| h == &family_handle) {
        child.parent_family_list.push(family_handle);
        person_repo::save_row(txn, &mut child)?;
    }
    Ok(())
}

/// Wire an existing child into a family, optionally specifying the
/// other parent. If `other_parent_handle` is provided, look for an
/// existing family where both parents match; if none exists, create one.
pub fn add_child_with_parents(
    txn: &Transaction,
    parent_handle: &str,
    child_handle: &str,
    other_parent_handle: Option<&str>,
) -> Result<()> {
    let parent_json: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![parent_handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load parent {parent_handle}"))?;
    let parent: Person =
        serde_json::from_str(&parent_json).context("parse parent")?;

    let family_handle = if let Some(other_h) = other_parent_handle {
        // Find an existing family with both parents.
        let mut found: Option<String> = None;
        for fh in &parent.family_list {
            let j: String = txn
                .query_row(
                    "SELECT json_data FROM family WHERE handle = ?1",
                    params![fh],
                    |r| r.get(0),
                )
                .with_context(|| format!("load family {fh}"))?;
            let fam: Family = serde_json::from_str(&j).context("parse family")?;
            let has_other = fam.father_handle.as_deref() == Some(other_h)
                || fam.mother_handle.as_deref() == Some(other_h);
            if has_other {
                found = Some(fh.clone());
                break;
            }
        }
        if let Some(fh) = found {
            fh
        } else {
            // Create a new family with both parents.
            let other_json: String = txn
                .query_row(
                    "SELECT json_data FROM person WHERE handle = ?1",
                    params![other_h],
                    |r| r.get(0),
                )
                .with_context(|| format!("load other parent {other_h}"))?;
            let other: Person = serde_json::from_str(&other_json)?;
            let (father, mother) = match (parent.gender, other.gender) {
                (0, _) => (Some(other_h.to_string()), Some(parent_handle.to_string())),
                (_, 0) => (Some(parent_handle.to_string()), Some(other_h.to_string())),
                (1, _) => (Some(parent_handle.to_string()), Some(other_h.to_string())),
                _ => (Some(parent_handle.to_string()), Some(other_h.to_string())),
            };
            let fam = family_repo::create(txn, father, mother, 0)?;
            fam.handle
        }
    } else {
        // No other parent specified - use existing logic.
        if let Some(fh) = parent.family_list.first().cloned() {
            fh
        } else {
            let (father, mother) = if parent.gender == 0 {
                (None, Some(parent_handle.to_string()))
            } else {
                (Some(parent_handle.to_string()), None)
            };
            let fam = family_repo::create(txn, father, mother, 0)?;
            fam.handle
        }
    };

    append_child_to_family(txn, &family_handle, child_handle)?;

    let mut child: Person = {
        let j: String = txn
            .query_row(
                "SELECT json_data FROM person WHERE handle = ?1",
                params![child_handle],
                |r| r.get(0),
            )
            .context("reload child")?;
        serde_json::from_str(&j).context("parse child")?
    };
    if !child.parent_family_list.iter().any(|h| h == &family_handle) {
        child.parent_family_list.push(family_handle);
        person_repo::save_row(txn, &mut child)?;
    }
    Ok(())
}

/// Wire an existing person as parent of `person_handle`.
pub fn add_parent_existing(
    txn: &Transaction,
    person_handle: &str,
    new_parent_handle: &str,
    gender: i32,
) -> Result<()> {
    let person_json: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![person_handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load person {person_handle}"))?;
    let mut person: Person =
        serde_json::from_str(&person_json).context("parse person")?;

    // Check existing parent family for open slot.
    let existing_family = person.parent_family_list.first().and_then(|fh| {
        let j: String = txn
            .query_row(
                "SELECT json_data FROM family WHERE handle = ?1",
                params![fh],
                |r| r.get(0),
            )
            .ok()?;
        let fam: Family = serde_json::from_str(&j).ok()?;
        let slot_open = if gender == 0 {
            fam.mother_handle.is_none()
        } else {
            fam.father_handle.is_none()
        };
        if slot_open { Some(fam) } else { None }
    });

    if let Some(mut fam) = existing_family {
        if gender == 0 {
            fam.mother_handle = Some(new_parent_handle.to_string());
        } else {
            fam.father_handle = Some(new_parent_handle.to_string());
        }
        family_repo::save(txn, &mut fam)?;

        let mut np: Person = {
            let j: String = txn
                .query_row(
                    "SELECT json_data FROM person WHERE handle = ?1",
                    params![new_parent_handle],
                    |r| r.get(0),
                )
                .context("reload new parent")?;
            serde_json::from_str(&j).context("parse new parent")?
        };
        if !np.family_list.iter().any(|h| h == &fam.handle) {
            np.family_list.push(fam.handle);
            person_repo::save_row(txn, &mut np)?;
        }
    } else {
        let (father, mother) = if gender == 0 {
            (None, Some(new_parent_handle.to_string()))
        } else {
            (Some(new_parent_handle.to_string()), None)
        };
        let fam = family_repo::create(txn, father, mother, 0)?;
        append_child_to_family(txn, &fam.handle, person_handle)?;
        if !person.parent_family_list.iter().any(|h| h == &fam.handle) {
            person.parent_family_list.push(fam.handle);
            person_repo::save_row(txn, &mut person)?;
        }
    }
    Ok(())
}

/// Wire an existing person as a sibling of `person_handle`.
pub fn add_sibling_existing(
    txn: &Transaction,
    person_handle: &str,
    sibling_handle: &str,
) -> Result<()> {
    let person_json: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![person_handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load person {person_handle}"))?;
    let person: Person =
        serde_json::from_str(&person_json).context("parse person")?;

    let family_handle = person
        .parent_family_list
        .first()
        .ok_or_else(|| anyhow!("no parent family to add a sibling to - add parents first"))?
        .clone();

    append_child_to_family(txn, &family_handle, sibling_handle)?;

    let mut sib: Person = {
        let j: String = txn
            .query_row(
                "SELECT json_data FROM person WHERE handle = ?1",
                params![sibling_handle],
                |r| r.get(0),
            )
            .context("reload sibling")?;
        serde_json::from_str(&j).context("parse sibling")?
    };
    if !sib.parent_family_list.iter().any(|h| h == &family_handle) {
        sib.parent_family_list.push(family_handle);
        person_repo::save_row(txn, &mut sib)?;
    }
    Ok(())
}
