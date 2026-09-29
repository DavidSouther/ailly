//! SQLite reconcile backend and query surface.

use crate::index::domain::{
    batch_from_parsed, event_kind_name, harness_name, parse_event_kind, parse_harness,
    relationship_kind_name, resolve_child_session_id, token_total_from_events, FileIdentity,
};
use crate::index::pricing::{session_figures, today_utc_days, FrozenEstimate, SessionTokenFigures};
use crate::index::reconcile::ReconcileBackend;
use crate::index::source_value::{decode, decode_required, encode, IndexCodecError, SourceKind};
use crate::index::{
    EventPage, IndexError, ListSessionsQuery, PageQuery, Paged, SearchHit, SearchQuery,
    SessionListItem, SessionSummary,
};
use crate::model::{Event, Harness, ParsedSession, Provenance, Relationship, Session, SourceValue};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::{HashMap, HashSet};

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
                    MAX(CASE WHEN e.timestamp_kind = 'recorded' THEN e.timestamp_value END) AS max_ts,
                    s.token_total_kind, s.token_total_value,
                    s.recorded_price_micros_kind, s.recorded_price_micros_value,
                    s.estimated_tokens_kind, s.estimated_tokens_value,
                    s.estimated_price_micros_kind, s.estimated_price_micros_value,
                    s.estimated_as_of_kind, s.estimated_as_of_value
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
                tool_result_json,
                subagent_json,
                response_id_kind, response_id_value,
                model_kind, model_value
             FROM events
             WHERE session_id = ?1
             ORDER BY prov_ordinal ASC
             LIMIT ?2 OFFSET ?3",
        )?;
        let mut events = stmt
            .query_map(
                params![session_id, query.limit as i64, query.offset as i64],
                |row| read_event(row).map_err(map_index_error),
            )?
            .collect::<Result<Vec<Event>, _>>()?;
        drop(stmt);
        self.resolve_spawn_children(&mut events)?;
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
                tool_result_json,
                subagent_json,
                response_id_kind, response_id_value,
                model_kind, model_value
             FROM events WHERE session_id = ?1",
        )?;
        let mut events = stmt
            .query_map(params![session_id], |row| {
                read_event(row).map_err(map_index_error)
            })?
            .collect::<Result<Vec<Event>, _>>()?;
        drop(stmt);
        self.resolve_spawn_children(&mut events)?;
        Ok(events)
    }

    /// Closes the parent→child link the adapters could not: an adapter sees one
    /// file, so only the index knows which session answers a spawn's recorded
    /// child id. Runs on every event read, so the Summary tile and the
    /// Subagents tab can never disagree about which spawns link.
    fn resolve_spawn_children(&self, events: &mut [Event]) -> Result<(), IndexError> {
        let resolvable = events.iter().any(|event| {
            matches!(&event.subagent, SourceValue::Recorded(subagent)
                if matches!(subagent.native_id, SourceValue::Recorded(_)))
        });
        if !resolvable {
            return Ok(());
        }
        let sessions = self.all_sessions()?;
        for event in events {
            if let SourceValue::Recorded(subagent) = &mut event.subagent {
                subagent.child_session_id =
                    resolve_child_session_id(&sessions, event.source.harness, &subagent.native_id);
            }
        }
        Ok(())
    }

    fn all_sessions(&self) -> Result<Vec<Session>, IndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id, harness,
                prov_harness, prov_path, prov_line, prov_ordinal,
                native_id_kind, native_id_value,
                project_kind, project_value,
                parent_session_kind, parent_session_value
             FROM sessions",
        )?;
        let sessions = stmt
            .query_map([], |row| read_session(row).map_err(map_index_error))?
            .collect::<Result<Vec<Session>, _>>()?;
        Ok(sessions)
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
        // Read before deleting: the estimate a previous run froze lives on the
        // session rows this reconcile is about to replace.
        let frozen = frozen_estimates(&tx, &identity.path)?;
        delete_file_rows(&tx, &identity.path)?;
        write_batch(
            &tx,
            harness,
            &identity.path,
            batch_from_parsed(harness, &identity.path, parsed),
            &frozen,
            today_utc_days(),
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
        Some(ts) => decode::<String>(SourceKind::Recorded, Some(&ts)),
        None => SourceValue::Absent,
    };
    Ok(SessionListItem {
        id: row.get(0)?,
        harness,
        project: decode(project_kind, project_value.as_deref()),
        event_count: row.get::<_, i64>(4)? as usize,
        token_total: read_figure(row, 6)?,
        recorded_price_micros: read_figure(row, 8)?,
        estimated_tokens: read_figure(row, 10)?,
        estimated_price_micros: read_figure(row, 12)?,
        estimated_as_of: read_figure(row, 14)?,
        last_activity,
    })
}

/// One stored session figure from its adjacent kind/value column pair.
fn read_figure<T: for<'de> serde::Deserialize<'de>>(
    row: &rusqlite::Row<'_>,
    kind_index: usize,
) -> rusqlite::Result<SourceValue<T>> {
    let kind = SourceKind::parse(&row.get::<_, String>(kind_index)?).ok_or(
        rusqlite::Error::InvalidColumnType(
            kind_index,
            "session figure kind".to_string(),
            rusqlite::types::Type::Text,
        ),
    )?;
    Ok(decode(
        kind,
        row.get::<_, Option<String>>(kind_index + 1)?.as_deref(),
    ))
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

/// The estimates a previous index run wrote for the sessions in one source
/// file, keyed by session id.
///
/// A re-index refreshes what the harness recorded but must leave a frozen
/// estimate alone: the catalog is newer than the estimate, and silently
/// restating an old session's dollars at today's rates is the fabrication the
/// age gate exists to prevent.
fn frozen_estimates(
    tx: &Transaction<'_>,
    source_path: &str,
) -> Result<HashMap<String, FrozenEstimate>, IndexError> {
    let mut stmt = tx.prepare(
        "SELECT id,
                estimated_tokens_kind, estimated_tokens_value,
                estimated_price_micros_kind, estimated_price_micros_value,
                estimated_as_of_kind, estimated_as_of_value
         FROM sessions WHERE source_path = ?1",
    )?;
    let rows = stmt.query_map(params![source_path], |row| {
        Ok((
            row.get::<_, String>(0)?,
            FrozenEstimate {
                estimated_tokens: read_figure(row, 1)?,
                estimated_price_micros: read_figure(row, 3)?,
                estimated_as_of: read_figure(row, 5)?,
            },
        ))
    })?;
    let mut frozen = HashMap::new();
    for row in rows {
        let (id, estimate) = row?;
        if estimate.is_written() {
            frozen.insert(id, estimate);
        }
    }
    Ok(frozen)
}

fn write_batch(
    tx: &Transaction<'_>,
    harness: Harness,
    source_path: &str,
    batch: crate::index::domain::ParsedBatch,
    frozen: &HashMap<String, FrozenEstimate>,
    today_days: i64,
) -> Result<(), IndexError> {
    for session in batch.sessions.values() {
        let own_events: Vec<Event> = batch
            .events
            .iter()
            .filter(|event| event.session_id == session.id)
            .cloned()
            .collect();
        let figures = session_figures(&own_events, today_days, frozen.get(&session.id));
        insert_session(tx, source_path, session, &figures)?;
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
    figures: &SessionTokenFigures,
) -> Result<(), IndexError> {
    let (native_kind, native_value) = encode(&session.native_id);
    let (project_kind, project_value) = encode(&session.project);
    let (parent_kind, parent_value) = encode(&session.parent_session);
    let (token_total_kind, token_total_value) = encode(&figures.token_total);
    let (recorded_price_kind, recorded_price_value) = encode(&figures.recorded_price_micros);
    let (estimated_tokens_kind, estimated_tokens_value) = encode(&figures.estimated_tokens);
    let (estimated_price_kind, estimated_price_value) = encode(&figures.estimated_price_micros);
    let (estimated_as_of_kind, estimated_as_of_value) = encode(&figures.estimated_as_of);
    tx.execute(
        "INSERT OR REPLACE INTO sessions (
            id, harness, source_path,
            prov_harness, prov_path, prov_line, prov_ordinal,
            native_id_kind, native_id_value,
            project_kind, project_value,
            parent_session_kind, parent_session_value,
            token_total_kind, token_total_value,
            recorded_price_micros_kind, recorded_price_micros_value,
            estimated_tokens_kind, estimated_tokens_value,
            estimated_price_micros_kind, estimated_price_micros_value,
            estimated_as_of_kind, estimated_as_of_value
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                  ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
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
            token_total_kind.as_str(),
            token_total_value,
            recorded_price_kind.as_str(),
            recorded_price_value,
            estimated_tokens_kind.as_str(),
            estimated_tokens_value,
            estimated_price_kind.as_str(),
            estimated_price_value,
            estimated_as_of_kind.as_str(),
            estimated_as_of_value,
        ],
    )?;
    Ok(())
}

fn insert_event(tx: &Transaction<'_>, source_path: &str, event: &Event) -> Result<(), IndexError> {
    let (native_kind, native_value) = encode(&event.native_id);
    let (response_kind, response_value) = encode(&event.response_id);
    let (model_kind, model_value) = encode(&event.model);
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
    let subagent_json = match &event.subagent {
        SourceValue::Recorded(subagent) => Some(serde_json::to_string(subagent)?),
        _ => None,
    };

    tx.execute(
        "INSERT INTO events (
            id, session_id, source_path, kind,
            prov_harness, prov_path, prov_line, prov_ordinal,
            native_id_kind, native_id_value,
            response_id_kind, response_id_value,
            model_kind, model_value,
            timestamp_kind, timestamp_value,
            turn_json, tool_call_json,
            token_usage_kind, token_usage_json,
            files_json,
            detail_kind, detail_value,
            tool_result_json,
            subagent_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
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
            response_kind.as_str(),
            response_value,
            model_kind.as_str(),
            model_value,
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
            subagent_json,
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

fn read_session(row: &rusqlite::Row<'_>) -> Result<Session, IndexError> {
    let harness = parse_harness(row.get::<_, String>(1)?.as_str())
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let prov_harness = parse_harness(row.get::<_, String>(2)?.as_str())
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let native_kind = SourceKind::parse(&row.get::<_, String>(6)?)
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let project_kind = SourceKind::parse(&row.get::<_, String>(8)?)
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let parent_kind = SourceKind::parse(&row.get::<_, String>(10)?)
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    Ok(Session {
        id: row.get(0)?,
        harness,
        source: Provenance {
            harness: prov_harness,
            path: row.get(3)?,
            line: row.get::<_, i64>(4)? as usize,
            ordinal: row.get::<_, i64>(5)? as usize,
        },
        native_id: decode_required(native_kind, row.get::<_, Option<String>>(7)?.as_deref())?,
        project: decode_required(project_kind, row.get::<_, Option<String>>(9)?.as_deref())?,
        parent_session: decode_required(parent_kind, row.get::<_, Option<String>>(11)?.as_deref())?,
    })
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
    let response_kind = SourceKind::parse(&row.get::<_, String>(21)?)
        .ok_or(IndexCodecError::InvalidRecordedPayload)?;
    let model_kind = SourceKind::parse(&row.get::<_, String>(23)?)
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
        response_id: decode_required(response_kind, row.get::<_, Option<String>>(22)?.as_deref())?,
        model: decode_required(model_kind, row.get::<_, Option<String>>(24)?.as_deref())?,
        timestamp: decode_required(timestamp_kind, row.get::<_, Option<String>>(11)?.as_deref())?,
        turn: decode_optional_json(row.get::<_, Option<String>>(12)?)?,
        tool_call: decode_optional_json(row.get::<_, Option<String>>(13)?)?,
        tool_result: decode_optional_json(row.get::<_, Option<String>>(19)?)?,
        token_usage: decode_required(token_kind, row.get::<_, Option<String>>(15)?.as_deref())?,
        files: decode_optional_json(row.get::<_, Option<String>>(16)?)?,
        detail: decode_required(detail_kind, row.get::<_, Option<String>>(18)?.as_deref())?,
        subagent: decode_optional_json(row.get::<_, Option<String>>(20)?)?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::pricing::iso_date_from_days;
    use crate::model::{EventKind, Subagent, TokenUsage};

    fn temp_backend(label: &str) -> SqliteBackend {
        let path = std::env::temp_dir().join(format!(
            "ailly-sqlite-backend-{}-{}-{}.sqlite",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        SqliteBackend::new(super::super::schema::open_connection(&path).expect("open index"))
    }

    fn provenance(path: &str) -> Provenance {
        Provenance {
            harness: Harness::ClaudeCode,
            path: path.to_string(),
            line: 1,
            ordinal: 1,
        }
    }

    fn spawn_event(source_path: &str, subagent: SourceValue<Subagent>) -> Event {
        Event {
            id: "evt-1".to_string(),
            session_id: "session-1".to_string(),
            kind: EventKind::SubagentSpawn,
            source: provenance(source_path),
            native_id: SourceValue::Absent,
            response_id: SourceValue::Absent,
            model: SourceValue::Absent,
            timestamp: SourceValue::Absent,
            turn: SourceValue::Absent,
            tool_call: SourceValue::Absent,
            tool_result: SourceValue::Absent,
            token_usage: SourceValue::Absent,
            files: SourceValue::Absent,
            detail: SourceValue::Absent,
            subagent,
        }
    }

    fn fully_recorded_subagent() -> Subagent {
        Subagent {
            native_id: SourceValue::Recorded("a3c304e0299ebbabc".to_string()),
            agent_type: SourceValue::Recorded("explore".to_string()),
            prompt: SourceValue::Recorded("Map the parser module".to_string()),
            outcome: SourceValue::Recorded("completed".to_string()),
            nickname: SourceValue::Recorded("Avicenna".to_string()),
            duration_ms: SourceValue::Recorded(467407),
            token_usage: SourceValue::Recorded(TokenUsage {
                input: SourceValue::Recorded(4210),
                output: SourceValue::Recorded(9330),
                cache_read: SourceValue::Recorded(112000),
                cache_write: SourceValue::Recorded(2910),
                total: SourceValue::Recorded(128450),
                cost_total_micros: SourceValue::Absent,
                scope: "subagent".to_string(),
            }),
            child_session_id: SourceValue::Absent,
        }
    }

    /// Stores one event through the reconcile path and reads it back out of the
    /// query path, which is the only way the payload column is exercised
    /// end-to-end.
    fn round_trip(label: &str, event: Event) -> Event {
        let source_path = event.source.path.clone();
        let session_id = event.session_id.clone();
        let mut backend = temp_backend(label);
        backend
            .upsert_file(
                Harness::ClaudeCode,
                &FileIdentity {
                    path: source_path,
                    mtime_secs: 1,
                    mtime_nanos: 0,
                    size: 1,
                },
                ParsedSession {
                    events: vec![event],
                    ..ParsedSession::default()
                },
            )
            .expect("upsert one event");
        let page = backend
            .get_event_page(
                &session_id,
                PageQuery {
                    limit: 10,
                    offset: 0,
                },
            )
            .expect("read the event page");
        page.events.into_iter().next().expect("one stored event")
    }

    #[test]
    fn a_recorded_subagent_payload_round_trips_through_the_index() {
        let event = spawn_event(
            "/tmp/parent.jsonl",
            SourceValue::Recorded(fully_recorded_subagent()),
        );

        let stored = round_trip("subagent-round-trip", event.clone());

        assert_eq!(stored.subagent, event.subagent);
    }

    #[test]
    fn an_event_with_no_recorded_subagent_reads_back_absent_not_malformed() {
        let event = spawn_event("/tmp/parent-absent.jsonl", SourceValue::Absent);

        let stored = round_trip("subagent-absent", event);

        assert_eq!(stored.subagent, SourceValue::Absent);
    }

    /// The payload column is independent of the others, exactly as
    /// `tool_call_json` is: an event carrying only a delegation still stores it.
    #[test]
    fn the_subagent_payload_stores_independently_of_every_other_event_field() {
        let event = spawn_event(
            "/tmp/parent-only-subagent.jsonl",
            SourceValue::Recorded(Subagent {
                outcome: SourceValue::Absent,
                ..fully_recorded_subagent()
            }),
        );

        let stored = round_trip("subagent-only", event);

        let SourceValue::Recorded(subagent) = &stored.subagent else {
            panic!("expected a recorded subagent, got {:?}", stored.subagent);
        };
        assert_eq!(subagent.outcome, SourceValue::Absent);
        assert_eq!(subagent.duration_ms, SourceValue::Recorded(467407));
        assert_eq!(stored.tool_call, SourceValue::Absent);
    }

    /// The accesses a recorded command implies are derived on the way in, so a
    /// stored event carries them without any reader parsing shell again — and an
    /// operand the shell would have expanded survives as a fragment with its
    /// reason, rather than being flattened into a path.
    #[test]
    fn shell_derived_accesses_and_their_ambiguity_round_trip_through_the_index() {
        let event = Event {
            kind: EventKind::ToolCall,
            tool_call: SourceValue::Recorded(crate::model::ToolCall {
                name: "Bash".to_string(),
                call_id: SourceValue::Absent,
                input: SourceValue::Absent,
                command: SourceValue::Recorded("cat logs/*.txt > build/out.env".to_string()),
                path: SourceValue::Absent,
                url: SourceValue::Absent,
                cwd: SourceValue::Recorded("/work/app".to_string()),
            }),
            ..spawn_event("/tmp/shell-access.jsonl", SourceValue::Absent)
        };

        let stored = round_trip("shell-access", event);

        let SourceValue::Recorded(files) = &stored.files else {
            panic!("expected recorded files, got {:?}", stored.files);
        };
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "logs/*.txt");
        assert_eq!(
            files[0].provenance,
            SourceValue::Recorded("shell".to_string())
        );
        assert_eq!(
            files[0].ambiguity,
            SourceValue::Recorded("glob not expanded".to_string())
        );
        assert_eq!(files[1].path, "build/out.env");
        assert_eq!(
            files[1].operation,
            SourceValue::Recorded("write".to_string())
        );
        assert_eq!(files[1].ambiguity, SourceValue::Absent);
    }

    /// Two records of one API response, each repeating that response's usage,
    /// which is how Claude actually writes a multi-block reply.
    fn repeated_response_events(source_path: &str, response_id: &str) -> Vec<Event> {
        (1..=2)
            .map(|line| Event {
                id: format!("evt-{line}"),
                response_id: SourceValue::Recorded(response_id.to_string()),
                token_usage: SourceValue::Recorded(TokenUsage {
                    input: SourceValue::Recorded(2),
                    output: SourceValue::Recorded(109),
                    cache_read: SourceValue::Recorded(29447),
                    cache_write: SourceValue::Recorded(22624),
                    total: SourceValue::Absent,
                    cost_total_micros: SourceValue::Absent,
                    scope: "message".to_string(),
                }),
                source: Provenance {
                    line,
                    ordinal: line,
                    ..provenance(source_path)
                },
                kind: EventKind::AssistantTurn,
                ..spawn_event(source_path, SourceValue::Absent)
            })
            .collect()
    }

    /// The dedup key has to survive the index, or the fold that collapses
    /// Claude's repeated records has nothing to group by.
    #[test]
    fn two_records_of_one_response_read_back_with_the_same_response_identity() {
        let source_path = "/tmp/repeated-response.jsonl";
        let mut backend = temp_backend("response-identity");
        index_file(
            &mut backend,
            source_path,
            ParsedSession {
                session: Some(indexed_session("session-1", source_path)),
                events: repeated_response_events(source_path, "msg_011Cdp1FRD2gPCQXuN1bKoF8"),
                ..ParsedSession::default()
            },
        );

        let page = backend
            .get_event_page(
                "session-1",
                PageQuery {
                    limit: 10,
                    offset: 0,
                },
            )
            .expect("read the event page");

        assert_eq!(page.events.len(), 2);
        for event in &page.events {
            assert_eq!(
                event.response_id,
                SourceValue::Recorded("msg_011Cdp1FRD2gPCQXuN1bKoF8".to_string())
            );
        }
    }

    #[test]
    fn an_event_whose_harness_wrote_no_response_identity_reads_back_absent() {
        let event = spawn_event("/tmp/no-response-id.jsonl", SourceValue::Absent);

        let stored = round_trip("response-identity-absent", event);

        assert_eq!(stored.response_id, SourceValue::Absent);
    }

    /// Both facts a price rests on have to survive the index: the model that
    /// spent the tokens, and the dollar figure the harness itself charged. The
    /// cost rides inside the usage payload rather than in a column of its own,
    /// so this is the only thing that proves it is not silently dropped there.
    #[test]
    fn a_recorded_model_and_price_round_trip_through_the_index() {
        let event = Event {
            model: SourceValue::Recorded("claude-sonnet-5".to_string()),
            token_usage: SourceValue::Recorded(TokenUsage {
                input: SourceValue::Recorded(6596),
                output: SourceValue::Recorded(296),
                cache_read: SourceValue::Recorded(5632),
                cache_write: SourceValue::Recorded(0),
                total: SourceValue::Recorded(12524),
                cost_total_micros: SourceValue::Recorded(17870),
                scope: "message".to_string(),
            }),
            kind: EventKind::AssistantTurn,
            ..spawn_event("/tmp/priced.jsonl", SourceValue::Absent)
        };

        let stored = round_trip("model-and-price", event);

        assert_eq!(
            stored.model,
            SourceValue::Recorded("claude-sonnet-5".to_string())
        );
        let SourceValue::Recorded(usage) = &stored.token_usage else {
            panic!("expected recorded usage, got {:?}", stored.token_usage);
        };
        assert_eq!(usage.cost_total_micros, SourceValue::Recorded(17870));
    }

    /// A harness that named no model, and one whose value names nothing
    /// priceable, are different facts and both have to survive as themselves.
    #[test]
    fn an_unrecorded_and_an_unpriceable_model_read_back_as_the_states_they_were() {
        for expected in [SourceValue::Absent, SourceValue::Unsupported] {
            let event = Event {
                model: expected.clone(),
                ..spawn_event("/tmp/model-states.jsonl", SourceValue::Absent)
            };

            let stored = round_trip("model-states", event);

            assert_eq!(stored.model, expected);
        }
    }

    fn indexed_session(id: &str, path: &str) -> Session {
        Session {
            id: id.to_string(),
            harness: Harness::ClaudeCode,
            source: provenance(path),
            native_id: SourceValue::Absent,
            project: SourceValue::Absent,
            parent_session: SourceValue::Absent,
        }
    }

    fn index_file(backend: &mut SqliteBackend, path: &str, parsed: ParsedSession) {
        backend
            .upsert_file(
                Harness::ClaudeCode,
                &FileIdentity {
                    path: path.to_string(),
                    mtime_secs: 1,
                    mtime_nanos: 0,
                    size: 1,
                },
                parsed,
            )
            .expect("index one file");
    }

    /// A parent whose spawn named a child, and the child's own transcript,
    /// indexed as the ordinary peer session it already is.
    fn backend_with_parent_and_child(label: &str, child_path: &str) -> SqliteBackend {
        let mut backend = temp_backend(label);
        index_file(
            &mut backend,
            "/tmp/parent.jsonl",
            ParsedSession {
                session: Some(indexed_session("session-1", "/tmp/parent.jsonl")),
                events: vec![spawn_event(
                    "/tmp/parent.jsonl",
                    SourceValue::Recorded(fully_recorded_subagent()),
                )],
                ..ParsedSession::default()
            },
        );
        index_file(
            &mut backend,
            child_path,
            ParsedSession {
                session: Some(indexed_session("session-child", child_path)),
                ..ParsedSession::default()
            },
        );
        backend
    }

    fn read_spawn(backend: &SqliteBackend) -> crate::model::Subagent {
        let page = backend
            .get_event_page(
                "session-1",
                PageQuery {
                    limit: 10,
                    offset: 0,
                },
            )
            .expect("read the parent's event page");
        match page.events.into_iter().next().expect("the spawn").subagent {
            SourceValue::Recorded(subagent) => subagent,
            other => panic!("expected a recorded subagent, got {other:?}"),
        }
    }

    #[test]
    fn a_recorded_spawn_resolves_to_its_indexed_child_session_at_read_time() {
        let backend = backend_with_parent_and_child(
            "resolve-child",
            "/tmp/parent/subagents/agent-a3c304e0299ebbabc.jsonl",
        );

        let subagent = read_spawn(&backend);

        assert_eq!(
            subagent.child_session_id,
            SourceValue::Recorded("session-child".to_string())
        );
    }

    /// The source named a child; nothing indexed answers it. That is not the
    /// same fact as naming none, but neither gives the user a session to open.
    #[test]
    fn a_named_child_with_no_indexed_transcript_reads_back_absent() {
        let backend =
            backend_with_parent_and_child("resolve-missing", "/tmp/some-other-session.jsonl");

        let subagent = read_spawn(&backend);

        assert_eq!(subagent.child_session_id, SourceValue::Absent);
    }

    fn list_one(backend: &SqliteBackend) -> SessionListItem {
        backend
            .list_sessions(ListSessionsQuery {
                limit: 10,
                offset: 0,
                harness: None,
                project: None,
            })
            .expect("list sessions")
            .items
            .into_iter()
            .next()
            .expect("one listed session")
    }

    /// A priced Claude response, timestamped now so the age gate lets it
    /// through however long after this test was written it runs.
    fn priced_event_indexed_today(source_path: &str) -> Event {
        Event {
            model: SourceValue::Recorded("claude-sonnet-4-5-20250929".to_string()),
            timestamp: SourceValue::Recorded(iso_today()),
            token_usage: SourceValue::Recorded(TokenUsage {
                input: SourceValue::Absent,
                output: SourceValue::Recorded(1_000),
                cache_read: SourceValue::Absent,
                cache_write: SourceValue::Absent,
                total: SourceValue::Absent,
                cost_total_micros: SourceValue::Absent,
                scope: "message".to_string(),
            }),
            kind: EventKind::AssistantTurn,
            ..spawn_event(source_path, SourceValue::Absent)
        }
    }

    /// Today as an ISO timestamp, derived from the same day count the estimate
    /// gate reads, so this test does not expire a month after it was written.
    fn iso_today() -> String {
        format!(
            "{}T12:00:00Z",
            iso_date_from_days(crate::index::pricing::today_utc_days())
        )
    }

    /// The whole point of the columns: a list row carries the session's tokens
    /// and price without anything reopening its transcript.
    #[test]
    fn a_session_indexed_today_lists_an_estimated_price_from_the_embedded_catalog() {
        let source_path = "/tmp/estimate-today.jsonl";
        let mut backend = temp_backend("estimate-today");
        index_file(
            &mut backend,
            source_path,
            ParsedSession {
                session: Some(indexed_session("session-1", source_path)),
                events: vec![priced_event_indexed_today(source_path)],
                ..ParsedSession::default()
            },
        );

        let listed = list_one(&backend);

        assert_eq!(listed.estimated_tokens, SourceValue::Recorded(1_000));
        // 1,000 output tokens at Sonnet 4.5's $15/M.
        assert_eq!(listed.estimated_price_micros, SourceValue::Recorded(15_000));
        assert_eq!(
            listed.token_total,
            SourceValue::Absent,
            "Claude writes no total of its own, and Absent must not become 0"
        );
        assert_eq!(listed.recorded_price_micros, SourceValue::Absent);
        assert_eq!(
            listed.estimated_as_of,
            SourceValue::Recorded(crate::index::pricing_as_of()),
            "the row says which day's rates priced it, since the catalog can refresh later"
        );
    }

    /// Today's rate table is not evidence about what a year-old session paid,
    /// so it gets no dollar figure rather than a plausible looking wrong one.
    #[test]
    fn a_session_older_than_a_month_lists_no_estimate_at_all() {
        let source_path = "/tmp/estimate-old.jsonl";
        let mut backend = temp_backend("estimate-old");
        index_file(
            &mut backend,
            source_path,
            ParsedSession {
                session: Some(indexed_session("session-1", source_path)),
                events: vec![Event {
                    timestamp: SourceValue::Recorded("2020-01-01T10:00:00Z".to_string()),
                    ..priced_event_indexed_today(source_path)
                }],
                ..ParsedSession::default()
            },
        );

        let listed = list_one(&backend);

        assert_eq!(listed.estimated_tokens, SourceValue::Absent);
        assert_eq!(listed.estimated_price_micros, SourceValue::Absent);
    }

    /// Pi charges its own price, and that receipt is what the row carries. No
    /// estimate is written beside it, so nothing can later present arithmetic
    /// as the figure the harness billed.
    #[test]
    fn a_harness_recorded_price_lists_as_recorded_with_no_estimate_beside_it() {
        let source_path = "/tmp/recorded-price.jsonl";
        let mut backend = temp_backend("recorded-price");
        let event = Event {
            token_usage: SourceValue::Recorded(TokenUsage {
                input: SourceValue::Recorded(10),
                output: SourceValue::Recorded(2),
                cache_read: SourceValue::Absent,
                cache_write: SourceValue::Absent,
                total: SourceValue::Recorded(12),
                cost_total_micros: SourceValue::Recorded(17_870),
                scope: "message".to_string(),
            }),
            ..priced_event_indexed_today(source_path)
        };
        index_file(
            &mut backend,
            source_path,
            ParsedSession {
                session: Some(indexed_session("session-1", source_path)),
                events: vec![event],
                ..ParsedSession::default()
            },
        );

        let listed = list_one(&backend);

        assert_eq!(listed.recorded_price_micros, SourceValue::Recorded(17_870));
        assert_eq!(listed.token_total, SourceValue::Recorded(12));
        assert_eq!(listed.estimated_price_micros, SourceValue::Absent);
    }

    /// Re-reading a changed transcript refreshes what the harness recorded, but
    /// the estimate stays the one the first index wrote: restating it against a
    /// newer catalog would silently reprice a session at rates it never ran at.
    #[test]
    fn reindexing_a_source_file_keeps_the_estimate_the_first_index_froze() {
        let source_path = "/tmp/frozen-estimate.jsonl";
        let mut backend = temp_backend("frozen-estimate");
        let parsed = || ParsedSession {
            session: Some(indexed_session("session-1", source_path)),
            events: vec![priced_event_indexed_today(source_path)],
            ..ParsedSession::default()
        };
        index_file(&mut backend, source_path, parsed());

        // Stand in for a catalog that has since moved: the first estimate is
        // whatever was on the row when the second reconcile started.
        backend
            .conn
            .execute(
                "UPDATE sessions SET estimated_price_micros_value = '4242',
                                     estimated_tokens_value = '999',
                                     estimated_as_of_value = '\"2026-01-02\"'",
                [],
            )
            .expect("age the stored estimate");
        index_file(&mut backend, source_path, parsed());

        let listed = list_one(&backend);
        assert_eq!(listed.estimated_price_micros, SourceValue::Recorded(4_242));
        assert_eq!(listed.estimated_tokens, SourceValue::Recorded(999));
        assert_eq!(
            listed.estimated_as_of,
            SourceValue::Recorded("2026-01-02".to_string()),
            "the date freezes with the price it dates, or the row would claim \
             an old figure was checked against today's rates"
        );
    }

    /// The Summary tile folds the same events the Subagents tab lists, so both
    /// must see the same resolution.
    #[test]
    fn the_summary_read_path_resolves_children_the_same_way_the_page_read_does() {
        let backend = backend_with_parent_and_child(
            "resolve-summary",
            "/tmp/parent/subagents/agent-a3c304e0299ebbabc.jsonl",
        );

        let events = backend
            .events_for_session("session-1")
            .expect("the summary's internal event read");

        let SourceValue::Recorded(subagent) = &events[0].subagent else {
            panic!("expected a recorded subagent");
        };
        assert_eq!(
            subagent.child_session_id,
            SourceValue::Recorded("session-child".to_string())
        );
    }
}
