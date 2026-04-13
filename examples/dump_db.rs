//! Phase 1 smoke test — open `test-fixtures/sample.db` read-only and
//! deserialize every row of every primary-object table into our Rust
//! structs. Prints a summary and the first three rows of each type.
//!
//! Run: `cargo run --example dump_db`

use std::path::PathBuf;

use anyhow::{Context, Result};
use anan::gramps::{
    self, Citation, Event, Family, Media, Note, Person, Place, Repository, Source, Tag,
};
use rusqlite::{Connection, OpenFlags};
use serde::de::DeserializeOwned;

fn main() -> Result<()> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/sample.db");
    println!("opening {}", fixture.display());

    let conn = Connection::open_with_flags(&fixture, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {}", fixture.display()))?;

    // DB schema version metadata (research task — see PLAN.md).
    if let Ok(version) = conn.query_row::<String, _, _>(
        "SELECT json_data FROM metadata WHERE setting = 'version'",
        [],
        |r| r.get(0),
    ) {
        println!("metadata.version = {version}");
    }

    println!();
    println!("== primary object counts and parse status ==");

    let persons: Vec<Person> = load_all(&conn, "person")?;
    report("person", &persons, |p| {
        format!("{} [{}]", p.primary_name.display(), p.gramps_id)
    });

    let families: Vec<Family> = load_all(&conn, "family")?;
    report("family", &families, |f| {
        format!(
            "{} ({} children, father={}, mother={})",
            f.gramps_id,
            f.child_ref_list.len(),
            f.father_handle.as_deref().unwrap_or("-"),
            f.mother_handle.as_deref().unwrap_or("-"),
        )
    });

    let events: Vec<Event> = load_all(&conn, "event")?;
    report("event", &events, |e| {
        let label = gramps::enums::event_type_label(e.r#type.value)
            .unwrap_or("custom")
            .to_string();
        let year = e
            .date
            .as_ref()
            .map(|d| d.primary_year())
            .filter(|y| *y != 0)
            .map(|y| y.to_string())
            .unwrap_or_else(|| "-".to_string());
        format!("{} {} ({})", e.gramps_id, label, year)
    });

    let places: Vec<Place> = load_all(&conn, "place")?;
    report("place", &places, |p| {
        let kind = gramps::enums::place_type_label(p.place_type.value).unwrap_or("custom");
        format!("{} {} ({})", p.gramps_id, p.name.value, kind)
    });

    let sources: Vec<Source> = load_all(&conn, "source")?;
    report("source", &sources, |s| {
        format!("{} {} — {}", s.gramps_id, s.title, s.author)
    });

    let citations: Vec<Citation> = load_all(&conn, "citation")?;
    report("citation", &citations, |c| {
        format!("{} conf={} {}", c.gramps_id, c.confidence, c.page)
    });

    let media: Vec<Media> = load_all(&conn, "media")?;
    report("media", &media, |m| {
        format!("{} {} [{}]", m.gramps_id, m.desc, m.mime)
    });

    let notes: Vec<Note> = load_all(&conn, "note")?;
    report("note", &notes, |n| {
        let kind = gramps::enums::note_type_label(n.r#type.value).unwrap_or("custom");
        let preview: String = n.text.string.chars().take(60).collect();
        format!("{} [{}] {}", n.gramps_id, kind, preview)
    });

    let repositories: Vec<Repository> = load_all(&conn, "repository")?;
    report("repository", &repositories, |r| {
        format!("{} {}", r.gramps_id, r.name)
    });

    // Tag is a primary table but not one of the "9 primary types". Still,
    // parse it so the data model stays coherent.
    let tags: Vec<Tag> = load_all(&conn, "tag")?;
    report("tag", &tags, |t| format!("{} ({})", t.name, t.color));

    println!();
    println!("== OK — every row in every primary table parsed cleanly ==");

    Ok(())
}

/// Load every `json_data` row from `table` and deserialize into `T`.
///
/// Fails loudly on the first row that does not parse — the error message
/// includes the row handle-or-index so we can iterate on struct definitions.
fn load_all<T: DeserializeOwned>(conn: &Connection, table: &str) -> Result<Vec<T>> {
    let sql = format!("SELECT json_data FROM {table}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;

    let mut out = Vec::new();
    for (idx, row) in rows.enumerate() {
        let json = row?;
        let parsed: T = serde_json::from_str(&json)
            .with_context(|| format!("deserialize {table}[{idx}]: {json}"))?;
        out.push(parsed);
    }
    Ok(out)
}

fn report<T>(table: &str, rows: &[T], render: impl Fn(&T) -> String) {
    println!("{:12} {:4} rows", table, rows.len());
    for item in rows.iter().take(3) {
        println!("    - {}", render(item));
    }
}
