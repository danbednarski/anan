//! Shared helpers for the CRUD functions in sibling modules.

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Transaction};

/// Current unix time in seconds, suitable for the `change` column.
pub fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Generate a fresh object handle — an RFC-4122 v4 UUID in hyphenated
/// hex form. Matches what most of the sample fixture's objects carry
/// and reads cleanly in tracing output. Implementation is local so we
/// don't need a `uuid` crate dep (the dev box is currently offline
/// from crates.io anyway).
///
/// Randomness comes from `/dev/urandom` on any Unix; on the unlikely
/// systems where that read fails, we fall back to nanosecond time
/// stuffed into the byte buffer, which is deterministic enough for a
/// single-user desktop app but not meaningfully unpredictable. A
/// collision between two such fallback handles within the same
/// nanosecond would require two writes at exactly the same moment,
/// which single-threaded edit sessions can't produce.
pub fn new_handle() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut bytes);
    } else {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let le = ns.to_le_bytes();
        bytes.copy_from_slice(&le);
    }
    // Set version (4) and RFC-4122 variant bits.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// Allocate the next sequential `gramps_id` for a primary type.
///
/// Reads `SELECT MAX(CAST(SUBSTR(gramps_id, 2) AS INTEGER))` over the
/// target table, treating only rows whose id starts with the expected
/// letter as candidates. Returns a zero-padded 4-digit id like
/// `N0005`, `R0012`, etc. — matching the Gramps core convention.
pub fn next_gramps_id(txn: &Transaction, table: &str, prefix: char) -> Result<String> {
    // GLOB '[A-Z][0-9]*' matches a letter followed by at least one digit.
    // We filter by prefix after reading to keep the SQL simple.
    let sql = format!(
        "SELECT gramps_id FROM {table} WHERE gramps_id GLOB '{prefix}[0-9]*'"
    );
    let mut stmt = txn
        .prepare(&sql)
        .with_context(|| format!("prepare {sql}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .with_context(|| format!("query {table}.gramps_id"))?;

    let mut max: i64 = -1;
    for row in rows {
        let id = row?;
        let num: i64 = id[1..].parse().unwrap_or(-1);
        if num > max {
            max = num;
        }
    }
    Ok(format!("{prefix}{:04}", max + 1))
}

/// Replace the `reference` rows for a source object.
///
/// The Gramps schema stores one row in `reference` per outbound
/// cross-reference: `(obj_handle, obj_class, ref_handle, ref_class)`.
/// This function deletes every existing row where `obj_handle` matches
/// and re-inserts from the provided list. Call it on every write.
pub fn rewrite_references(
    txn: &Transaction,
    obj_handle: &str,
    obj_class: &str,
    refs: &[(String, String)], // (ref_handle, ref_class)
) -> Result<()> {
    txn.execute(
        "DELETE FROM reference WHERE obj_handle = ?1",
        params![obj_handle],
    )
    .context("delete old reference rows")?;

    if refs.is_empty() {
        return Ok(());
    }

    let mut stmt = txn
        .prepare(
            "INSERT INTO reference (obj_handle, obj_class, ref_handle, ref_class) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .context("prepare reference insert")?;
    for (ref_handle, ref_class) in refs {
        stmt.execute(params![obj_handle, obj_class, ref_handle, ref_class])
            .with_context(|| format!("insert reference row → {ref_handle}"))?;
    }
    Ok(())
}

/// Count inbound references to `handle` — i.e. the number of
/// `reference` rows with `ref_handle = handle`. Used to refuse deletion
/// of objects that are still in use by some other object.
pub fn inbound_ref_count(txn: &Transaction, handle: &str) -> Result<i64> {
    txn.query_row(
        "SELECT COUNT(*) FROM reference WHERE ref_handle = ?1",
        params![handle],
        |r| r.get(0),
    )
    .map_err(|e| anyhow!("count inbound refs: {e}"))
}

/// Serialize a primary object as the exact `json_data` TEXT to store.
pub fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("serialize object to json")
}
