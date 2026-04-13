//! Person CRUD with cascade-aware deletion.
//!
//! Phase 5 model (what the UI can currently create / edit):
//!
//! - `first_name` and one `primary_name.surname_list[0].surname`
//! - `gender` (0 female / 1 male / 2 unknown)
//! - Optional birth year → a date-only Birth event referenced from
//!   `event_ref_list[birth_ref_index]`
//! - Optional death year → same pattern with a Death event
//!
//! Fields not surfaced in the Phase 5 form — alternate_names,
//! addresses, urls, attributes, lds_ord_list, media_list,
//! citation_list, note_list, tag_list, person_ref_list, and the
//! full family graph — are **preserved on update**. Creating a new
//! person initializes them as empty.
//!
//! Delete cascade (see `delete_with_cascade`):
//!
//! - For every family in `parent_family_list`, prune the person from
//!   `child_ref_list` and resave the family.
//! - For every family in `family_list`, clear `father_handle` or
//!   `mother_handle` where they match and resave the family.
//! - Delete any Birth/Death event that was owned exclusively by this
//!   person (only referenced by this person's `event_ref_list`) —
//!   caller opts into this via the `delete_owned_events` flag so the
//!   UI can offer a choice.
//! - Delete the person's `reference` rows and then the person row.

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, Transaction};

use super::common::{
    inbound_ref_count, new_handle, next_gramps_id, now_unix, rewrite_references, to_json,
};
use super::{event as event_repo, family as family_repo};
use crate::gramps::common::Typed;
use crate::gramps::event::EventRef;
use crate::gramps::person::{Name, Person, Surname};

/// Event type values — must match `gen/lib/eventtype.py` /
/// `views::widgets::date_display`.
const EVENT_BIRTH: i32 = 12;
const EVENT_DEATH: i32 = 13;

/// Summary of the impact deleting a person will have, so the UI can
/// show a "are you sure" banner before committing.
#[derive(Debug, Clone, Default)]
pub struct DeletePreview {
    pub handle: String,
    pub display_name: String,
    /// Families where this person is father or mother.
    pub parent_of: Vec<String>,
    /// Families where this person is listed as a child.
    pub child_of: Vec<String>,
    /// Number of events referenced by this person's event_ref_list.
    pub event_count: usize,
    /// Number of those events that are only referenced by this person
    /// and can therefore be safely deleted in cascade.
    pub exclusive_event_count: usize,
}

/// Read-only precomputation of what delete would do, without touching
/// the DB. Works on any `&Connection`; the write-side paths pass a
/// `&*txn` so the preview sees in-transaction state.
pub fn preview_delete(conn: &Connection, handle: &str) -> Result<DeletePreview> {
    let existing: String = conn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load person {handle}"))?;
    let person: Person = serde_json::from_str(&existing).context("parse person")?;

    let mut parent_of: Vec<String> = Vec::new();
    for h in &person.family_list {
        if let Some(gramps_id) = family_gramps_id(conn, h)? {
            parent_of.push(gramps_id);
        }
    }
    let mut child_of: Vec<String> = Vec::new();
    for h in &person.parent_family_list {
        if let Some(gramps_id) = family_gramps_id(conn, h)? {
            child_of.push(gramps_id);
        }
    }

    let event_count = person.event_ref_list.len();
    let mut exclusive_event_count = 0usize;
    for ev_ref in &person.event_ref_list {
        let inbound = inbound_ref_count(conn, &ev_ref.r#ref)?;
        if inbound <= 1 {
            exclusive_event_count += 1;
        }
    }

    Ok(DeletePreview {
        handle: handle.to_string(),
        display_name: person.primary_name.display(),
        parent_of,
        child_of,
        event_count,
        exclusive_event_count,
    })
}

/// Create a new person with the given name + gender. Optionally
/// attaches freshly-created Birth and Death events (year-only). The
/// returned record reflects what was written.
pub fn create(
    txn: &Transaction,
    first_name: &str,
    surname: &str,
    gender: i32,
    birth_year: Option<i32>,
    death_year: Option<i32>,
) -> Result<Person> {
    let mut person = Person {
        class: Some("Person".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "person", 'I')?,
        gender,
        change: now_unix(),
        private: false,
        primary_name: build_name(first_name, surname),
        alternate_names: Vec::new(),
        event_ref_list: Vec::new(),
        birth_ref_index: -1,
        death_ref_index: -1,
        family_list: Vec::new(),
        parent_family_list: Vec::new(),
        person_ref_list: Vec::new(),
        address_list: Vec::new(),
        urls: Vec::new(),
        lds_ord_list: Vec::new(),
        media_list: Vec::new(),
        attribute_list: Vec::new(),
        citation_list: Vec::new(),
        note_list: Vec::new(),
        tag_list: Vec::new(),
    };

    if let Some(y) = birth_year {
        let ev = event_repo::create_date_only(txn, EVENT_BIRTH, y)?;
        person.event_ref_list.push(primary_event_ref(&ev.handle));
        person.birth_ref_index = (person.event_ref_list.len() - 1) as i32;
    }
    if let Some(y) = death_year {
        let ev = event_repo::create_date_only(txn, EVENT_DEATH, y)?;
        person.event_ref_list.push(primary_event_ref(&ev.handle));
        person.death_ref_index = (person.event_ref_list.len() - 1) as i32;
    }

    insert(txn, &person)?;
    tracing::info!(handle = %person.handle, gramps_id = %person.gramps_id, "created person");
    Ok(person)
}

/// Update the editable subset of a person: name, gender, and the
/// birth/death event dates. Preserves every other field.
///
/// Date semantics:
///
/// - `birth_year = Some(0)` or `None` removes the birth event link
///   (the event row itself stays — orphan cleanup is the caller's
///   decision, and for update it's typically wrong to delete).
/// - `birth_year = Some(y > 0)` updates the existing birth event's
///   date if one is linked, or creates a new Birth event otherwise.
/// - Same for death.
pub fn update(
    txn: &Transaction,
    handle: &str,
    first_name: &str,
    surname: &str,
    gender: i32,
    birth_year: Option<i32>,
    death_year: Option<i32>,
) -> Result<Person> {
    let existing_json: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load person {handle}"))?;
    let mut person: Person =
        serde_json::from_str(&existing_json).context("parse existing person")?;

    person.primary_name = build_name(first_name, surname);
    person.gender = gender;
    apply_year_edit(
        txn,
        &mut person,
        EVENT_BIRTH,
        birth_year,
        PersonDateField::Birth,
    )?;
    apply_year_edit(
        txn,
        &mut person,
        EVENT_DEATH,
        death_year,
        PersonDateField::Death,
    )?;
    person.change = now_unix();

    update_row(txn, &person)?;
    tracing::info!(handle = %person.handle, "updated person");
    Ok(person)
}

/// Delete a person, cascading through the families that reference them.
///
/// `delete_owned_events = true` also removes any events in the
/// person's `event_ref_list` whose only inbound reference is this
/// person (i.e. "exclusive" events per `preview_delete`). Shared
/// events are left intact — they may be linked from other persons.
pub fn delete_with_cascade(
    txn: &Transaction,
    handle: &str,
    delete_owned_events: bool,
) -> Result<()> {
    let existing: String = txn
        .query_row(
            "SELECT json_data FROM person WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load person {handle}"))?;
    let person: Person = serde_json::from_str(&existing).context("parse person")?;

    // 1. Parent-of families — clear father/mother pointing at this person.
    for fam_handle in &person.family_list {
        let fam_json: String = match txn.query_row(
            "SELECT json_data FROM family WHERE handle = ?1",
            params![fam_handle],
            |r| r.get::<_, String>(0),
        ) {
            Ok(j) => j,
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => return Err(anyhow!("load family {fam_handle}: {e}")),
        };
        let mut family: crate::gramps::Family =
            serde_json::from_str(&fam_json).context("parse family")?;
        if family.father_handle.as_deref() == Some(handle) {
            family.father_handle = None;
        }
        if family.mother_handle.as_deref() == Some(handle) {
            family.mother_handle = None;
        }
        family_repo::save(txn, &mut family)?;
    }

    // 2. Child-of families — drop the person from child_ref_list.
    for fam_handle in &person.parent_family_list {
        let fam_json: String = match txn.query_row(
            "SELECT json_data FROM family WHERE handle = ?1",
            params![fam_handle],
            |r| r.get::<_, String>(0),
        ) {
            Ok(j) => j,
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => return Err(anyhow!("load family {fam_handle}: {e}")),
        };
        let mut family: crate::gramps::Family =
            serde_json::from_str(&fam_json).context("parse family")?;
        family
            .child_ref_list
            .retain(|cr| cr.r#ref.as_str() != handle);
        family_repo::save(txn, &mut family)?;
    }

    // 3. Optionally delete exclusive events (those only this person
    //    referenced). We must compute "exclusive" *before* we drop
    //    the person's own reference rows, otherwise every event will
    //    look exclusive.
    let exclusive_event_handles = if delete_owned_events {
        let mut out = Vec::new();
        for ev_ref in &person.event_ref_list {
            let inbound = inbound_ref_count(txn, &ev_ref.r#ref)?;
            if inbound <= 1 {
                out.push(ev_ref.r#ref.clone());
            }
        }
        out
    } else {
        Vec::new()
    };

    // 4. Delete the person row and its own outbound references.
    let removed = txn
        .execute("DELETE FROM person WHERE handle = ?1", params![handle])
        .context("delete person row")?;
    if removed == 0 {
        bail!("no person with handle {handle}");
    }
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete person's outbound reference rows")?;

    // 5. Now delete the owned events (their only referer is gone).
    for ev_handle in &exclusive_event_handles {
        let _ = event_repo::delete(txn, ev_handle);
    }

    tracing::info!(
        handle,
        family_parent_of = person.family_list.len(),
        family_child_of = person.parent_family_list.len(),
        deleted_events = exclusive_event_handles.len(),
        "deleted person with cascade"
    );
    Ok(())
}

fn insert(txn: &Transaction, person: &Person) -> Result<()> {
    let json = to_json(person)?;
    let surname = primary_surname_string(person);
    txn.execute(
        "INSERT INTO person (handle, given_name, surname, json_data, gramps_id, \
         gender, death_ref_index, birth_ref_index, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            &person.handle,
            &person.primary_name.first_name,
            surname,
            &json,
            &person.gramps_id,
            person.gender,
            person.death_ref_index,
            person.birth_ref_index,
            person.change,
            person.private as i32,
        ],
    )
    .context("insert person row")?;
    rewrite_references(txn, &person.handle, "Person", &outbound_refs(person))?;
    Ok(())
}

/// Rewrite an already-mutated `Person` row without going through
/// the public update API (which re-applies the editable subset).
/// Used by Phase 6a Family CRUD to keep reverse links consistent
/// (appending/removing a family from family_list / parent_family_list).
pub fn save_row(txn: &Transaction, person: &mut Person) -> Result<()> {
    person.change = now_unix();
    update_row(txn, person)
}

fn update_row(txn: &Transaction, person: &Person) -> Result<()> {
    let json = to_json(person)?;
    let surname = primary_surname_string(person);
    let updated = txn
        .execute(
            "UPDATE person SET given_name = ?2, surname = ?3, json_data = ?4, \
             gender = ?5, death_ref_index = ?6, birth_ref_index = ?7, \
             change = ?8, private = ?9 \
             WHERE handle = ?1",
            params![
                &person.handle,
                &person.primary_name.first_name,
                surname,
                &json,
                person.gender,
                person.death_ref_index,
                person.birth_ref_index,
                person.change,
                person.private as i32,
            ],
        )
        .context("update person row")?;
    if updated == 0 {
        bail!("no person with handle {}", person.handle);
    }
    rewrite_references(txn, &person.handle, "Person", &outbound_refs(person))?;
    Ok(())
}

fn outbound_refs(person: &Person) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for ev in &person.event_ref_list {
        out.push((ev.r#ref.clone(), "Event".to_string()));
    }
    for fam in &person.family_list {
        out.push((fam.clone(), "Family".to_string()));
    }
    for fam in &person.parent_family_list {
        out.push((fam.clone(), "Family".to_string()));
    }
    for pref in &person.person_ref_list {
        out.push((pref.r#ref.clone(), "Person".to_string()));
    }
    for m in &person.media_list {
        out.push((m.r#ref.clone(), "Media".to_string()));
    }
    for h in &person.citation_list {
        out.push((h.clone(), "Citation".to_string()));
    }
    for h in &person.note_list {
        out.push((h.clone(), "Note".to_string()));
    }
    for h in &person.tag_list {
        out.push((h.clone(), "Tag".to_string()));
    }
    out
}

fn build_name(first_name: &str, surname: &str) -> Name {
    Name {
        class: Some("Name".to_string()),
        first_name: first_name.to_string(),
        suffix: String::new(),
        title: String::new(),
        call: String::new(),
        nick: String::new(),
        famnick: String::new(),
        group_as: String::new(),
        sort_as: 0,
        display_as: 0,
        r#type: Typed {
            class: Some("NameType".to_string()),
            // NameType::Birth Name = 2
            value: 2,
            string: String::new(),
        },
        date: None,
        surname_list: vec![Surname {
            class: Some("Surname".to_string()),
            surname: surname.to_string(),
            prefix: String::new(),
            primary: true,
            connector: String::new(),
            origintype: Typed {
                class: Some("NameOriginType".to_string()),
                // NameOriginType::Inherited = 2 in gen/lib/nameorigintype.py
                value: 2,
                string: String::new(),
            },
        }],
        private: false,
        citation_list: Vec::new(),
        note_list: Vec::new(),
    }
}

fn primary_surname_string(person: &Person) -> String {
    person
        .primary_name
        .surname_list
        .iter()
        .find(|s| s.primary)
        .or_else(|| person.primary_name.surname_list.first())
        .map(|s| s.surname.clone())
        .unwrap_or_default()
}

fn primary_event_ref(handle: &str) -> EventRef {
    EventRef {
        class: Some("EventRef".to_string()),
        r#ref: handle.to_string(),
        private: false,
        citation_list: Vec::new(),
        note_list: Vec::new(),
        attribute_list: Vec::new(),
        role: Typed {
            class: Some("EventRoleType".to_string()),
            // EventRoleType::Primary = 1
            value: 1,
            string: String::new(),
        },
    }
}

#[derive(Copy, Clone)]
enum PersonDateField {
    Birth,
    Death,
}

/// Apply the requested year edit to `person`'s birth or death event.
///
/// - `Some(y > 0)` with existing linked event → update that event.
/// - `Some(y > 0)` with no existing link → create a new event and
///   append to `event_ref_list`, recording the new index.
/// - `Some(0)` or `None` → clear the link (leave the event row
///   intact; user can delete it via the event view).
fn apply_year_edit(
    txn: &Transaction,
    person: &mut Person,
    type_value: i32,
    year: Option<i32>,
    field: PersonDateField,
) -> Result<()> {
    let current_index: &mut i32 = match field {
        PersonDateField::Birth => &mut person.birth_ref_index,
        PersonDateField::Death => &mut person.death_ref_index,
    };

    let linked_handle: Option<String> = if *current_index >= 0 {
        person
            .event_ref_list
            .get(*current_index as usize)
            .map(|er| er.r#ref.clone())
    } else {
        None
    };

    match year {
        None | Some(0) => {
            *current_index = -1;
            // We intentionally leave the event row alone so we don't
            // clobber data the user might still want.
            let _ = linked_handle;
            Ok(())
        }
        Some(y) => {
            if let Some(handle) = linked_handle {
                event_repo::set_year(txn, &handle, y)?;
            } else {
                let ev = event_repo::create_date_only(txn, type_value, y)?;
                person.event_ref_list.push(primary_event_ref(&ev.handle));
                *current_index = (person.event_ref_list.len() - 1) as i32;
            }
            Ok(())
        }
    }
}

fn family_gramps_id(conn: &Connection, handle: &str) -> Result<Option<String>> {
    match conn.query_row(
        "SELECT gramps_id FROM family WHERE handle = ?1",
        params![handle],
        |r| r.get::<_, String>(0),
    ) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow!("load family gramps_id {handle}: {e}")),
    }
}
