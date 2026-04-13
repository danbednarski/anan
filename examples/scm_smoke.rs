//! Phase 6b smoke test — Source / Citation / Media CRUD.

use std::path::PathBuf;

use anyhow::{Context, Result};
use anan::db::{repo, Database};
use anan::gramps::date::{Date, DateVal};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/scm_smoke.db"));

    let db = Database::open(&path).context("open db")?;

    let baseline = db.snapshot()?;
    let s_base = baseline.sources.len();
    let c_base = baseline.citations.len();
    let m_base = baseline.media.len();
    println!("baseline: sources={s_base} citations={c_base} media={m_base}");

    // ---- create source ---------------------------------------------------
    let src = db
        .write_txn(|txn| {
            repo::source::create(
                txn,
                "Smoke Town Gazette",
                "Smoke Staff",
                "Smoke Town, 1901",
                "STG",
            )
        })
        .context("create source")?;
    assert!(src.gramps_id.starts_with('S'));
    println!("created source {} ({})", src.gramps_id, src.title);

    // ---- create citation referencing the new source ---------------------
    let citation_date = Some(Date {
        class: Some("Date".to_string()),
        calendar: 0,
        modifier: 0,
        quality: 0,
        dateval: Some(DateVal::Simple(0, 0, 1901, false)),
        text: String::new(),
        sortval: 0,
        newyear: 0,
        format: None,
        year: Some(1901),
    });
    let cit = db
        .write_txn(|txn| {
            repo::citation::create(txn, &src.handle, "p. 4", 3, citation_date.clone())
        })
        .context("create citation")?;
    assert!(cit.gramps_id.starts_with('C'));
    println!("created citation {} → {}", cit.gramps_id, src.gramps_id);

    // ---- create media ---------------------------------------------------
    let media = db
        .write_txn(|txn| {
            repo::media::create(
                txn,
                "/tmp/fake.jpg",
                "image/jpeg",
                "smoke test image",
                None,
            )
        })
        .context("create media")?;
    assert!(media.gramps_id.starts_with('O'));
    println!("created media {}", media.gramps_id);

    // ---- update each ----------------------------------------------------
    let sh = src.handle.clone();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    db.write_txn(|txn| {
        repo::source::update(
            txn,
            &sh,
            "Smoke Town Herald",
            "Smoke Staff",
            "Smoke Town, 1902",
            "STH",
        )
    })
    .context("update source")?;

    let ch = cit.handle.clone();
    db.write_txn(|txn| repo::citation::update(txn, &ch, &src.handle, "p. 12", 4, None))
        .context("update citation")?;

    let mh = media.handle.clone();
    db.write_txn(|txn| {
        repo::media::update(
            txn,
            &mh,
            "/tmp/fake2.jpg",
            "image/jpeg",
            "smoke test image v2",
            None,
        )
    })
    .context("update media")?;

    let after_update = db.snapshot()?;
    assert_eq!(
        after_update.source(&src.handle).unwrap().title,
        "Smoke Town Herald"
    );
    assert_eq!(after_update.citation(&cit.handle).unwrap().page, "p. 12");
    assert_eq!(after_update.citation(&cit.handle).unwrap().confidence, 4);
    assert_eq!(
        after_update.media_item(&media.handle).unwrap().desc,
        "smoke test image v2"
    );
    println!("update OK");

    // ---- delete each (citation first, then source, then media) ----------
    // Source delete should fail while the citation references it.
    let fail = db.write_txn(|txn| repo::source::delete(txn, &src.handle));
    assert!(
        fail.is_err(),
        "expected source delete to refuse while citation references it"
    );
    // Now delete citation, then source.
    db.write_txn(|txn| repo::citation::delete(txn, &cit.handle))
        .context("delete citation")?;
    db.write_txn(|txn| repo::source::delete(txn, &src.handle))
        .context("delete source")?;
    db.write_txn(|txn| repo::media::delete(txn, &media.handle))
        .context("delete media")?;

    let after_delete = db.snapshot()?;
    assert_eq!(after_delete.sources.len(), s_base);
    assert_eq!(after_delete.citations.len(), c_base);
    assert_eq!(after_delete.media.len(), m_base);
    println!("delete OK");

    println!();
    println!("== Phase 6b source/citation/media smoke: OK ==");
    Ok(())
}
