//! SQLite reconcile backend and query surface.

use crate::index::domain::{
    batch_from_parsed, event_kind_name, harness_name, parse_event_kind, parse_harness,
    relationship_kind_name, token_total_from_events, FileIdentity,
};
use crate::index::reconcile::ReconcileBackend;
use crate::index::source_value::{decode, decode_required, encode, IndexCodecError, SourceKind};
use crate::index::{
    EventPage, IndexError, ListSessionsQuery, PageQuery, Paged, SearchHit, SearchQuery,
    SessionListItem, SessionSummary,
};
use crate::model::{Event, Harness, ParsedSession, Provenance, Relationship, Session, SourceValue};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::HashSet;

pub struct SqliteBackend {
    conn: Connection,
}

impl SqliteBackend {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn list_sessions(
        &self,
        query: ListSessionsQuery,
    ) -> Result<Paged<SessionListItem>, IndexError> {
        let mut sql = String::from(
            "SELECT s.id, s.harness, s.project_kind, s.project_value,
                    COUNT(e.id) AS event_count,
                    MAX(CASE WHEN e.timestamp_kind = 'recorded' THEN e.timestamp_value END) AS max_ts
             FROM sessions s
             LEFT JOIN events e ON e.session_id = s.id
             WHERE 1 = 1",
        );
        let mut bind: Vec<String> = Vec::new();
        if let Some(harness) = query.harness {
            sql.push_str(" AND s.harness = ?");
            bind.push(harness_name(harness).to_string());
        }
        if let Some(project) = &query.project {
            sql.push_str(" AND s.project_kind = 'recorded' AND s.project_value = ?");
            bind.push(project.clone());
        }
        sql.push_str(
            " GROUP BY s.id
              ORDER BY (max_ts IS NULL) ASC, max_ts DESC, s.id ASC
              LIMIT ? OFFSET ?",
        );

        let limit = query.limit as i64;
        let offset = query.offset as i64;
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = match bind.len() {
            0 => stmt.query_map(params![limit, offset], map_session_row)?,
            1 => stmt.query_map(params![&bind[0], limit, offset], map_session_row)?,
            2 => stmt.query_map(params![&bind[0], &bind[1], limit, offset], map_session_row)?,
            _ => unreachable!("at most two filter binds"),
        };

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(Paged { items })
    }

    pub fn get_session_summary(&self, session_id: &str) -> Result<SessionSummary, IndexError> {
        let events = self.events_for_session(session_id)?;
        let (token_total, token_recorded_count) = token_total_from_events(&events);
        Ok(SessionSummary {
            token_total,
            token_recorded_count,
            event_count: events.len(),
        })
    }

    pub fn get_event_page(
        &self,
        session_id: &str,
        query: PageQuery,
    ) -> Result<EventPage, IndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id, session_id, source_path, kind,
                prov_harness, prov_path, prov_line, prov_ordinal,
                native_id_kind, native_id_value,
                timestamp_kind, timestamp_value,
                turn_json, tool_call_json,
                token_usage_kind, token_usage_json,
                files_json,
                detail_kind, detail_value,
                tool_result_json
             FROM events
             WHERE session_id = ?1
             ORDER BY prov_ordinal ASC
             LIMIT ?2 OFFSET ?3",
        )?;
        let events = stmt
            .query_map(
                params![session_id, query.limit as i64, query.offset as i64],
                |row| read_event(row).map_err(map_index_error),
            )?
            .collect::<Result<Vec<Event>, _>>()?;
        Ok(EventPage { events })
    }

    pub fn search_index(&self, query: SearchQuery) -> Result<Paged<SearchHit>, IndexError> {
        let fts_query = format!("\"{}\"", query.query.replace('"', "\"\""));
        let mut stmt = self.conn.prepare(
            "SELECT sc.session_id, sc.event_id, snippet(search_fts, 0, '[', ']', '…', 10) AS snippet
             FROM search_fts
             JOIN search_content sc ON sc.rowid = search_fts.rowid
             WHERE search_fts MATCH ?1
             ORDER BY bm25(search_fts)
             LIMIT ?2 OFFSET ?3",
        )?;
        let items = stmt
            .query_map(
                params![fts_query, query.limit as i64, query.offset as i64],
                |row| {
                    Ok(SearchHit {
                        session_id: row.get(0)?,
                        event_id: row.get(1)?,
                        snippet: row.get(2)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Paged { items })
    }

    fn events_for_session(&self, session_id: &str) -> Result<Vec<Event>, IndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id, session_id, source_path, kind,
                prov_harness, prov_path, prov_line, prov_ordinal,
                native_id_kind, native_id_value,
                timestamp_kind, timestamp_value,
                turn_json, tool_call_json,
                token_usage_kind, token_usage_json,
                files_json,
                detail_kind, detail_value,
                tool_result_json
             FROM events WHERE session_id = ?1",
        )?;
        let events = stmt
            .query_map(params![session_id], |row| {
                read_event(row).map_err(map_index_error)
            })?
            .collect::<Result<Vec<Event>, _>>()?;
        Ok(events)
    }
}

impl ReconcileBackend for SqliteBackend {
    fn stored_paths(&self) -> Result<HashSet<String>, IndexError> {
        let paths = self
            .conn
            .prepare("SELECT path FROM source_files")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(paths.into_iter().collect())
    }

    fn is_unchanged(&self, identity: &FileIdentity) -> Result<bool, IndexError> {
        let row = self
            .conn
            .query_row(
                "SELECT mtime_secs, mtime_nanos, size FROM source_files WHERE path = ?1",
                params![identity.path],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        Ok(row == Some((identity.mtime_secs, identity.mtime_nanos, identity.size)))
    }

    fn remove_file(&mut self, path: &str) -> Result<(), IndexError> {
        let tx = self.conn.unchecked_transaction()?;
        delete_file_rows(&tx, path)?;
        tx.execute("DELETE FROM source_files WHERE path = ?1", params![path])?;
        tx.commit()?;
        Ok(())
    }

    fn upsert_file(
        &mut self,
        harness: Harness,
        identity: &FileIdentity,
        parsed: ParsedSession,
    ) -> Result<(), IndexError> {
        let tx = self.conn.unchecked_transaction()?;
        delete_file_rows(&tx, &identity.path)?;
        write_batch(
            &tx,
            harness,
            &identity.path,
            batch_from_parsed(harness, &identity.path, parsed),
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO source_files (path, mtime_secs, mtime_nanos, size)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                identity.path,
                identity.mtime_secs,
                identity.mtime_nanos,
                identity.size
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn map_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionListItem> {
    let harness = parse_harness(&row.get::<_, String>(1)?).expect("stored harness");
    let project_kind = SourceKind::parse(&row.get::<_, String>(2)?).expect("stored project kind");
    let project_value = row.get::<_, Option<String>>(3)?;
    let last_activity = match row.get::<_, Option<String>>(5)? {
        Some(ts) => SourceValue::Recorded(ts),
        None => SourceValue::Absent,
    };
    Ok(SessionListItem {
        id: row.get(0)?,
        harness,
        project: decode(project_kind, project_value.as_deref()),
        event_count: row.get::<_, i64>(4)? as usize,
        token_total: SourceValue::Absent,
        last_activity,
    })
}

fn delete_file_rows(tx: &Transaction<'_>, source_path: &str) -> Result<(), IndexError> {
    tx.execute(
        "DELETE FROM search_content WHERE event_id IN (SELECT id FROM events WHERE source_path = ?1)",
        params![source_path],
    )?;
    tx.execute(
        "DELETE FROM relationships WHERE source_path = ?1",
        params![source_path],
    )?;
    tx.execute(
        "DELETE FROM diagnostics WHERE source_path = ?1",
        params![source_path],
    )?;
    tx.execute(
        "DELETE FROM events WHERE source_path = ?1",
        params![source_path],
    )?;
    tx.execute(
        "DELETE FROM sessions WHERE source_path = ?1",
        params![source_path],
    )?;
    Ok(())
}

fn write_batch(
    tx: &Transaction<'_>,
    harness: Harness,
    source_path: &str,
    batch: crate::index::domain::ParsedBatch,
) -> Result<(), IndexError> {
    for session in batch.sessions.values() {
        insert_session(tx, source_path, session)?;
    }

    let event_ids: HashSet<String> = batch.events.iter().map(|event| event.id.clone()).collect();
    for event in &batch.events {
        insert_event(tx, source_path, event)?;
    }
    for row in &batch.search {
        tx.execute(
            "INSERT INTO search_content (event_id, session_id, text) VALUES (?1, ?2, ?3)",
            params![row.event_id, row.session_id, row.text],
        )?;
    }
    for relationship in batch.relationships {
        insert_relationship(tx, source_path, &relationship, &event_ids)?;
    }
    for diagnostic in batch.diagnostics {
        insert_diagnostic(tx, source_path, &diagnostic)?;
    }
    let _ = harness;
    Ok(())
}

fn insert_session(
    tx: &Transaction<'_>,
    source_path: &str,
    session: &Session,
) -> Result<(), IndexError> {
    let (native_kind, native_value) = encode(&session.native_id);
    let (project_kind, project_value) = encode(&session.project);
    let (parent_kind, parent_value) = encode(&session.parent_session);
    tx.execute(
        "INSERT OR REPLACE INTO sessions (
            id, harness, source_path,
            prov_harness, prov_path, prov_line, prov_ordinal,
            native_id_kind, native_id_value,
            project_kind, project_value,
            parent_session_kind, parent_session_value
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            session.id,
            harness_name(session.harness),
            source_path,
            harness_name(session.source.harness),
            session.source.path,
            session.source.line as i64,
            session.source.ordinal as i64,
            native_kind.as_str(),
            native_value,
            project_kind.as_str(),
            project_value,
            parent_kind.as_str(),
            parent_value,
        ],
    )?;
    Ok(())
}

fn insert_event(tx: &Transaction<'_>, source_path: &str, event: &Event) -> Result<(), IndexError> {
    let (native_kind, native_value) = encode(&event.native_id);
    let (timestamp_kind, timestamp_value) = encode(&event.timestamp);
    let turn_json = match &event.turn {
        SourceValue::Recorded(turn) => Some(serde_json::to_string(turn)?),
        _ => None,
    };
    let tool_call_json = match &event.tool_call {
        SourceValue::Recorded(tool) => Some(serde_json::to_string(tool)?),
        _ => None,
    };
    let tool_result_json = match &event.tool_result {
        SourceValue::Recorded(result) => Some(serde_json::to_string(result)?),
        _ => None,
    };
    let (token_kind, token_json) = encode(&event.token_usage);
    let files_json = match &event.files {
        SourceValue::Recorded(files) => Some(serde_json::to_string(files)?),
        _ => None,
    };
    let (detail_kind, detail_value) = encode(&event.detail);

    tx.execute(
        "INSERT INTO events (
            id, session_id, source_path, kind,
            prov_harness, prov_path, prov_line, prov_ordinal,
            native_id_kind, native_id_value,
            timestamp_kind, timestamp_value,
            turn_json, tool_call_json,
            token_usage_kind, token_usage_json,
            files_json,
            detail_kind, detail_value,
            tool_result_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            event.id,
            event.session_id,
            source_path,
            event_kind_name(event.kind),
            harness_name(event.source.harness),
            event.source.path,
            event.source.line as i64,
            event.source.ordinal as i64,
            native_kind.as_str(),
            native_value,
            timestamp_kind.as_str(),
            timestamp_value,
            turn_json,
            tool_call_json,
            token_kind.as_str(),
            token_json,
            files_json,
            detail_kind.as_str(),
            detail_value,
            tool_result_json,
        ],
    )?;
    Ok(())
}

fn insert_relationship(
    tx: &Transaction<'_>,
    source_path: &str,
    relationship: &Relationship,
    event_ids: &HashSet<String>,
) -> Result<(), IndexError> {
    let resolved = event_ids.contains(&relationship.to_native_id);
    let (resolved_event_id, status) = if resolved {
        (Some(relationship.to_native_id.clone()), "resolved")
    } else {
        (None, "unresolved")
    };
    tx.execute(
        "INSERT INTO relationships (
            source_path, kind, from_event_id, to_native_id,
            resolved_event_id, resolution_status,
            prov_harness, prov_path, prov_line, prov_ordinal
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            source_path,
            relationship_kind_name(relationship.kind.clone()),
            relationship.from_event_id,
            relationship.to_native_id,
            resolved_event_id,
            status,
            harness_name(relationship.source.harness),
            relationship.source.path,
            relationship.source.line as i64,
            relationship.source.ordinal as i64,
        ],
    )?;
    Ok(())
}

fn insert_diagnostic(
    tx: &Transaction<'_>,
    source_path: &str,
    diagnostic: &crate::model::Diagnostic,
) -> Result<(), IndexError> {
    tx.execute(
        "INSERT INTO diagnostics (
            source_path, prov_harness, prov_path, prov_line, prov_ordinal, message
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            source_path,
            harness_name(diagnostic.source.harness),
            diagnostic.source.path,
            diagnostic.source.line as i64,
            diagnostic.source.ordinal as i64,
            diagnostic.message,
        ],
    )?;
    Ok(())
}

fn read_event(row: &rusqlite::Row<'_>) -> Result<Event, IndexError> {
    let harness = parse_harness(row.get::<_, String>(4)?.as_str())
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let native_kind = SourceKind::parse(&row.get::<_, String>(8)?)
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let timestamp_kind = SourceKind::parse(&row.get::<_, String>(10)?)
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let token_kind = SourceKind::parse(&row.get::<_, String>(14)?)
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let detail_kind = SourceKind::parse(&row.get::<_, String>(17)?)
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;

    Ok(Event {
        id: row.get(0)?,
        session_id: row.get(1)?,
        kind: parse_event_kind(&row.get::<_, String>(3)?),
        source: Provenance {
            harness,
            path: row.get(5)?,
            line: row.get::<_, i64>(6)? as usize,
            ordinal: row.get::<_, i64>(7)? as usize,
        },
        native_id: decode_required(native_kind, row.get::<_, Option<String>>(9)?.as_deref())?,
        timestamp: decode_required(timestamp_kind, row.get::<_, Option<String>>(11)?.as_deref())?,
        turn: decode_optional_json(row.get::<_, Option<String>>(12)?)?,
        tool_call: decode_optional_json(row.get::<_, Option<String>>(13)?)?,
        tool_result: decode_optional_json(row.get::<_, Option<String>>(19)?)?,
        token_usage: decode_required(token_kind, row.get::<_, Option<String>>(15)?.as_deref())?,
        files: decode_optional_json(row.get::<_, Option<String>>(16)?)?,
        detail: decode_required(detail_kind, row.get::<_, Option<String>>(18)?.as_deref())?,
    })
}

fn decode_optional_json<T: for<'de> serde::Deserialize<'de>>(
    json: Option<String>,
) -> Result<SourceValue<T>, IndexError> {
    Ok(match json {
        Some(text) => serde_json::from_str(&text)
            .map(SourceValue::Recorded)
            .unwrap_or(SourceValue::Malformed),
        None => SourceValue::Absent,
    })
}

fn map_index_error(err: IndexError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(err))
}
