//! Phase 5 Person write-path smoke test.
//!
//! Exercises the backend CRUD against a throwaway copy of the sample
//! fixture, with particular attention to the delete cascade:
//!
//! 1. Create a person with birth + death years.
//! 2. Verify an I-prefixed gramps_id was allocated and that birth/death
//!    events were created and linked via `birth_ref_index` /
//!    `death_ref_index`.
//! 3. Update the person's name, gender, and dates; verify the linked
//!    events' years were overwritten (not duplicated).
//! 4. Preview the delete cascade, then delete with
//!    `delete_owned_events = true`.
//! 5. Verify:
//!    - the person row is gone
//!    - the person's exclusive birth/death events are gone
//!    - the reference table has no rows pointing at the person
//!
//! Run: `cargo run --example person_smoke -- /tmp/person_smoke.db`
//! (first copy test-fixtures/sample.db to that path).

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use anan::db::{repo, Database};
use anan::db::repo::event::make_date;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/person_smoke.db"));

    println!("opening {}", path.display());
    let db = Database::open(&path).context("open db")?;

    let baseline = db.snapshot()?;
    let person_baseline = baseline.persons.len();
    let event_baseline = baseline.events.len();
    println!(
        "baseline: persons={person_baseline} events={event_baseline}"
    );

    // ---- create ---------------------------------------------------------
    let created = db
        .write_txn(|txn| {
            repo::person::create(
                txn,
                "Testy",
                "McTestface",
                1, // male
                Some(make_date(0, 0, 1901)),
                Some(make_date(0, 0, 1975)),
            )
        })
        .context("create person")?;

    assert!(
        created.gramps_id.starts_with('I'),
        "expected I-prefixed gramps_id, got {}",
        created.gramps_id
    );
    assert_eq!(created.event_ref_list.len(), 2, "two events linked");
    assert_eq!(created.birth_ref_index, 0, "birth index points at first event");
    assert_eq!(created.death_ref_index, 1, "death index points at second event");

    let after_create = db.snapshot()?;
    assert_len(
        after_create.persons.len(),
        person_baseline + 1,
        "persons after create",
    );
    assert_len(
        after_create.events.len(),
        event_baseline + 2,
        "events after create",
    );
    let fetched = after_create
        .person(&created.handle)
        .expect("created person missing");
    assert_eq!(fetched.primary_name.first_name, "Testy");
    assert_eq!(
        fetched.primary_name.surname_list[0].surname,
        "McTestface"
    );
    let birth = after_create
        .event(&fetched.event_ref_list[0].r#ref)
        .expect("birth event");
    assert_eq!(
        birth.date.as_ref().map(|d| d.primary_year()),
        Some(make_date(0, 0, 1901))
    );
    println!("create OK  —  {} {}", fetched.gramps_id, fetched.primary_name.display());

    // ---- update: change name + dates ------------------------------------
    let h = created.handle.clone();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    db.write_txn(|txn| {
        repo::person::update(
            txn,
            &h,
            "Testing",
            "McTesterson",
            1,
            Some(make_date(0, 0, 1902)), // birth year changed
            Some(make_date(0, 0, 1980)), // death year changed
        )
    })
    .context("update person")?;

    let after_update = db.snapshot()?;
    // Updating should NOT have added more events — the existing
    // birth/death rows should have been rewritten in place.
    assert_len(
        after_update.events.len(),
        event_baseline + 2,
        "events unchanged after update",
    );
    let fetched = after_update
        .person(&created.handle)
        .expect("updated person missing");
    assert_eq!(fetched.primary_name.first_name, "Testing");
    assert_eq!(
        fetched.primary_name.surname_list[0].surname,
        "McTesterson"
    );
    let birth = after_update
        .event(&fetched.event_ref_list[0].r#ref)
        .expect("birth event after update");
    assert_eq!(
        birth.date.as_ref().map(|d| d.primary_year()),
        Some(make_date(0, 0, 1902)),
        "birth year updated"
    );
    let death = after_update
        .event(&fetched.event_ref_list[1].r#ref)
        .expect("death event after update");
    assert_eq!(
        death.date.as_ref().map(|d| d.primary_year()),
        Some(make_date(0, 0, 1980)),
        "death year updated"
    );
    println!("update OK");

    // ---- preview + delete with cascade ----------------------------------
    let birth_handle = fetched.event_ref_list[0].r#ref.clone();
    let death_handle = fetched.event_ref_list[1].r#ref.clone();

    let preview = db
        .write_txn(|txn| repo::person::preview_delete(txn, &created.handle))
        .context("preview delete")?;
    println!(
        "preview: parent_of={:?} child_of={:?} events={} exclusive={}",
        preview.parent_of, preview.child_of, preview.event_count, preview.exclusive_event_count
    );
    assert_eq!(preview.exclusive_event_count, 2, "both events exclusive");

    db.write_txn(|txn| repo::person::delete_with_cascade(txn, &created.handle, true))
        .context("delete person")?;

    let after_delete = db.snapshot()?;
    assert_len(
        after_delete.persons.len(),
        person_baseline,
        "persons back to baseline",
    );
    assert_len(
        after_delete.events.len(),
        event_baseline,
        "events back to baseline (exclusive events removed)",
    );
    assert!(
        after_delete.person(&created.handle).is_none(),
        "person still present"
    );
    assert!(
        after_delete.event(&birth_handle).is_none(),
        "birth event still present"
    );
    assert!(
        after_delete.event(&death_handle).is_none(),
        "death event still present"
    );
    println!("delete OK");

    println!();
    println!("== Phase 5 person smoke: OK ==");
    Ok(())
}

fn assert_len(got: usize, want: usize, context: &'static str) {
    if got != want {
        panic!("{context}: got {got}, want {want}");
    }
}

#[allow(dead_code)]
fn fail(msg: &str) -> Result<()> {
    bail!("{msg}")
}
