//! Place CRUD.
//!
//! Places form a tree via `placeref_list` — each place points at
//! (at most) one parent. The Phase 6a form exposes the name, type,
//! latitude, longitude, and an optional parent place handle. Other
//! fields (alt_names, urls, citations, media, notes, tags) are
//! preserved on update.
//!
//! Delete is cascade-lite: we walk every `place` row whose first
//! `placeref_list` entry points at us and clear that link; every
//! `event.place` column that points at us is cleared too. This keeps
//! the tree valid without forcing the user to manually relink
//! downstream places and events.

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Transaction};

use super::common::{new_handle, next_gramps_id, now_unix, rewrite_references, to_json};
use crate::gramps::common::Typed;
use crate::gramps::place::{Place, PlaceName, PlaceRef};

pub fn create(
    txn: &Transaction,
    name: &str,
    type_value: i32,
    lat: &str,
    long: &str,
    parent_handle: Option<String>,
) -> Result<Place> {
    let place = Place {
        class: Some("Place".to_string()),
        handle: new_handle(),
        gramps_id: next_gramps_id(txn, "place", 'P')?,
        change: now_unix(),
        private: false,
        title: String::new(),
        long: long.to_string(),
        lat: lat.to_string(),
        code: String::new(),
        name: PlaceName {
            class: Some("PlaceName".to_string()),
            value: name.to_string(),
            lang: String::new(),
            date: None,
        },
        alt_names: Vec::new(),
        place_type: Typed {
            class: Some("PlaceType".to_string()),
            value: type_value,
            string: String::new(),
        },
        alt_loc: Vec::new(),
        placeref_list: parent_handle
            .filter(|s| !s.is_empty())
            .map(|h| {
                vec![PlaceRef {
                    class: Some("PlaceRef".to_string()),
                    r#ref: h,
                    date: None,
                }]
            })
            .unwrap_or_default(),
        urls: Vec::new(),
        media_list: Vec::new(),
        citation_list: Vec::new(),
        note_list: Vec::new(),
        tag_list: Vec::new(),
    };

    insert(txn, &place)?;
    tracing::info!(handle = %place.handle, gramps_id = %place.gramps_id, "created place");
    Ok(place)
}

pub fn update(
    txn: &Transaction,
    handle: &str,
    name: &str,
    type_value: i32,
    lat: &str,
    long: &str,
    parent_handle: Option<String>,
) -> Result<Place> {
    let existing: String = txn
        .query_row(
            "SELECT json_data FROM place WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load place {handle}"))?;
    let mut place: Place = serde_json::from_str(&existing).context("parse place")?;

    place.name.value = name.to_string();
    place.place_type.value = type_value;
    place.place_type.string = String::new();
    place.lat = lat.to_string();
    place.long = long.to_string();

    // Replace just the first (primary) placeref. Additional entries
    // in placeref_list beyond index 0 are preserved — Gramps core
    // uses these for dated historical place hierarchies (rare).
    let new_parent_ref = parent_handle.filter(|s| !s.is_empty()).map(|h| PlaceRef {
        class: Some("PlaceRef".to_string()),
        r#ref: h,
        date: None,
    });
    if place.placeref_list.is_empty() {
        if let Some(pr) = new_parent_ref {
            place.placeref_list.push(pr);
        }
    } else {
        match new_parent_ref {
            Some(pr) => place.placeref_list[0] = pr,
            None => {
                place.placeref_list.remove(0);
            }
        }
    }

    place.change = now_unix();

    update_row(txn, &place)?;
    tracing::info!(handle = %place.handle, "updated place");
    Ok(place)
}

/// Delete a place with cascade — clears downstream child places' parent
/// pointer and clears the `event.place` link on any event referencing
/// this place. Events and child places stay; only the link goes.
pub fn delete_with_cascade(txn: &Transaction, handle: &str) -> Result<()> {
    let existing: String = txn
        .query_row(
            "SELECT json_data FROM place WHERE handle = ?1",
            params![handle],
            |r| r.get(0),
        )
        .with_context(|| format!("load place {handle}"))?;
    let _place: Place = serde_json::from_str(&existing).context("parse place")?;

    // 1. Clear downstream child places' parent pointer.
    let child_handles: Vec<String> = {
        let mut stmt = txn
            .prepare("SELECT handle FROM place WHERE enclosed_by = ?1")
            .context("prepare child place query")?;
        let rows = stmt
            .query_map(params![handle], |r| r.get::<_, String>(0))
            .context("query child places")?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    for child_handle in child_handles {
        let child_json: String = match txn.query_row(
            "SELECT json_data FROM place WHERE handle = ?1",
            params![&child_handle],
            |r| r.get(0),
        ) {
            Ok(j) => j,
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => return Err(anyhow!("load child place {child_handle}: {e}")),
        };
        let mut child: Place =
            serde_json::from_str(&child_json).context("parse child place")?;
        child.placeref_list.retain(|pr| pr.r#ref != handle);
        child.change = now_unix();
        update_row(txn, &child)?;
    }

    // 2. Clear `event.place` on every event pointing at us. Updating
    //    the JSON is enough; rewrite_references will drop the place
    //    entry from the event's outbound refs.
    let event_handles: Vec<String> = {
        let mut stmt = txn
            .prepare("SELECT handle FROM event WHERE place = ?1")
            .context("prepare event place query")?;
        let rows = stmt
            .query_map(params![handle], |r| r.get::<_, String>(0))
            .context("query events by place")?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    for ev_handle in event_handles {
        super::event::clear_place(txn, &ev_handle)?;
    }

    // 3. Delete the place row and its outbound refs.
    let removed = txn
        .execute("DELETE FROM place WHERE handle = ?1", params![handle])
        .context("delete place row")?;
    if removed == 0 {
        bail!("no place with handle {handle}");
    }
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![handle],
    )
    .context("delete place outbound refs")?;
    tracing::info!(handle, "deleted place with cascade");
    Ok(())
}

fn insert(txn: &Transaction, place: &Place) -> Result<()> {
    let json = to_json(place)?;
    let enclosed_by = place
        .placeref_list
        .first()
        .map(|pr| pr.r#ref.clone());
    let title = if place.title.is_empty() {
        &place.name.value
    } else {
        &place.title
    };
    txn.execute(
        "INSERT INTO place (handle, enclosed_by, json_data, gramps_id, title, long, lat, code, change, private) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            &place.handle,
            enclosed_by,
            &json,
            &place.gramps_id,
            title,
            &place.long,
            &place.lat,
            &place.code,
            place.change,
            place.private as i32,
        ],
    )
    .context("insert place row")?;
    rewrite_references(txn, &place.handle, "Place", &outbound_refs(place))?;
    Ok(())
}

fn update_row(txn: &Transaction, place: &Place) -> Result<()> {
    let json = to_json(place)?;
    let enclosed_by = place
        .placeref_list
        .first()
        .map(|pr| pr.r#ref.clone());
    let title = if place.title.is_empty() {
        &place.name.value
    } else {
        &place.title
    };
    let updated = txn
        .execute(
            "UPDATE place SET enclosed_by = ?2, json_data = ?3, title = ?4, long = ?5, lat = ?6, code = ?7, change = ?8, private = ?9 \
             WHERE handle = ?1",
            params![
                &place.handle,
                enclosed_by,
                &json,
                title,
                &place.long,
                &place.lat,
                &place.code,
                place.change,
                place.private as i32,
            ],
        )
        .context("update place row")?;
    if updated == 0 {
        bail!("no place with handle {}", place.handle);
    }
    rewrite_references(txn, &place.handle, "Place", &outbound_refs(place))?;
    Ok(())
}

fn outbound_refs(place: &Place) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for pr in &place.placeref_list {
        out.push((pr.r#ref.clone(), "Place".to_string()));
    }
    for m in &place.media_list {
        out.push((m.r#ref.clone(), "Media".to_string()));
    }
    for h in &place.citation_list {
        out.push((h.clone(), "Citation".to_string()));
    }
    for h in &place.note_list {
        out.push((h.clone(), "Note".to_string()));
    }
    for h in &place.tag_list {
        out.push((h.clone(), "Tag".to_string()));
    }
    out
}
