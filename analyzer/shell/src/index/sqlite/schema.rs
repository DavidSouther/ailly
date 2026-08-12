//! SQLite schema and rebuild-on-mismatch lifecycle.

use rusqlite::{Connection, OptionalExtension};

pub const SCHEMA_VERSION: i64 = 3;

pub fn open_connection(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    ensure_schema(&conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    // `meta.value` is a TEXT column, so the stored version is read as text and
    // parsed. An unparseable value counts as a mismatch and triggers a rebuild.
    let stored = match conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
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
    let version = stored.as_deref().and_then(|text| text.parse::<i64>().ok());

    if version != Some(SCHEMA_VERSION) {
        if stored.is_some() {
            drop_tables(conn)?;
        }
        create_schema(conn)?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            [SCHEMA_VERSION.to_string()],
        )?;
    }
    Ok(())
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
