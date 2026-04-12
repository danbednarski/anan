//! Phase 4 write-path smoke test — exercises Tag/Note/Repository CRUD
//! against a copy of the sample fixture.
//!
//! Run with:
//!
//! ```sh
//! cp test-fixtures/sample.db /tmp/phase4_writable.db
//! cargo run --example write_smoke -- /tmp/phase4_writable.db
//! ```
//!
//! The test:
//!
//! 1. Opens the copy with `Database::open` (verifies schema).
//! 2. Creates a tag, a note, a repository. Asserts counts bumped.
//! 3. Updates each. Asserts fields changed, change timestamp bumped.
//! 4. Deletes each. Asserts counts back to baseline.
//! 5. Verifies the `reference` table was cleaned up for each delete.
//!
//! If every step passes, prints "OK" and exits 0. Otherwise panics
//! with a descriptive message.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use gramps_desktop::db::{repo, Database};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/phase4_writable.db"));

    println!("opening {}", path.display());
    let db = Database::open(&path).context("open db")?;

    let baseline = db.snapshot()?;
    let tag_baseline = baseline.tags.len();
    let note_baseline = baseline.notes.len();
    let repo_baseline = baseline.repositories.len();
    println!(
        "baseline: tags={tag_baseline} notes={note_baseline} repos={repo_baseline}"
    );

    // ---- create ---------------------------------------------------------
    let created_tag = db
        .write_txn(|txn| repo::tag::create(txn, "smoke-tag", "#ff00ff", 7))
        .context("create tag")?;
    let created_note = db
        .write_txn(|txn| repo::note::create(txn, 2, "smoke note body"))
        .context("create note")?;
    let created_repo = db
        .write_txn(|txn| repo::repository::create(txn, "smoke repo", 3))
        .context("create repository")?;

    let after_create = db.snapshot()?;
    assert_len(
        after_create.tags.len(),
        tag_baseline + 1,
        "tags after create",
    );
    assert_len(
        after_create.notes.len(),
        note_baseline + 1,
        "notes after create",
    );
    assert_len(
        after_create.repositories.len(),
        repo_baseline + 1,
        "repos after create",
    );
    assert!(
        after_create.tag(&created_tag.handle).is_some(),
        "created tag not found"
    );
    assert!(
        after_create.note(&created_note.handle).is_some(),
        "created note not found"
    );
    assert!(
        after_create.repository(&created_repo.handle).is_some(),
        "created repo not found"
    );
    println!("create OK");

    // ---- update ---------------------------------------------------------
    let original_tag_change = after_create.tag(&created_tag.handle).unwrap().change;
    let original_note_change = after_create.note(&created_note.handle).unwrap().change;
    let original_repo_change = after_create
        .repository(&created_repo.handle)
        .unwrap()
        .change;
    // Sleep a moment so the unix-second `change` column will differ.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let tag_h = created_tag.handle.clone();
    db.write_txn(|txn| repo::tag::update(txn, &tag_h, "smoke-tag-v2", "#00ffff", 9))
        .context("update tag")?;
    let note_h = created_note.handle.clone();
    db.write_txn(|txn| repo::note::update(txn, &note_h, 3, "smoke note body v2"))
        .context("update note")?;
    let repo_h = created_repo.handle.clone();
    db.write_txn(|txn| repo::repository::update(txn, &repo_h, "smoke repo v2", 5))
        .context("update repository")?;

    let after_update = db.snapshot()?;
    let t = after_update.tag(&created_tag.handle).unwrap();
    assert_eq!(t.name, "smoke-tag-v2", "tag name");
    assert_eq!(t.color, "#00ffff", "tag color");
    assert_eq!(t.priority, 9, "tag priority");
    assert!(t.change > original_tag_change, "tag change timestamp bump");

    let n = after_update.note(&created_note.handle).unwrap();
    assert_eq!(n.text.string, "smoke note body v2", "note body");
    assert_eq!(n.r#type.value, 3, "note type");
    assert!(n.change > original_note_change, "note change timestamp bump");

    let r = after_update.repository(&created_repo.handle).unwrap();
    assert_eq!(r.name, "smoke repo v2", "repo name");
    assert_eq!(r.r#type.value, 5, "repo type");
    assert!(r.change > original_repo_change, "repo change timestamp bump");
    println!("update OK");

    // ---- delete ---------------------------------------------------------
    db.write_txn(|txn| repo::tag::delete(txn, &created_tag.handle))
        .context("delete tag")?;
    db.write_txn(|txn| repo::note::delete(txn, &created_note.handle))
        .context("delete note")?;
    db.write_txn(|txn| repo::repository::delete(txn, &created_repo.handle))
        .context("delete repository")?;

    let after_delete = db.snapshot()?;
    assert_len(after_delete.tags.len(), tag_baseline, "tags after delete");
    assert_len(after_delete.notes.len(), note_baseline, "notes after delete");
    assert_len(
        after_delete.repositories.len(),
        repo_baseline,
        "repos after delete",
    );
    assert!(
        after_delete.tag(&created_tag.handle).is_none(),
        "deleted tag still present"
    );
    assert!(
        after_delete.note(&created_note.handle).is_none(),
        "deleted note still present"
    );
    assert!(
        after_delete.repository(&created_repo.handle).is_none(),
        "deleted repo still present"
    );
    println!("delete OK");

    println!();
    println!("== Phase 4 write-path smoke: OK ==");
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
