//! Read-only SQLite access to a Gramps family tree.
//!
//! Phase 2 only needs a bulk snapshot loader: `load_snapshot(path)` opens
//! the DB read-only, deserializes every `person`/`family`/`event` row into
//! the typed structs from `crate::gramps`, and returns an in-memory
//! `Snapshot`. For a 48-person fixture this is instant; a bigger tree
//! will still fit comfortably in memory (a few MB at most).
//!
//! When writes arrive (Phase 4+) we'll grow this module into a proper
//! `Database` type with prepared statements and a transaction wrapper.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::de::DeserializeOwned;

use crate::gramps::{Citation, Event, Family, Media, Note, Person, Place, Repository, Source, Tag};

/// Everything we need to render the read-only UI for a single tree.
///
/// `Clone` is derived so that the `Snapshot` can travel inside `Message`
/// (iced requires `Clone` on messages). In practice iced never actually
/// clones these — messages bounce once from runtime to `update`. If
/// snapshots grow large enough for that to matter we'll wrap this in
/// `Arc<Snapshot>` and drop `Clone`.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub path: PathBuf,
    pub persons: Vec<Person>,
    pub families: HashMap<String, Family>,
    pub events: HashMap<String, Event>,
    pub places: HashMap<String, Place>,
}

/// Open a Gramps SQLite file read-only and load the primary tables we
/// currently render. Fails loudly on the first row whose `json_data` does
/// not match our structs.
pub fn load_snapshot(path: &Path) -> Result<Snapshot> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {}", path.display()))?;

    let persons: Vec<Person> = load_all(&conn, "person")?;
    let families: Vec<Family> = load_all(&conn, "family")?;
    let events: Vec<Event> = load_all(&conn, "event")?;
    let places: Vec<Place> = load_all(&conn, "place")?;

    Ok(Snapshot {
        path: path.to_path_buf(),
        persons,
        families: families.into_iter().map(|f| (f.handle.clone(), f)).collect(),
        events: events.into_iter().map(|e| (e.handle.clone(), e)).collect(),
        places: places.into_iter().map(|p| (p.handle.clone(), p)).collect(),
    })
}

/// Generic loader used by [`load_snapshot`] and by `examples/dump_db.rs`.
/// Deserializes every `json_data` row of `table` into `T`.
pub fn load_all<T: DeserializeOwned>(conn: &Connection, table: &str) -> Result<Vec<T>> {
    let sql = format!("SELECT json_data FROM {table}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;

    let mut out = Vec::new();
    for (idx, row) in rows.enumerate() {
        let json = row?;
        let parsed: T = serde_json::from_str(&json)
            .with_context(|| format!("deserialize {table}[{idx}]"))?;
        out.push(parsed);
    }
    Ok(out)
}

// The following helpers are not used by the current UI but exist so
// `examples/dump_db.rs` (and forthcoming read views) can pull any primary
// type through a single entry point.
#[allow(dead_code)]
pub fn load_sources(conn: &Connection) -> Result<Vec<Source>> {
    load_all(conn, "source")
}
#[allow(dead_code)]
pub fn load_citations(conn: &Connection) -> Result<Vec<Citation>> {
    load_all(conn, "citation")
}
#[allow(dead_code)]
pub fn load_media(conn: &Connection) -> Result<Vec<Media>> {
    load_all(conn, "media")
}
#[allow(dead_code)]
pub fn load_notes(conn: &Connection) -> Result<Vec<Note>> {
    load_all(conn, "note")
}
#[allow(dead_code)]
pub fn load_repositories(conn: &Connection) -> Result<Vec<Repository>> {
    load_all(conn, "repository")
}
#[allow(dead_code)]
pub fn load_tags(conn: &Connection) -> Result<Vec<Tag>> {
    load_all(conn, "tag")
}
