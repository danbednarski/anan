//! Persistent read-write access to a Gramps tree.
//!
//! [`Database::open`] establishes a single long-lived [`Connection`]
//! and guards it behind a [`Mutex`]. The app holds the result in an
//! `Arc<Database>`, clones the Arc into `tokio::spawn_blocking` closures
//! for write operations, and keeps using the same file for the life of
//! the editing session.
//!
//! ## Schema version check
//!
//! On open we read `metadata.version` and refuse to touch anything
//! except Gramps schema `"21"` (Gramps 6.0.x). This is the single
//! version the Rust structs in `gramps::*` were modeled against —
//! parsing older or newer layouts would be silent data loss.
//!
//! ## Backup-before-write
//!
//! Every write transaction first calls [`Database::ensure_backup`],
//! which copies `sqlite.db` to a timestamped `sqlite.db.bak-<unix>`
//! sibling file if at least [`BACKUP_INTERVAL`] has passed since the
//! last backup of this session. Older backups beyond [`BACKUP_KEEP`]
//! are pruned from the same directory.
//!
//! This is a belt-and-braces guard against my own bugs — not a
//! replacement for the user's real backup strategy.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{Connection, OpenFlags, Transaction};

use super::{load_from_conn, Snapshot};

/// Minimum wait between backup snapshots during continuous editing.
pub const BACKUP_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Max number of `sqlite.db.bak-*` files to retain alongside the tree.
pub const BACKUP_KEEP: usize = 10;

/// Gramps schema version this build supports. Anything else is refused
/// on open to keep data-loss bugs loud.
pub const SUPPORTED_SCHEMA: &str = "21";

/// A long-lived read-write handle to a Gramps tree.
pub struct Database {
    path: PathBuf,
    conn: Mutex<Connection>,
    /// Last successful backup this session. `None` means "never yet".
    last_backup: Mutex<Option<SystemTime>>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Database {
    /// Open a Gramps SQLite file for read-write access and verify its
    /// schema version. Fails if the file does not exist, is not a
    /// Gramps tree, or carries a schema version we don't know.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("open {} read-write", path.display()))?;

        verify_schema(&conn)
            .with_context(|| format!("verify schema of {}", path.display()))?;

        Ok(Database {
            path: path.to_path_buf(),
            conn: Mutex::new(conn),
            last_backup: Mutex::new(None),
        })
    }

    /// Absolute path of the underlying SQLite file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Re-read every primary table into a fresh [`Snapshot`]. Call after
    /// every successful write so the UI sees the latest state.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let conn = self.conn.lock().map_err(|_| anyhow!("connection mutex poisoned"))?;
        load_from_conn(&conn, self.path.clone())
    }

    /// Run `f` with a borrowed read-only view of the connection. No
    /// transaction, no backup-before-write. Use this for cheap
    /// read-side queries like the delete-cascade preview — write
    /// operations should still go through [`Database::write_txn`].
    pub fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> Result<R>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("connection mutex poisoned"))?;
        f(&conn)
    }

    /// Run `f` inside a SQLite transaction. Commits on `Ok`, rolls back
    /// on `Err`. A backup is written first if stale.
    ///
    /// The closure receives a borrowed [`Transaction`] and must do all
    /// its DB work through it — not through the outer `self.conn`.
    pub fn write_txn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Transaction) -> Result<R>,
    {
        self.ensure_backup()
            .context("backup-before-write")?;

        let mut conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("connection mutex poisoned"))?;
        let txn = conn.transaction().context("begin txn")?;

        match f(&txn) {
            Ok(value) => {
                txn.commit().context("commit txn")?;
                Ok(value)
            }
            Err(err) => {
                // Rollback is best-effort; surface the original error.
                let _ = txn.rollback();
                Err(err)
            }
        }
    }

    /// Ensure a backup of `self.path` exists that is newer than
    /// [`BACKUP_INTERVAL`]. Called at the start of every write txn.
    fn ensure_backup(&self) -> Result<()> {
        let mut last = self
            .last_backup
            .lock()
            .map_err(|_| anyhow!("backup mutex poisoned"))?;
        let now = SystemTime::now();
        let need_backup = match *last {
            None => true,
            Some(prev) => now
                .duration_since(prev)
                .map(|d| d >= BACKUP_INTERVAL)
                .unwrap_or(true),
        };
        if !need_backup {
            return Ok(());
        }

        let stamp = now
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = self
            .path
            .file_name()
            .ok_or_else(|| anyhow!("no filename in path {}", self.path.display()))?
            .to_string_lossy()
            .into_owned();
        let backup = self.path.with_file_name(format!("{filename}.bak-{stamp}"));

        std::fs::copy(&self.path, &backup)
            .with_context(|| format!("copy {} → {}", self.path.display(), backup.display()))?;
        tracing::info!(
            src = %self.path.display(),
            backup = %backup.display(),
            "wrote backup"
        );

        *last = Some(now);
        drop(last);

        prune_old_backups(&self.path)?;
        Ok(())
    }
}

/// Delete backup files beyond [`BACKUP_KEEP`] in the same directory as
/// `tree`. Oldest-first removal. Best-effort: logs but does not fail
/// the caller if a single file can't be removed.
fn prune_old_backups(tree: &Path) -> Result<()> {
    let Some(dir) = tree.parent() else { return Ok(()); };
    let Some(basename) = tree.file_name().map(|s| s.to_string_lossy().into_owned()) else {
        return Ok(());
    };
    let prefix = format!("{basename}.bak-");

    let mut backups: Vec<(u64, PathBuf)> = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(rest) = name.strip_prefix(&prefix) {
            if let Ok(ts) = rest.parse::<u64>() {
                backups.push((ts, entry.path()));
            }
        }
    }
    if backups.len() <= BACKUP_KEEP {
        return Ok(());
    }
    backups.sort_by_key(|(ts, _)| *ts);
    let excess = backups.len() - BACKUP_KEEP;
    for (_, path) in backups.into_iter().take(excess) {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), error = %e, "prune failed");
        }
    }
    Ok(())
}

/// Read `metadata.version` and compare to [`SUPPORTED_SCHEMA`]. Any
/// mismatch is a hard error — we'd rather refuse the file than silently
/// write a shape Gramps can't load.
fn verify_schema(conn: &Connection) -> Result<()> {
    let row: String = conn
        .query_row(
            "SELECT json_data FROM metadata WHERE setting = 'version'",
            [],
            |r| r.get(0),
        )
        .context("read metadata.version")?;
    let parsed: serde_json::Value =
        serde_json::from_str(&row).context("parse metadata.version json")?;
    let value = parsed
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("metadata.version missing 'value' field"))?;
    if value != SUPPORTED_SCHEMA {
        bail!(
            "unsupported Gramps schema version {value:?} (this build supports {SUPPORTED_SCHEMA:?} = Gramps 6.0.x)"
        );
    }
    Ok(())
}
