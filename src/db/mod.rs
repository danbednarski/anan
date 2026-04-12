//! SQLite access to a Gramps family tree — both read and write.
//!
//! Two entry points:
//!
//! - [`load_snapshot`] opens the file *read-only*, parses every row into
//!   Rust structs, and drops the connection. This is what the
//!   `dump_db` example and any diagnostic tool should use. The returned
//!   [`Snapshot`] is a plain owned value safe to pass around.
//!
//! - [`Database::open`] opens the file *read-write* and keeps the
//!   connection alive for the duration of an editing session.
//!   [`Database::snapshot`] re-reads the tree on demand, and
//!   [`Database::write_txn`] runs a closure inside a SQLite transaction
//!   with backup-before-write and change-timestamp handling.
//!
//! Every primary type is stored as a `Vec<T>` so the UI can iterate in
//! load order, plus a `HashMap<String, usize>` in [`HandleIndex`] for
//! O(1) handle lookup. Views should prefer the `Snapshot::foo()`
//! accessors over touching the raw fields directly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::de::DeserializeOwned;

use crate::gramps::{Citation, Event, Family, Media, Note, Person, Place, Repository, Source, Tag};

pub mod database;
pub mod repo;

pub use database::{Database, BACKUP_INTERVAL, BACKUP_KEEP};

/// All primary objects loaded from a single tree, plus a lookup index.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub path: PathBuf,
    pub persons: Vec<Person>,
    pub families: Vec<Family>,
    pub events: Vec<Event>,
    pub places: Vec<Place>,
    pub sources: Vec<Source>,
    pub citations: Vec<Citation>,
    pub media: Vec<Media>,
    pub notes: Vec<Note>,
    pub repositories: Vec<Repository>,
    pub tags: Vec<Tag>,
    pub index: HandleIndex,
}

/// Handle → position-in-Vec maps for every primary type.
#[derive(Debug, Clone, Default)]
pub struct HandleIndex {
    pub persons: HashMap<String, usize>,
    pub families: HashMap<String, usize>,
    pub events: HashMap<String, usize>,
    pub places: HashMap<String, usize>,
    pub sources: HashMap<String, usize>,
    pub citations: HashMap<String, usize>,
    pub media: HashMap<String, usize>,
    pub notes: HashMap<String, usize>,
    pub repositories: HashMap<String, usize>,
    pub tags: HashMap<String, usize>,
}

impl Snapshot {
    pub fn person(&self, handle: &str) -> Option<&Person> {
        self.index.persons.get(handle).and_then(|&i| self.persons.get(i))
    }
    pub fn family(&self, handle: &str) -> Option<&Family> {
        self.index.families.get(handle).and_then(|&i| self.families.get(i))
    }
    pub fn event(&self, handle: &str) -> Option<&Event> {
        self.index.events.get(handle).and_then(|&i| self.events.get(i))
    }
    pub fn place(&self, handle: &str) -> Option<&Place> {
        self.index.places.get(handle).and_then(|&i| self.places.get(i))
    }
    pub fn source(&self, handle: &str) -> Option<&Source> {
        self.index.sources.get(handle).and_then(|&i| self.sources.get(i))
    }
    pub fn citation(&self, handle: &str) -> Option<&Citation> {
        self.index.citations.get(handle).and_then(|&i| self.citations.get(i))
    }
    pub fn media_item(&self, handle: &str) -> Option<&Media> {
        self.index.media.get(handle).and_then(|&i| self.media.get(i))
    }
    pub fn note(&self, handle: &str) -> Option<&Note> {
        self.index.notes.get(handle).and_then(|&i| self.notes.get(i))
    }
    pub fn repository(&self, handle: &str) -> Option<&Repository> {
        self.index.repositories.get(handle).and_then(|&i| self.repositories.get(i))
    }
    pub fn tag(&self, handle: &str) -> Option<&Tag> {
        self.index.tags.get(handle).and_then(|&i| self.tags.get(i))
    }
}

/// Open a Gramps SQLite file read-only and load every primary table.
/// Fails loudly on the first row whose `json_data` does not match our
/// structs — we want schema drift to be obvious, not silent.
pub fn load_snapshot(path: &Path) -> Result<Snapshot> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {}", path.display()))?;
    load_from_conn(&conn, path.to_path_buf())
}

/// Shared reader used by [`load_snapshot`] and [`Database::snapshot`].
/// Caller is responsible for providing an open connection with
/// sufficient privileges; this function just runs the SELECTs.
pub(crate) fn load_from_conn(conn: &Connection, path: PathBuf) -> Result<Snapshot> {
    let persons: Vec<Person> = load_all(conn, "person")?;
    let families: Vec<Family> = load_all(conn, "family")?;
    let events: Vec<Event> = load_all(conn, "event")?;
    let places: Vec<Place> = load_all(conn, "place")?;
    let sources: Vec<Source> = load_all(conn, "source")?;
    let citations: Vec<Citation> = load_all(conn, "citation")?;
    let media: Vec<Media> = load_all(conn, "media")?;
    let notes: Vec<Note> = load_all(conn, "note")?;
    let repositories: Vec<Repository> = load_all(conn, "repository")?;
    let tags: Vec<Tag> = load_all(conn, "tag")?;

    let index = HandleIndex {
        persons: index_by_handle(&persons, |p| &p.handle),
        families: index_by_handle(&families, |f| &f.handle),
        events: index_by_handle(&events, |e| &e.handle),
        places: index_by_handle(&places, |p| &p.handle),
        sources: index_by_handle(&sources, |s| &s.handle),
        citations: index_by_handle(&citations, |c| &c.handle),
        media: index_by_handle(&media, |m| &m.handle),
        notes: index_by_handle(&notes, |n| &n.handle),
        repositories: index_by_handle(&repositories, |r| &r.handle),
        tags: index_by_handle(&tags, |t| &t.handle),
    };

    Ok(Snapshot {
        path,
        persons,
        families,
        events,
        places,
        sources,
        citations,
        media,
        notes,
        repositories,
        tags,
        index,
    })
}

fn index_by_handle<T>(items: &[T], key: impl Fn(&T) -> &String) -> HashMap<String, usize> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| (key(item).clone(), i))
        .collect()
}

/// Generic loader used by [`load_from_conn`] and by `examples/dump_db.rs`.
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
