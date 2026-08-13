//! SQLite schema and rebuild-on-mismatch lifecycle.

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

pub const SCHEMA_VERSION: i64 = 4;

pub fn open_connection(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    ensure_schema(&mut conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &mut Connection) -> rusqlite::Result<()> {
    // The rebuild runs in one immediate transaction so a rebuild that fails or
    // races another connection cannot leave the file half-built. A half-built
    // file has tables but no version row, and every later launch then failed on
    // `CREATE TABLE meta` with no way to repair itself.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if stored_version(&tx)? != Some(SCHEMA_VERSION) {
        drop_tables(&tx)?;
        create_schema(&tx)?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
            [SCHEMA_VERSION.to_string()],
        )?;
    }
    tx.commit()
}

/// The usable schema version this file records, if any. A missing `meta` table,
/// a missing row, and a value no schema of ours would have written all read as
/// `None`, which counts as a mismatch and rebuilds.
fn stored_version(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    let stored = match conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, Value>(0),
        )
        .optional()
    {
        Ok(value) => value,
        Err(rusqlite::Error::SqliteFailure(code, _))
            if code.extended_code == rusqlite::ffi::SQLITE_ERROR =>
        {
            None
        }
        Err(err) => return Err(err),
    };
    Ok(match stored {
        // `meta.value` is TEXT, but an integer survives the column's affinity,
        // so accept both rather than rebuilding a current index.
        Some(Value::Text(text)) => text.parse().ok(),
        Some(Value::Integer(version)) => Some(version),
        _ => None,
    })
}

fn drop_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS search_fts;
        DROP TABLE IF EXISTS search_content;
        DROP TABLE IF EXISTS relationships;
        DROP TABLE IF EXISTS diagnostics;
        DROP TABLE IF EXISTS events;
        DROP TABLE IF EXISTS sessions;
        DROP TABLE IF EXISTS source_files;
        DROP TABLE IF EXISTS meta;
        ",
    )
}

fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE source_files (
            path TEXT PRIMARY KEY,
            mtime_secs INTEGER NOT NULL,
            mtime_nanos INTEGER NOT NULL,
            size INTEGER NOT NULL
        );

        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            harness TEXT NOT NULL,
            source_path TEXT NOT NULL,
            prov_harness TEXT NOT NULL,
            prov_path TEXT NOT NULL,
            prov_line INTEGER NOT NULL,
            prov_ordinal INTEGER NOT NULL,
            native_id_kind TEXT NOT NULL,
            native_id_value TEXT,
            project_kind TEXT NOT NULL,
            project_value TEXT,
            parent_session_kind TEXT NOT NULL,
            parent_session_value TEXT
        );

        CREATE TABLE events (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            source_path TEXT NOT NULL,
            kind TEXT NOT NULL,
            prov_harness TEXT NOT NULL,
            prov_path TEXT NOT NULL,
            prov_line INTEGER NOT NULL,
            prov_ordinal INTEGER NOT NULL,
            native_id_kind TEXT NOT NULL,
            native_id_value TEXT,
            timestamp_kind TEXT NOT NULL,
            timestamp_value TEXT,
            turn_json TEXT,
            tool_call_json TEXT,
            tool_result_json TEXT,
            token_usage_kind TEXT NOT NULL,
            token_usage_json TEXT,
            files_json TEXT,
            detail_kind TEXT NOT NULL,
            detail_value TEXT,
            subagent_json TEXT,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        );

        CREATE INDEX idx_events_session_ordinal ON events(session_id, prov_ordinal);
        CREATE INDEX idx_events_source_path ON events(source_path);
        CREATE INDEX idx_sessions_source_path ON sessions(source_path);

        CREATE TABLE relationships (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_path TEXT NOT NULL,
            kind TEXT NOT NULL,
            from_event_id TEXT NOT NULL,
            to_native_id TEXT NOT NULL,
            resolved_event_id TEXT,
            resolution_status TEXT NOT NULL,
            prov_harness TEXT NOT NULL,
            prov_path TEXT NOT NULL,
            prov_line INTEGER NOT NULL,
            prov_ordinal INTEGER NOT NULL
        );

        CREATE TABLE diagnostics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_path TEXT NOT NULL,
            prov_harness TEXT NOT NULL,
            prov_path TEXT NOT NULL,
            prov_line INTEGER NOT NULL,
            prov_ordinal INTEGER NOT NULL,
            message TEXT NOT NULL
        );

        CREATE TABLE search_content (
            rowid INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT NOT NULL UNIQUE,
            session_id TEXT NOT NULL,
            text TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE search_fts USING fts5(
            text,
            content='search_content',
            content_rowid='rowid'
        );

        CREATE TRIGGER search_content_ai AFTER INSERT ON search_content BEGIN
            INSERT INTO search_fts(rowid, text) VALUES (new.rowid, new.text);
        END;
        CREATE TRIGGER search_content_ad AFTER DELETE ON search_content BEGIN
            INSERT INTO search_fts(search_fts, rowid, text) VALUES('delete', old.rowid, old.text);
        END;
        CREATE TRIGGER search_content_au AFTER UPDATE ON search_content BEGIN
            INSERT INTO search_fts(search_fts, rowid, text) VALUES('delete', old.rowid, old.text);
            INSERT INTO search_fts(rowid, text) VALUES (new.rowid, new.text);
        END;
        ",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "ailly-schema-{}-{}-{}.sqlite",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn source_file_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM source_files", [], |row| row.get(0))
            .expect("count source files")
    }

    fn record_a_source_file(conn: &Connection) {
        conn.execute(
            "INSERT INTO source_files (path, mtime_secs, mtime_nanos, size)
             VALUES ('/tmp/session.jsonl', 1, 0, 10)",
            [],
        )
        .expect("record a source file");
    }

    /// The reported failure: a rebuild that did not finish left the file with
    /// every table but no version row, and each later launch reported "table
    /// meta already exists" because it skipped the drop.
    #[test]
    fn rebuilds_an_index_whose_version_row_is_missing() {
        let path = temp_path("missing-version");
        {
            let conn = open_connection(&path).expect("create a new index");
            record_a_source_file(&conn);
            conn.execute("DELETE FROM meta", []).expect("lose the row");
        }

        let reopened = open_connection(&path).expect("reopen an index with no version row");

        assert_eq!(
            stored_version(&reopened).expect("read version"),
            Some(SCHEMA_VERSION)
        );
        assert_eq!(
            source_file_count(&reopened),
            0,
            "an index with no recorded version is rebuilt from its sources"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rebuilds_an_index_left_with_only_some_of_its_tables() {
        let path = temp_path("partial-schema");
        {
            let conn = Connection::open(&path).expect("create a bare file");
            conn.execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
                .expect("create meta alone");
        }

        let reopened = open_connection(&path).expect("reopen a half-built index");

        assert_eq!(
            stored_version(&reopened).expect("read version"),
            Some(SCHEMA_VERSION)
        );
        assert_eq!(source_file_count(&reopened), 0);
        let _ = std::fs::remove_file(&path);
    }

    /// `meta.value` is TEXT, but SQLite stores an integer as an integer, so a
    /// current index written that way must not be discarded as unreadable.
    #[test]
    fn reads_a_version_that_was_stored_as_an_integer() {
        let path = temp_path("integer-version");
        {
            let conn = open_connection(&path).expect("create a new index");
            record_a_source_file(&conn);
            conn.execute(
                "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
                [SCHEMA_VERSION],
            )
            .expect("store the version as an integer");
        }

        let reopened = open_connection(&path).expect("reopen an index versioned by integer");

        assert_eq!(
            source_file_count(&reopened),
            1,
            "a current index keeps its rows rather than rebuilding"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rebuilds_an_index_written_by_an_older_schema() {
        let path = temp_path("older-schema");
        {
            let conn = open_connection(&path).expect("create a new index");
            record_a_source_file(&conn);
            conn.execute(
                "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
                [],
            )
            .expect("age the stored version");
        }

        let reopened = open_connection(&path).expect("reopen an index from an older schema");

        assert_eq!(
            stored_version(&reopened).expect("read version"),
            Some(SCHEMA_VERSION)
        );
        assert_eq!(source_file_count(&reopened), 0);
        let _ = std::fs::remove_file(&path);
    }
}
