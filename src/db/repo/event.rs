//! Event CRUD — Phase 5 shipped `create_date_only` for Person's
//! birth/death path; Phase 6a adds `create_full` and `update_full`
//! with type / description / place / full Date support.
//!
//! Delete is still refuse-on-use (inbound_ref_count > 0 → bail).
//! Users who want to delete a linked event should unlink it from
//! the referencing Person or Family first.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{
    inbound_ref_count, new_handle, next_gramps_id, now_unix, rewrite_references, to_json,
};
use crate::gramps::common::Typed;
use crate::gramps::date::{Date, DateVal};
use crate::gramps::Event;

/// Create a date-only Event with the given type value and year.
/// `year == 0` yields an Event with no meaningful date — callers that
/// want "unknown" should prefer `None` instead.
pub fn create_date_only(txn: &Transaction, type_value: i32, year: i32) -> Result<Event> {
    let event = Event {
        class: Some("Event".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "event", 'E')?,
        change: now_unix(),
        private: false,
        r#type: Typed {
            class: Some("EventType".to_string()),
            value: type_value,
            string: String::new(),
        },
        description: String::new(),
        place: String::new(),
        date: Some(year_only_date(year)),
        citation_list: Vec::new(),
        note_list: Vec::new(),
        media_list: Vec::new(),
        attribute_list: Vec::new(),
        tag_list: Vec::new(),
    };

    insert(txn, &event)?;
    tracing::info!(handle = %event.handle, type_value, year, "created event");
    Ok(event)
}

/// Update just the date of an existing event. Leaves every other
/// field intact. `year == 0` clears the date.
pub fn set_year(txn: &Transaction, handle: &str, year: i32) -> Result<Event> {
    let existing_json: String = txn
        .query_row(
            "SELECT json_data FROM event WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load event {handle}"))?;
    let mut event: Event =
        serde_json::from_str(&existing_json).context("parse existing event")?;

    event.date = if year == 0 {
        None
    } else {
        Some(year_only_date(year))
    };
    event.change = now_unix();

    update_row(txn, &event)?;
    Ok(event)
}

/// Delete an event. Refuses if any object still references it.
pub fn delete(txn: &Transaction, handle: &str) -> Result<()> {
    let refs = inbound_ref_count(txn, handle)?;
    if refs > 0 {
        bail!(
            "cannot delete event {handle}: still referenced by {refs} object(s)."
        );
    }
    let removed = txn
        .execute("DELETE FROM event WHERE handle = ?1", params![handle])
        .context("delete event row")?;
    if removed == 0 {
        bail!("no event with handle {handle}");
    }
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete own reference rows")?;
    tracing::info!(handle, "deleted event");
    Ok(())
}

fn insert(txn: &Transaction, event: &Event) -> Result<()> {
    let json = to_json(event)?;
    let place = if event.place.is_empty() {
        None
    } else {
        Some(event.place.clone())
    };
    txn.execute(
        "INSERT INTO event (handle, json_data, gramps_id, description, place, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            &event.handle,
            &json,
            &event.gramps_id,
            &event.description,
            place,
            event.change,
            event.private as i32,
        ],
    )
    .context("insert event row")?;
    rewrite_references(txn, &event.handle, "Event", &outbound_refs(event))?;
    Ok(())
}

fn update_row(txn: &Transaction, event: &Event) -> Result<()> {
    let json = to_json(event)?;
    let place = if event.place.is_empty() {
        None
    } else {
        Some(event.place.clone())
    };
    let updated = txn
        .execute(
            "UPDATE event SET json_data = ?2, description = ?3, place = ?4, change = ?5, private = ?6 \
             WHERE handle = ?1",
            params![
                &event.handle,
                &json,
                &event.description,
                place,
                event.change,
                event.private as i32,
            ],
        )
        .context("update event row")?;
    if updated == 0 {
        bail!("no event with handle {}", event.handle);
    }
    rewrite_references(txn, &event.handle, "Event", &outbound_refs(event))?;
    Ok(())
}

fn outbound_refs(event: &Event) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !event.place.is_empty() {
        out.push((event.place.clone(), "Place".to_string()));
    }
    for h in &event.citation_list {
        out.push((h.clone(), "Citation".to_string()));
    }
    for h in &event.note_list {
        out.push((h.clone(), "Note".to_string()));
    }
    for mref in &event.media_list {
        out.push((mref.r#ref.clone(), "Media".to_string()));
    }
    for h in &event.tag_list {
        out.push((h.clone(), "Tag".to_string()));
    }
    out
}

/// Build a Gramps Date carrying just a year and nothing else. Matches
/// the shape observed in the fixture: `dateval = [0, 0, year, false]`,
/// sortval = 0, modifier = 0 (none).
fn year_only_date(year: i32) -> Date {
    Date {
        class: Some("Date".to_string()),
        calendar: 0,
        modifier: 0,
        quality: 0,
        dateval: Some(DateVal::Simple(0, 0, year, false)),
        text: String::new(),
        sortval: 0,
        newyear: 0,
        format: None,
        year: Some(year),
    }
}

/// Create an Event with type, description, optional place handle,
/// and an optional full Date. Used by the Phase 6a Event UI.
pub fn create_full(
    txn: &Transaction,
    type_value: i32,
    description: &str,
    place_handle: Option<String>,
    date: Option<Date>,
) -> Result<Event> {
    let event = Event {
        class: Some("Event".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "event", 'E')?,
        change: now_unix(),
        private: false,
        r#type: Typed {
            class: Some("EventType".to_string()),
            value: type_value,
            string: String::new(),
        },
        description: description.to_string(),
        place: place_handle.unwrap_or_default(),
        date,
        citation_list: Vec::new(),
        note_list: Vec::new(),
        media_list: Vec::new(),
        attribute_list: Vec::new(),
        tag_list: Vec::new(),
    };
    insert(txn, &event)?;
    tracing::info!(handle = %event.handle, gramps_id = %event.gramps_id, "created event (full)");
    Ok(event)
}

/// Clear the `place` field on an event. Used by Phase 6a Place
/// delete cascade when the deleted place is currently linked from
/// one or more events — we null out the link rather than refuse the
/// delete.
pub(super) fn clear_place(txn: &Transaction, handle: &str) -> Result<()> {
    let existing_json: String = match txn.query_row(
        "SELECT json_data FROM event WHERE handle = ?1",
        params![handle],
        |r| r.get(0),
    ) {
        Ok(j) => j,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(e) => return Err(anyhow::anyhow!("load event {handle}: {e}")),
    };
    let mut event: Event =
        serde_json::from_str(&existing_json).context("parse event")?;
    if event.place.is_empty() {
        return Ok(());
    }
    event.place.clear();
    event.change = now_unix();
    update_row(txn, &event)?;
    Ok(())
}

/// Update the editable subset of an existing event: type, description,
/// place link, date. Preserves cross-ref lists and tags.
pub fn update_full(
    txn: &Transaction,
    handle: &str,
    type_value: i32,
    description: &str,
    place_handle: Option<String>,
    date: Option<Date>,
) -> Result<Event> {
    let existing_json: String = txn
        .query_row(
            "SELECT json_data FROM event WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load event {handle}"))?;
    let mut event: Event =
        serde_json::from_str(&existing_json).context("parse existing event")?;

    event.r#type.value = type_value;
    event.r#type.string = String::new();
    event.description = description.to_string();
    event.place = place_handle.unwrap_or_default();
    event.date = date;
    event.change = now_unix();

    update_row(txn, &event)?;
    tracing::info!(handle = %event.handle, "updated event (full)");
    Ok(event)
}
