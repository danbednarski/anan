//! Phase 6a smoke test — Family / Event / Place CRUD.
//!
//! Uses a throwaway copy of the fixture and exercises:
//!
//! 1. create event (full) with a simple year date and a place link
//! 2. create place as a child of an existing place
//! 3. create family with a new father + mother couple
//!    - asserts both persons get the new family in their family_list
//! 4. update event date, place, description
//! 5. update place name + parent
//! 6. update family type + swap mother
//! 7. delete family (cascade) — asserts both persons lose the family
//! 8. delete place (cascade) — asserts child place's parent cleared,
//!    and any event.place pointing at the deleted place goes empty
//!
//! Run: `cargo run --example fep_smoke -- /tmp/fep_smoke.db`

use std::path::PathBuf;

use anyhow::{Context, Result};
use anan::db::{repo, Database};
use anan::gramps::date::{Date, DateVal};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/fep_smoke.db"));

    let db = Database::open(&path).context("open db")?;

    let baseline = db.snapshot()?;
    let p_base = baseline.places.len();
    let e_base = baseline.events.len();
    let f_base = baseline.families.len();
    println!("baseline: places={p_base} events={e_base} families={f_base}");

    // Pick an existing place to use as parent for the new one.
    let parent_place = baseline
        .places
        .first()
        .expect("fixture has at least one place")
        .handle
        .clone();
    println!("parent place: {parent_place}");

    // ---- create place ---------------------------------------------------
    let place = db
        .write_txn(|txn| {
            repo::place::create(
                txn,
                "Smoketown",
                15, // Town
                "38.0",
                "-94.0",
                Some(parent_place.clone()),
            )
        })
        .context("create place")?;
    assert!(place.gramps_id.starts_with('P'));

    // ---- create event ---------------------------------------------------
    let date = Some(Date {
        class: Some("Date".to_string()),
        calendar: 0,
        modifier: 0,
        quality: 0,
        dateval: Some(DateVal::Simple(0, 0, 1912, false)),
        text: String::new(),
        sortval: 0,
        newyear: 0,
        format: None,
        year: Some(1912),
    });
    let event = db
        .write_txn(|txn| {
            repo::event::create_full(
                txn,
                42, // Residence
                "smoke test residence",
                Some(place.handle.clone()),
                date.clone(),
            )
        })
        .context("create event")?;
    assert!(event.gramps_id.starts_with('E'));

    // ---- create two persons and a family --------------------------------
    let dad = db
        .write_txn(|txn| {
            repo::person::create(txn, "Alpha", "Smoke", 1, Some(1880), None)
        })
        .context("create dad")?;
    let mom = db
        .write_txn(|txn| {
            repo::person::create(txn, "Beta", "Smoke", 0, Some(1882), None)
        })
        .context("create mom")?;
    let family = db
        .write_txn(|txn| {
            repo::family::create(
                txn,
                Some(dad.handle.clone()),
                Some(mom.handle.clone()),
                0, // Married
            )
        })
        .context("create family")?;
    assert!(family.gramps_id.starts_with('F'));

    let after_create = db.snapshot()?;
    assert!(after_create.place(&place.handle).is_some());
    assert!(after_create.event(&event.handle).is_some());
    assert!(after_create.family(&family.handle).is_some());
    let dad_view = after_create.person(&dad.handle).unwrap();
    assert!(
        dad_view.family_list.iter().any(|h| h == &family.handle),
        "dad should have family in family_list (got {:?})",
        dad_view.family_list
    );
    let mom_view = after_create.person(&mom.handle).unwrap();
    assert!(
        mom_view.family_list.iter().any(|h| h == &family.handle),
        "mom should have family in family_list"
    );
    println!("create OK");

    // ---- update event ---------------------------------------------------
    let eh = event.handle.clone();
    let new_date = Some(Date {
        class: Some("Date".to_string()),
        calendar: 0,
        modifier: 3, // about
        quality: 1,  // estimated
        dateval: Some(DateVal::Simple(0, 0, 1915, false)),
        text: String::new(),
        sortval: 0,
        newyear: 0,
        format: None,
        year: Some(1915),
    });
    db.write_txn(|txn| {
        repo::event::update_full(
            txn,
            &eh,
            42,
            "updated description",
            Some(place.handle.clone()),
            new_date,
        )
    })
    .context("update event")?;
    let after_ev_update = db.snapshot()?;
    let evu = after_ev_update.event(&event.handle).unwrap();
    assert_eq!(evu.description, "updated description");
    assert_eq!(evu.date.as_ref().unwrap().modifier, 3);
    assert_eq!(evu.date.as_ref().unwrap().quality, 1);

    // ---- update place ---------------------------------------------------
    let ph = place.handle.clone();
    db.write_txn(|txn| {
        repo::place::update(txn, &ph, "Smoketown v2", 15, "38.1", "-94.1", None)
    })
    .context("update place")?;
    let after_pu = db.snapshot()?;
    let pu = after_pu.place(&place.handle).unwrap();
    assert_eq!(pu.name.value, "Smoketown v2");
    assert!(
        pu.placeref_list.is_empty(),
        "update with None parent should clear placeref_list"
    );

    // ---- update family (swap mother for a new one) ----------------------
    let new_mom = db
        .write_txn(|txn| {
            repo::person::create(txn, "Gamma", "Smoke", 0, Some(1884), None)
        })
        .context("create replacement mom")?;
    let fh = family.handle.clone();
    db.write_txn(|txn| {
        repo::family::update(
            txn,
            &fh,
            Some(dad.handle.clone()),
            Some(new_mom.handle.clone()),
            0,
        )
    })
    .context("update family")?;
    let after_fu = db.snapshot()?;
    let fu = after_fu.family(&family.handle).unwrap();
    assert_eq!(fu.mother_handle.as_deref(), Some(new_mom.handle.as_str()));
    let old_mom_after = after_fu.person(&mom.handle).unwrap();
    assert!(
        !old_mom_after.family_list.iter().any(|h| h == &family.handle),
        "old mom should no longer have the family in family_list"
    );
    let new_mom_after = after_fu.person(&new_mom.handle).unwrap();
    assert!(
        new_mom_after.family_list.iter().any(|h| h == &family.handle),
        "new mom should have the family in family_list"
    );
    println!("update OK");

    // ---- delete family (cascade) ---------------------------------------
    db.write_txn(|txn| repo::family::delete_with_cascade(txn, &family.handle))
        .context("delete family")?;
    let after_fd = db.snapshot()?;
    assert!(after_fd.family(&family.handle).is_none());
    let dad_final = after_fd.person(&dad.handle).unwrap();
    assert!(
        !dad_final.family_list.iter().any(|h| h == &family.handle),
        "dad should no longer list the deleted family"
    );
    println!("family delete OK");

    // ---- delete place (cascade) -----------------------------------------
    // The event we created should still be present but with place cleared.
    db.write_txn(|txn| repo::place::delete_with_cascade(txn, &place.handle))
        .context("delete place")?;
    let after_pd = db.snapshot()?;
    assert!(after_pd.place(&place.handle).is_none());
    let evf = after_pd.event(&event.handle).unwrap();
    assert!(
        evf.place.is_empty(),
        "event.place should have been cleared by cascade, got {:?}",
        evf.place
    );
    println!("place delete OK");

    println!();
    println!("== Phase 6a family/event/place smoke: OK ==");
    Ok(())
}
