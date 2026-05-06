//! anan-cli — command-line interface for Gramps SQLite trees.
//!
//! Designed for non-interactive use (scripts, agents, LLMs):
//! - A single `--tree <path>` flag (or `ANAN_TREE` env var) selects the file.
//! - Every read prints JSON to stdout. Errors go to stderr with non-zero exit.
//! - Every write goes through `Database::write_txn`, which auto-snapshots the
//!   tree before touching it, so a bad command can't silently corrupt data.
//!
//! See `LLMS.md` for the agent-facing usage guide.

use std::path::PathBuf;
use std::process::ExitCode;

use anan::db::{repo, Database, Snapshot};
use anan::gramps::date::{Date, DateVal};
use anan::gramps::enums;
use anan::gramps::Person;
use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};

const EVENT_BIRTH: i32 = 12;
const EVENT_DEATH: i32 = 13;

#[derive(Parser, Debug)]
#[command(
    name = "anan-cli",
    version,
    about = "Read and edit Gramps SQLite trees from the command line.",
    long_about = "Every read prints JSON. Every write goes through a transaction \
                  with an auto-snapshot of the tree first.\n\n\
                  Tree path comes from --tree or $ANAN_TREE."
)]
struct Cli {
    /// Path to the Gramps SQLite file (sqlite.db inside a tree dir).
    #[arg(long, short = 't', global = true, env = "ANAN_TREE")]
    tree: Option<PathBuf>,

    /// One-line JSON output (default is pretty-printed).
    #[arg(long, global = true)]
    compact: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Print counts of every primary type.
    Stats,

    /// Print every row of one or all tables as JSON.
    Dump {
        /// Restrict to a single table: person, family, event, place,
        /// source, citation, media, note, repository, tag.
        #[arg(long)]
        table: Option<String>,
    },

    /// Substring search across person display names + gramps IDs + place names.
    Search {
        /// Query string (case-insensitive substring).
        query: String,

        /// Max results per type.
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },

    /// Person CRUD.
    #[command(subcommand)]
    Person(PersonOp),

    /// Family CRUD and child-list management.
    #[command(subcommand)]
    Family(FamilyOp),

    /// Event CRUD.
    #[command(subcommand)]
    Event(EventOp),

    /// Place CRUD.
    #[command(subcommand)]
    Place(PlaceOp),
}

#[derive(Subcommand, Debug)]
enum PersonOp {
    /// List persons (id, display name, gender, birth/death year).
    List {
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Get one person by handle or gramps_id (e.g. I0001).
    Get { id: String },
    /// Create a new person.
    Add {
        #[arg(long)]
        first: String,
        #[arg(long)]
        surname: String,
        #[arg(long, value_enum, default_value_t = GenderArg::Unknown)]
        gender: GenderArg,
        /// Birth date as YYYY, YYYY-MM, or YYYY-MM-DD.
        #[arg(long)]
        birth: Option<String>,
        /// Death date as YYYY, YYYY-MM, or YYYY-MM-DD.
        #[arg(long)]
        death: Option<String>,
    },
    /// Update name / gender / birth / death. Pass --clear-birth or
    /// --clear-death to remove a date link without setting a new one.
    Update {
        id: String,
        #[arg(long)]
        first: Option<String>,
        #[arg(long)]
        surname: Option<String>,
        #[arg(long, value_enum)]
        gender: Option<GenderArg>,
        #[arg(long)]
        birth: Option<String>,
        #[arg(long)]
        death: Option<String>,
        #[arg(long)]
        clear_birth: bool,
        #[arg(long)]
        clear_death: bool,
    },
    /// Delete a person, cascading through their families.
    Delete {
        id: String,
        /// Also delete events that only this person referenced.
        #[arg(long)]
        cascade_events: bool,
    },
}

#[derive(Subcommand, Debug)]
enum FamilyOp {
    /// List families (id, parents, child count).
    List {
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Get one family by id.
    Get { id: String },
    /// Create a family from existing parents (one or both required).
    Add {
        #[arg(long)]
        father: Option<String>,
        #[arg(long)]
        mother: Option<String>,
        /// Relationship type: married, unmarried, civil-union, unknown.
        #[arg(long, value_enum, default_value_t = FamilyRelArg::Married)]
        rel: FamilyRelArg,
    },
    /// Update parents/relationship-type. Use empty string to unset a parent.
    Update {
        id: String,
        #[arg(long)]
        father: Option<String>,
        #[arg(long)]
        mother: Option<String>,
        #[arg(long, value_enum)]
        rel: Option<FamilyRelArg>,
    },
    /// Delete a family, unlinking parents and children but keeping their rows.
    Delete { id: String },
    /// Append an existing person to a family's child list.
    AddChild {
        family: String,
        child: String,
    },
}

#[derive(Subcommand, Debug)]
enum EventOp {
    /// List events.
    List {
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Get one event by id.
    Get { id: String },
    /// Create an event. Type is the integer code (12=Birth, 13=Death,
    /// 1=Marriage, 19=Burial, ...) or the english label.
    Add {
        #[arg(long)]
        r#type: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        date: Option<String>,
        /// Place handle or gramps_id.
        #[arg(long)]
        place: Option<String>,
    },
    /// Delete an event. Refuses if the event is still referenced.
    Delete { id: String },
}

#[derive(Subcommand, Debug)]
enum PlaceOp {
    List {
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    Get { id: String },
    /// Create a place. Type is the integer code or label (e.g. "City").
    Add {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "Unknown")]
        r#type: String,
        #[arg(long, default_value = "")]
        lat: String,
        #[arg(long, default_value = "")]
        long: String,
        /// Optional parent place handle or gramps_id.
        #[arg(long)]
        parent: Option<String>,
    },
    /// Delete a place; clears the link from any referencing event.
    Delete { id: String },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum GenderArg {
    Female,
    Male,
    Unknown,
    Other,
}

impl GenderArg {
    fn as_i32(self) -> i32 {
        match self {
            Self::Female => 0,
            Self::Male => 1,
            Self::Unknown => 2,
            Self::Other => 3,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum FamilyRelArg {
    Married,
    Unmarried,
    CivilUnion,
    Unknown,
}

impl FamilyRelArg {
    fn as_i32(self) -> i32 {
        match self {
            Self::Married => 0,
            Self::Unmarried => 1,
            Self::CivilUnion => 2,
            Self::Unknown => 3,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    // Stats/Search/Dump/Get only need the snapshot, but every command
    // first opens the DB so the schema check runs uniformly.
    let path = cli
        .tree
        .clone()
        .ok_or_else(|| anyhow!("no tree path: pass --tree <path> or set ANAN_TREE"))?;
    let db = Database::open(&path).with_context(|| format!("open {}", path.display()))?;

    match cli.cmd {
        Cmd::Stats => print_json(&stats(&db)?, cli.compact),
        Cmd::Dump { table } => print_json(&dump(&db, table.as_deref())?, cli.compact),
        Cmd::Search { query, limit } => {
            print_json(&search(&db, &query, limit)?, cli.compact)
        }

        Cmd::Person(op) => person_cmd(&db, op, cli.compact),
        Cmd::Family(op) => family_cmd(&db, op, cli.compact),
        Cmd::Event(op) => event_cmd(&db, op, cli.compact),
        Cmd::Place(op) => place_cmd(&db, op, cli.compact),
    }
}

// ---------- snapshot-level commands ----------

fn stats(db: &Database) -> Result<Value> {
    let snap = db.snapshot()?;
    Ok(json!({
        "path": snap.path,
        "persons": snap.persons.len(),
        "families": snap.families.len(),
        "events": snap.events.len(),
        "places": snap.places.len(),
        "sources": snap.sources.len(),
        "citations": snap.citations.len(),
        "media": snap.media.len(),
        "notes": snap.notes.len(),
        "repositories": snap.repositories.len(),
        "tags": snap.tags.len(),
    }))
}

fn dump(db: &Database, table: Option<&str>) -> Result<Value> {
    let snap = db.snapshot()?;
    let v = match table {
        None => serde_json::to_value(SnapshotJson::from(&snap))?,
        Some("person") => serde_json::to_value(&snap.persons)?,
        Some("family") => serde_json::to_value(&snap.families)?,
        Some("event") => serde_json::to_value(&snap.events)?,
        Some("place") => serde_json::to_value(&snap.places)?,
        Some("source") => serde_json::to_value(&snap.sources)?,
        Some("citation") => serde_json::to_value(&snap.citations)?,
        Some("media") => serde_json::to_value(&snap.media)?,
        Some("note") => serde_json::to_value(&snap.notes)?,
        Some("repository") => serde_json::to_value(&snap.repositories)?,
        Some("tag") => serde_json::to_value(&snap.tags)?,
        Some(t) => bail!("unknown table {t:?}"),
    };
    Ok(v)
}

#[derive(serde::Serialize)]
struct SnapshotJson<'a> {
    persons: &'a [anan::gramps::Person],
    families: &'a [anan::gramps::Family],
    events: &'a [anan::gramps::Event],
    places: &'a [anan::gramps::Place],
    sources: &'a [anan::gramps::Source],
    citations: &'a [anan::gramps::Citation],
    media: &'a [anan::gramps::Media],
    notes: &'a [anan::gramps::Note],
    repositories: &'a [anan::gramps::Repository],
    tags: &'a [anan::gramps::Tag],
}

impl<'a> From<&'a Snapshot> for SnapshotJson<'a> {
    fn from(s: &'a Snapshot) -> Self {
        Self {
            persons: &s.persons,
            families: &s.families,
            events: &s.events,
            places: &s.places,
            sources: &s.sources,
            citations: &s.citations,
            media: &s.media,
            notes: &s.notes,
            repositories: &s.repositories,
            tags: &s.tags,
        }
    }
}

fn search(db: &Database, query: &str, limit: usize) -> Result<Value> {
    let snap = db.snapshot()?;
    let q = query.to_lowercase();
    let q_str = q.as_str();

    let persons: Vec<Value> = snap
        .persons
        .iter()
        .filter(|p| {
            p.gramps_id.to_lowercase().contains(q_str)
                || p.primary_name.display().to_lowercase().contains(q_str)
        })
        .take(limit)
        .map(person_summary)
        .collect();

    let places: Vec<Value> = snap
        .places
        .iter()
        .filter(|p| {
            p.gramps_id.to_lowercase().contains(q_str)
                || p.name.value.to_lowercase().contains(q_str)
        })
        .take(limit)
        .map(|p| {
            json!({
                "handle": p.handle,
                "gramps_id": p.gramps_id,
                "name": p.name.value,
                "type": enums::place_type_label(p.place_type.value)
                    .unwrap_or("Custom"),
            })
        })
        .collect();

    Ok(json!({ "persons": persons, "places": places }))
}

// ---------- person commands ----------

fn person_cmd(db: &Database, op: PersonOp, compact: bool) -> Result<()> {
    let value = match op {
        PersonOp::List { limit } => {
            let snap = db.snapshot()?;
            let take = if limit == 0 { snap.persons.len() } else { limit };
            let rows: Vec<Value> = snap.persons.iter().take(take).map(person_summary).collect();
            json!({ "count": snap.persons.len(), "persons": rows })
        }

        PersonOp::Get { id } => {
            let snap = db.snapshot()?;
            let p = resolve_person(&snap, &id)?;
            person_detail(&snap, p)
        }

        PersonOp::Add {
            first,
            surname,
            gender,
            birth,
            death,
        } => {
            let bd = parse_date_opt(birth.as_deref())?;
            let dd = parse_date_opt(death.as_deref())?;
            let person = db.write_txn(|txn| {
                repo::person::create(txn, &first, &surname, gender.as_i32(), bd, dd)
            })?;
            person_detail(&db.snapshot()?, &person)
        }

        PersonOp::Update {
            id,
            first,
            surname,
            gender,
            birth,
            death,
            clear_birth,
            clear_death,
        } => {
            let snap = db.snapshot()?;
            let existing = resolve_person(&snap, &id)?.clone();
            let new_first = first.unwrap_or_else(|| existing.primary_name.first_name.clone());
            let new_surname = surname.unwrap_or_else(|| {
                existing
                    .primary_name
                    .surname_list
                    .first()
                    .map(|s| s.surname.clone())
                    .unwrap_or_default()
            });
            let new_gender = gender.map(|g| g.as_i32()).unwrap_or(existing.gender);

            // Birth: explicit clear wins; new value parses; otherwise reuse existing.
            let new_birth = if clear_birth {
                None
            } else if let Some(s) = birth.as_deref() {
                parse_date_opt(Some(s))?
            } else {
                read_event_date(&snap, &existing, EVENT_BIRTH)
            };
            let new_death = if clear_death {
                None
            } else if let Some(s) = death.as_deref() {
                parse_date_opt(Some(s))?
            } else {
                read_event_date(&snap, &existing, EVENT_DEATH)
            };

            let person = db.write_txn(|txn| {
                repo::person::update(
                    txn,
                    &existing.handle,
                    &new_first,
                    &new_surname,
                    new_gender,
                    new_birth,
                    new_death,
                )
            })?;
            person_detail(&db.snapshot()?, &person)
        }

        PersonOp::Delete { id, cascade_events } => {
            let snap = db.snapshot()?;
            let p = resolve_person(&snap, &id)?.clone();
            db.write_txn(|txn| repo::person::delete_with_cascade(txn, &p.handle, cascade_events))?;
            json!({ "deleted": { "handle": p.handle, "gramps_id": p.gramps_id } })
        }
    };
    print_json(&value, compact)
}

fn person_summary(p: &Person) -> Value {
    let birth = p
        .birth_ref_index
        .try_into()
        .ok()
        .and_then(|i: usize| p.event_ref_list.get(i))
        .map(|r| r.r#ref.clone());
    let death = p
        .death_ref_index
        .try_into()
        .ok()
        .and_then(|i: usize| p.event_ref_list.get(i))
        .map(|r| r.r#ref.clone());
    json!({
        "handle": p.handle,
        "gramps_id": p.gramps_id,
        "name": p.primary_name.display(),
        "gender": enums::gender_label(p.gender),
        "birth_event": birth,
        "death_event": death,
    })
}

fn person_detail(snap: &Snapshot, p: &Person) -> Value {
    let resolve_event = |handle: &str| -> Option<Value> {
        let idx = *snap.index.events.get(handle)?;
        let e = snap.events.get(idx)?;
        Some(json!({
            "handle": e.handle,
            "gramps_id": e.gramps_id,
            "type": enums::event_type_label(e.r#type.value).unwrap_or("Custom"),
            "type_value": e.r#type.value,
            "date": e.date.as_ref().map(format_date),
            "description": e.description,
            "place_handle": (!e.place.is_empty()).then(|| e.place.clone()),
        }))
    };

    let events: Vec<Value> = p
        .event_ref_list
        .iter()
        .filter_map(|r| resolve_event(&r.r#ref))
        .collect();

    json!({
        "handle": p.handle,
        "gramps_id": p.gramps_id,
        "name": p.primary_name.display(),
        "first_name": p.primary_name.first_name,
        "surname": p.primary_name.surname_list.first().map(|s| s.surname.clone()),
        "gender": enums::gender_label(p.gender),
        "events": events,
        "family_handles": p.family_list,       // families this person is a parent in
        "parent_family_handles": p.parent_family_list, // families this person is a child in
        "private": p.private,
    })
}

fn read_event_date(snap: &Snapshot, p: &Person, want_type: i32) -> Option<Date> {
    for r in &p.event_ref_list {
        let idx = *snap.index.events.get(&r.r#ref)?;
        let e = snap.events.get(idx)?;
        if e.r#type.value == want_type {
            return e.date.clone();
        }
    }
    None
}

// ---------- family commands ----------

fn family_cmd(db: &Database, op: FamilyOp, compact: bool) -> Result<()> {
    let value = match op {
        FamilyOp::List { limit } => {
            let snap = db.snapshot()?;
            let take = if limit == 0 { snap.families.len() } else { limit };
            let rows: Vec<Value> = snap
                .families
                .iter()
                .take(take)
                .map(|f| {
                    json!({
                        "handle": f.handle,
                        "gramps_id": f.gramps_id,
                        "father_handle": f.father_handle,
                        "mother_handle": f.mother_handle,
                        "child_count": f.child_ref_list.len(),
                        "rel": enums::family_rel_label(f.r#type.value).unwrap_or("Custom"),
                    })
                })
                .collect();
            json!({ "count": snap.families.len(), "families": rows })
        }

        FamilyOp::Get { id } => {
            let snap = db.snapshot()?;
            let idx = resolve_id(&id, |h| snap.index.families.get(h).copied(), |gid| {
                snap.families.iter().position(|f| f.gramps_id == gid)
            })
            .ok_or_else(|| anyhow!("family {id} not found"))?;
            let f = &snap.families[idx];
            let children: Vec<String> = f.child_ref_list.iter().map(|c| c.r#ref.clone()).collect();
            json!({
                "handle": f.handle,
                "gramps_id": f.gramps_id,
                "father_handle": f.father_handle,
                "mother_handle": f.mother_handle,
                "rel": enums::family_rel_label(f.r#type.value).unwrap_or("Custom"),
                "rel_value": f.r#type.value,
                "child_handles": children,
                "event_handles": f.event_ref_list.iter().map(|r| r.r#ref.clone())
                    .collect::<Vec<_>>(),
            })
        }

        FamilyOp::Add { father, mother, rel } => {
            if father.is_none() && mother.is_none() {
                bail!("provide --father or --mother (or both)");
            }
            let snap = db.snapshot()?;
            let father_handle = match father {
                Some(s) => Some(resolve_person(&snap, &s)?.handle.clone()),
                None => None,
            };
            let mother_handle = match mother {
                Some(s) => Some(resolve_person(&snap, &s)?.handle.clone()),
                None => None,
            };
            let fam = db.write_txn(|txn| {
                repo::family::create(txn, father_handle, mother_handle, rel.as_i32())
            })?;
            json!({
                "handle": fam.handle,
                "gramps_id": fam.gramps_id,
            })
        }

        FamilyOp::Update {
            id,
            father,
            mother,
            rel,
        } => {
            let snap = db.snapshot()?;
            let f_idx = resolve_id(&id, |h| snap.index.families.get(h).copied(), |gid| {
                snap.families.iter().position(|f| f.gramps_id == gid)
            })
            .ok_or_else(|| anyhow!("family {id} not found"))?;
            let existing = snap.families[f_idx].clone();

            let resolve_parent = |opt: Option<String>, current: Option<String>| -> Result<Option<String>> {
                match opt {
                    None => Ok(current),
                    Some(s) if s.is_empty() => Ok(None),
                    Some(s) => Ok(Some(resolve_person(&snap, &s)?.handle.clone())),
                }
            };
            let new_father = resolve_parent(father, existing.father_handle.clone())?;
            let new_mother = resolve_parent(mother, existing.mother_handle.clone())?;
            let new_rel = rel.map(|r| r.as_i32()).unwrap_or(existing.r#type.value);

            let fam = db.write_txn(|txn| {
                repo::family::update(txn, &existing.handle, new_father, new_mother, new_rel)
            })?;
            json!({
                "handle": fam.handle,
                "gramps_id": fam.gramps_id,
            })
        }

        FamilyOp::Delete { id } => {
            let snap = db.snapshot()?;
            let f_idx = resolve_id(&id, |h| snap.index.families.get(h).copied(), |gid| {
                snap.families.iter().position(|f| f.gramps_id == gid)
            })
            .ok_or_else(|| anyhow!("family {id} not found"))?;
            let f = snap.families[f_idx].clone();
            db.write_txn(|txn| repo::family::delete_with_cascade(txn, &f.handle))?;
            json!({ "deleted": { "handle": f.handle, "gramps_id": f.gramps_id } })
        }

        FamilyOp::AddChild { family, child } => {
            let snap = db.snapshot()?;
            let f_idx = resolve_id(&family, |h| snap.index.families.get(h).copied(), |gid| {
                snap.families.iter().position(|f| f.gramps_id == gid)
            })
            .ok_or_else(|| anyhow!("family {family} not found"))?;
            let family_handle = snap.families[f_idx].handle.clone();
            let child_handle = resolve_person(&snap, &child)?.handle.clone();
            db.write_txn(|txn| {
                repo::relationships::link_child_to_family(txn, &family_handle, &child_handle)
            })?;
            json!({
                "family_handle": family_handle,
                "child_handle": child_handle,
            })
        }
    };
    print_json(&value, compact)
}

// ---------- event commands ----------

fn event_cmd(db: &Database, op: EventOp, compact: bool) -> Result<()> {
    let value = match op {
        EventOp::List { limit } => {
            let snap = db.snapshot()?;
            let take = if limit == 0 { snap.events.len() } else { limit };
            let rows: Vec<Value> = snap
                .events
                .iter()
                .take(take)
                .map(|e| {
                    json!({
                        "handle": e.handle,
                        "gramps_id": e.gramps_id,
                        "type": enums::event_type_label(e.r#type.value).unwrap_or("Custom"),
                        "date": e.date.as_ref().map(format_date),
                        "description": e.description,
                        "place_handle": (!e.place.is_empty()).then(|| e.place.clone()),
                    })
                })
                .collect();
            json!({ "count": snap.events.len(), "events": rows })
        }

        EventOp::Get { id } => {
            let snap = db.snapshot()?;
            let idx = resolve_id(&id, |h| snap.index.events.get(h).copied(), |gid| {
                snap.events.iter().position(|e| e.gramps_id == gid)
            })
            .ok_or_else(|| anyhow!("event {id} not found"))?;
            let e = &snap.events[idx];
            json!({
                "handle": e.handle,
                "gramps_id": e.gramps_id,
                "type": enums::event_type_label(e.r#type.value).unwrap_or("Custom"),
                "type_value": e.r#type.value,
                "date": e.date.as_ref().map(format_date),
                "description": e.description,
                "place_handle": (!e.place.is_empty()).then(|| e.place.clone()),
            })
        }

        EventOp::Add {
            r#type,
            description,
            date,
            place,
        } => {
            let type_value = parse_event_type(&r#type)?;
            let date = parse_date_opt(date.as_deref())?;
            let place_handle = match place {
                Some(s) => Some(resolve_place(&db.snapshot()?, &s)?),
                None => None,
            };
            let ev = db.write_txn(|txn| {
                repo::event::create_full(txn, type_value, &description, place_handle, date)
            })?;
            json!({
                "handle": ev.handle,
                "gramps_id": ev.gramps_id,
            })
        }

        EventOp::Delete { id } => {
            let snap = db.snapshot()?;
            let idx = resolve_id(&id, |h| snap.index.events.get(h).copied(), |gid| {
                snap.events.iter().position(|e| e.gramps_id == gid)
            })
            .ok_or_else(|| anyhow!("event {id} not found"))?;
            let e = snap.events[idx].clone();
            db.write_txn(|txn| repo::event::delete(txn, &e.handle))?;
            json!({ "deleted": { "handle": e.handle, "gramps_id": e.gramps_id } })
        }
    };
    print_json(&value, compact)
}

// ---------- place commands ----------

fn place_cmd(db: &Database, op: PlaceOp, compact: bool) -> Result<()> {
    let value = match op {
        PlaceOp::List { limit } => {
            let snap = db.snapshot()?;
            let take = if limit == 0 { snap.places.len() } else { limit };
            let rows: Vec<Value> = snap
                .places
                .iter()
                .take(take)
                .map(|p| {
                    json!({
                        "handle": p.handle,
                        "gramps_id": p.gramps_id,
                        "name": p.name.value,
                        "type": enums::place_type_label(p.place_type.value).unwrap_or("Custom"),
                    })
                })
                .collect();
            json!({ "count": snap.places.len(), "places": rows })
        }

        PlaceOp::Get { id } => {
            let snap = db.snapshot()?;
            let idx = resolve_id(&id, |h| snap.index.places.get(h).copied(), |gid| {
                snap.places.iter().position(|p| p.gramps_id == gid)
            })
            .ok_or_else(|| anyhow!("place {id} not found"))?;
            let p = &snap.places[idx];
            json!({
                "handle": p.handle,
                "gramps_id": p.gramps_id,
                "name": p.name.value,
                "type": enums::place_type_label(p.place_type.value).unwrap_or("Custom"),
                "type_value": p.place_type.value,
                "lat": p.lat,
                "long": p.long,
                "parent_place_handles": p.placeref_list.iter()
                    .map(|r| r.r#ref.clone()).collect::<Vec<_>>(),
            })
        }

        PlaceOp::Add {
            name,
            r#type,
            lat,
            long,
            parent,
        } => {
            let type_value = parse_place_type(&r#type)?;
            let parent_handle = match parent {
                Some(s) => Some(resolve_place(&db.snapshot()?, &s)?),
                None => None,
            };
            let place = db.write_txn(|txn| {
                repo::place::create(txn, &name, type_value, &lat, &long, parent_handle)
            })?;
            json!({
                "handle": place.handle,
                "gramps_id": place.gramps_id,
            })
        }

        PlaceOp::Delete { id } => {
            let snap = db.snapshot()?;
            let idx = resolve_id(&id, |h| snap.index.places.get(h).copied(), |gid| {
                snap.places.iter().position(|p| p.gramps_id == gid)
            })
            .ok_or_else(|| anyhow!("place {id} not found"))?;
            let p = snap.places[idx].clone();
            db.write_txn(|txn| repo::place::delete_with_cascade(txn, &p.handle))?;
            json!({ "deleted": { "handle": p.handle, "gramps_id": p.gramps_id } })
        }
    };
    print_json(&value, compact)
}

// ---------- helpers ----------

fn print_json(v: &Value, compact: bool) -> Result<()> {
    let s = if compact {
        serde_json::to_string(v)?
    } else {
        serde_json::to_string_pretty(v)?
    };
    println!("{s}");
    Ok(())
}

/// Resolve a string id (handle or gramps_id) by trying handle-lookup
/// first and falling back to a gramps-id scan.
fn resolve_id<H, G>(id: &str, by_handle: H, by_gramps_id: G) -> Option<usize>
where
    H: Fn(&str) -> Option<usize>,
    G: Fn(&str) -> Option<usize>,
{
    if let Some(i) = by_handle(id) {
        return Some(i);
    }
    by_gramps_id(id)
}

fn resolve_person<'a>(snap: &'a Snapshot, id: &str) -> Result<&'a Person> {
    let idx = resolve_id(id, |h| snap.index.persons.get(h).copied(), |gid| {
        snap.persons.iter().position(|p| p.gramps_id == gid)
    })
    .ok_or_else(|| anyhow!("person {id} not found"))?;
    Ok(&snap.persons[idx])
}

fn resolve_place(snap: &Snapshot, id: &str) -> Result<String> {
    let idx = resolve_id(id, |h| snap.index.places.get(h).copied(), |gid| {
        snap.places.iter().position(|p| p.gramps_id == gid)
    })
    .ok_or_else(|| anyhow!("place {id} not found"))?;
    Ok(snap.places[idx].handle.clone())
}

/// Parse YYYY, YYYY-MM, or YYYY-MM-DD into a Gramps Date.
fn parse_date_opt(s: Option<&str>) -> Result<Option<Date>> {
    let Some(s) = s.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let parts: Vec<&str> = s.split('-').collect();
    let nums: Vec<i32> = parts
        .iter()
        .map(|p| {
            p.parse::<i32>()
                .map_err(|_| anyhow!("date {s:?} must be digits separated by '-'"))
        })
        .collect::<Result<_>>()?;
    let (year, month, day) = match nums.as_slice() {
        [y] => (*y, 0, 0),
        [y, m] => (*y, *m, 0),
        [y, m, d] => (*y, *m, *d),
        _ => bail!("date {s:?} must be YYYY, YYYY-MM, or YYYY-MM-DD"),
    };
    if year == 0 {
        bail!("date {s:?} has zero year");
    }
    Ok(Some(Date {
        class: Some("Date".to_string()),
        calendar: 0,
        modifier: 0,
        quality: 0,
        dateval: Some(DateVal::Simple(day, month, year, false)),
        text: String::new(),
        sortval: 0,
        newyear: 0,
        format: None,
        year: Some(year),
    }))
}

fn format_date(d: &Date) -> Value {
    let (day, month, year) = match &d.dateval {
        Some(DateVal::Simple(d, m, y, _)) => (*d, *m, *y),
        Some(DateVal::Range(d, m, y, _, _, _, _, _)) => (*d, *m, *y),
        None => (0, 0, d.year.unwrap_or(0)),
    };
    let iso = match (year, month, day) {
        (0, _, _) => None,
        (y, 0, _) => Some(format!("{y:04}")),
        (y, m, 0) => Some(format!("{y:04}-{m:02}")),
        (y, m, d) => Some(format!("{y:04}-{m:02}-{d:02}")),
    };
    json!({
        "iso": iso,
        "year": year,
        "month": month,
        "day": day,
        "text": d.text,
    })
}

/// Accept either "Birth", "birth", or "12".
fn parse_event_type(s: &str) -> Result<i32> {
    if let Ok(n) = s.parse::<i32>() {
        return Ok(n);
    }
    let needle = s.to_lowercase();
    for v in -1..=53 {
        if let Some(label) = enums::event_type_label(v) {
            if label.to_lowercase() == needle {
                return Ok(v);
            }
        }
    }
    bail!("unknown event type {s:?}; pass an integer or label like 'Birth'")
}

fn parse_place_type(s: &str) -> Result<i32> {
    if let Ok(n) = s.parse::<i32>() {
        return Ok(n);
    }
    let needle = s.to_lowercase();
    for v in -1..=20 {
        if let Some(label) = enums::place_type_label(v) {
            if label.to_lowercase() == needle {
                return Ok(v);
            }
        }
    }
    bail!("unknown place type {s:?}; pass an integer or label like 'City'")
}
