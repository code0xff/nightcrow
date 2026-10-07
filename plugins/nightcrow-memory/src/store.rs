//! The shared store: one SQLite file per project, opened by every agent's
//! helper process at once.
//!
//! There is no server in front of it. SQLite already serialises writers across
//! processes, so WAL plus a busy timeout is the whole concurrency story, and a
//! helper that dies takes nothing with it but its own connection.

use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, params};
use std::path::Path;
use std::time::{Duration, Instant};

/// Longest note or board post. A note is a fact to recall, not a document; the
/// cap also bounds what one agent can put in front of another.
pub const MAX_BODY_BYTES: usize = 4 * 1024;
pub const MAX_TAGS: usize = 8;
pub const MAX_TAG_BYTES: usize = 32;
/// Most notes a project keeps. Reaching it refuses the write rather than
/// dropping an old note nobody chose to lose.
pub const MAX_NOTES: i64 = 2000;
/// Most rows one read returns, whatever was asked for.
pub const MAX_RESULTS: usize = 50;
pub const DEFAULT_RESULTS: usize = 10;
pub const DEFAULT_BOARD_TTL_SECS: i64 = 4 * 60 * 60;
pub const MAX_BOARD_TTL_SECS: i64 = 24 * 60 * 60;
/// Search terms read from one query; the rest are ignored.
const MAX_QUERY_TERMS: usize = 16;
/// How long a writer waits for another process's write to finish.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const WAL_RETRY_INTERVAL: Duration = Duration::from_millis(20);
const SCHEMA_VERSION: i64 = 1;

const KIND_NOTE: &str = "note";
const KIND_BOARD: &str = "board";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: i64,
    pub body: String,
    pub tags: Vec<String>,
    pub author: String,
    pub created_at: i64,
    /// Set on board posts only.
    pub expires_at: Option<i64>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            create_private_dir(dir)?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("cannot open the shared memory at {}", path.display()))?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        enter_wal(&conn)?;
        let store = Self { conn };
        store.migrate(path)?;
        Ok(store)
    }

    fn migrate(&self, path: &Path) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        // A newer helper wrote this file. Guessing at its shape could corrupt
        // what the other panes are relying on.
        ensure!(
            version <= SCHEMA_VERSION,
            "{} has schema version {version}, newer than this helper's {SCHEMA_VERSION}; \
             update nightcrow-memory",
            path.display()
        );
        if version == SCHEMA_VERSION {
            return Ok(());
        }
        self.conn.execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE IF NOT EXISTS entries (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 kind TEXT NOT NULL,
                 body TEXT NOT NULL,
                 tags TEXT NOT NULL,
                 author TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 expires_at INTEGER
             );
             CREATE INDEX IF NOT EXISTS entries_kind ON entries (kind, created_at);
             CREATE VIRTUAL TABLE IF NOT EXISTS note_fts USING fts5(body, tags);
             PRAGMA user_version = 1;
             COMMIT;",
        )?;
        Ok(())
    }

    /// Keep a fact for the other agents. Returns its id.
    pub fn write_note(
        &mut self,
        body: &str,
        tags: &[String],
        author: &str,
        now: i64,
    ) -> Result<i64> {
        let body = checked_body(body)?;
        let tags = checked_tags(tags)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM entries WHERE kind = ?1",
            [KIND_NOTE],
            |r| r.get(0),
        )?;
        ensure!(
            count < MAX_NOTES,
            "this project already holds {MAX_NOTES} notes; delete ones that are no longer true \
             (memory_delete) before adding more"
        );
        tx.execute(
            "INSERT INTO entries (kind, body, tags, author, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![KIND_NOTE, body, tags, author, now],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO note_fts (rowid, body, tags) VALUES (?1, ?2, ?3)",
            params![id, body, tags],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Remove a note. False when there is no note with that id.
    pub fn delete_note(&mut self, id: i64) -> Result<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let removed = tx.execute(
            "DELETE FROM entries WHERE id = ?1 AND kind = ?2",
            params![id, KIND_NOTE],
        )?;
        if removed > 0 {
            tx.execute("DELETE FROM note_fts WHERE rowid = ?1", [id])?;
        }
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Notes matching any of the query's words, best match first.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Entry>> {
        let Some(expr) = match_expression(query) else {
            bail!("the search query has no words in it");
        };
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.body, e.tags, e.author, e.created_at, e.expires_at
             FROM note_fts JOIN entries e ON e.id = note_fts.rowid
             WHERE note_fts MATCH ?1
             ORDER BY bm25(note_fts), e.created_at DESC
             LIMIT ?2",
        )?;
        collect(stmt.query_map(params![expr, clamp(limit)], read_entry)?)
    }

    /// The newest notes.
    pub fn recent(&self, limit: usize) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, tags, author, created_at, expires_at FROM entries
             WHERE kind = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
        )?;
        collect(stmt.query_map(params![KIND_NOTE, clamp(limit)], read_entry)?)
    }

    /// Every note, oldest first — for a person reading the whole store.
    pub fn all_notes(&self) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, tags, author, created_at, expires_at FROM entries
             WHERE kind = ?1 ORDER BY created_at, id",
        )?;
        collect(stmt.query_map([KIND_NOTE], read_entry)?)
    }

    /// Say what this author is doing now, replacing what it said before: the
    /// board is who is on what, so one line per author is the whole of it.
    pub fn post_board(&mut self, body: &str, author: &str, now: i64, ttl_secs: i64) -> Result<i64> {
        let body = checked_body(body)?;
        ensure!(
            (1..=MAX_BOARD_TTL_SECS).contains(&ttl_secs),
            "a board post lasts between 1 second and {MAX_BOARD_TTL_SECS} seconds, not {ttl_secs}"
        );
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM entries WHERE kind = ?1 AND (author = ?2 OR expires_at <= ?3)",
            params![KIND_BOARD, author, now],
        )?;
        tx.execute(
            "INSERT INTO entries (kind, body, tags, author, created_at, expires_at)
             VALUES (?1, ?2, '', ?3, ?4, ?5)",
            params![KIND_BOARD, body, author, now, now + ttl_secs],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// What every author last said it is doing, newest first, expired lines
    /// left out.
    pub fn board(&self, now: i64) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, tags, author, created_at, expires_at FROM entries
             WHERE kind = ?1 AND expires_at > ?2 ORDER BY created_at DESC, id DESC",
        )?;
        collect(stmt.query_map(params![KIND_BOARD, now], read_entry)?)
    }

    #[cfg(test)]
    pub(crate) fn note_by_id(&self, id: i64) -> Result<Option<Entry>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row(
                "SELECT id, body, tags, author, created_at, expires_at FROM entries WHERE id = ?1",
                [id],
                read_entry,
            )
            .optional()?)
    }
}

/// Put the file in WAL mode, which lets the other panes keep reading while one
/// writes.
///
/// Retried by hand: changing the journal mode needs the file to itself, and
/// SQLite answers `BUSY` for that at once instead of waiting out the busy
/// timeout. Several agents starting together all arrive here, so without the
/// wait all but the first would fail to open a store that is merely being
/// opened.
fn enter_wal(conn: &Connection) -> Result<()> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        match conn.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(failure, _))
                if failure.code == rusqlite::ErrorCode::DatabaseBusy
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(WAL_RETRY_INTERVAL);
            }
            Err(e) => return Err(e).context("cannot switch the shared memory to WAL mode"),
        }
    }
}

fn clamp(limit: usize) -> i64 {
    limit.clamp(1, MAX_RESULTS) as i64
}

fn read_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    let tags: String = row.get(2)?;
    Ok(Entry {
        id: row.get(0)?,
        body: row.get(1)?,
        tags: tags.split_whitespace().map(str::to_string).collect(),
        author: row.get(3)?,
        created_at: row.get(4)?,
        expires_at: row.get(5)?,
    })
}

fn collect(rows: impl Iterator<Item = rusqlite::Result<Entry>>) -> Result<Vec<Entry>> {
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

#[path = "store_input.rs"]
mod input;
use input::{checked_body, checked_tags, create_private_dir, match_expression};

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
