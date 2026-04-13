//! Phase 5 cascade-delete smoke test against a person who actually
//! has family ties in the sample fixture.
//!
//! I0000 (Maxwell Zachary Hansen) in sample.db is in one "parent of"
//! family (F0000) and one "child of" family (per sample fields). We
//! delete him and verify both families get rewritten.

use std::path::PathBuf;

use anyhow::{Context, Result};
use gramps_desktop::db::{repo, Database};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/cascade_smoke.db"));

    let db = Database::open(&path).context("open db")?;
    let baseline = db.snapshot()?;

    // Find I0000 — the first person with a non-empty family_list.
    let target = baseline
        .persons
        .iter()
        .find(|p| !p.family_list.is_empty() || !p.parent_family_list.is_empty())
        .expect("no person with family ties in fixture");
    let handle = target.handle.clone();
    let name = target.primary_name.display();
    let family_list = target.family_list.clone();
    let parent_family_list = target.parent_family_list.clone();

    println!(
        "target: {name} {}  parent_of={:?} child_of={:?}",
        target.gramps_id, family_list, parent_family_list
    );

    // Preview.
    let preview = db
        .write_txn(|txn| repo::person::preview_delete(txn, &handle))
        .context("preview delete")?;
    println!(
        "preview: parent_of={:?} child_of={:?} events={} exclusive={}",
        preview.parent_of, preview.child_of, preview.event_count, preview.exclusive_event_count
    );
    assert!(
        !preview.parent_of.is_empty() || !preview.child_of.is_empty(),
        "expected cascade to hit at least one family"
    );

    // Count references to the target before delete.
    let before_refs_to_target: i64 = db
        .write_txn(|txn| {
            repo::common::inbound_ref_count(txn, &handle)
        })
        .context("count inbound refs pre-delete")?;
    println!("inbound refs before delete: {before_refs_to_target}");
    assert!(
        before_refs_to_target > 0,
        "expected some inbound refs pointing at the person"
    );

    // Delete with cascade, preserving events (so we can check the
    // family rewrite path in isolation — events stay, just disowned).
    db.write_txn(|txn| repo::person::delete_with_cascade(txn, &handle, false))
        .context("cascade delete")?;

    let after = db.snapshot()?;

    assert!(
        after.person(&handle).is_none(),
        "person still present after delete"
    );

    // For every family the person was "parent of", the father/mother
    // handle should no longer equal the deleted person handle.
    for fh in &family_list {
        if let Some(f) = after.family(fh) {
            assert_ne!(
                f.father_handle.as_deref(),
                Some(handle.as_str()),
                "family {fh} still has deleted father"
            );
            assert_ne!(
                f.mother_handle.as_deref(),
                Some(handle.as_str()),
                "family {fh} still has deleted mother"
            );
        }
    }
    // For every family the person was "child of", they should no
    // longer appear in child_ref_list.
    for fh in &parent_family_list {
        if let Some(f) = after.family(fh) {
            assert!(
                f.child_ref_list.iter().all(|cr| cr.r#ref != handle),
                "family {fh} still has deleted child"
            );
        }
    }

    // And reference table should have no rows pointing at the deleted
    // handle — either as obj (outbound refs cleared) or as ref
    // (families cleaned up).
    let after_refs: i64 = db
        .write_txn(|txn| repo::common::inbound_ref_count(txn, &handle))
        .context("count inbound refs post-delete")?;
    assert_eq!(
        after_refs, 0,
        "reference table still has {after_refs} rows pointing at deleted person"
    );

    println!("cascade OK");
    println!();
    println!("== Phase 5 cascade smoke: OK ==");
    Ok(())
}
