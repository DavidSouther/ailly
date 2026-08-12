//! Persist and restore [`SourceValue`] with explicit kind discrimination.

use crate::model::SourceValue;
use rusqlite::types::{FromSql, FromSqlError, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Result as SqlResult;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceKind {
    Recorded,
    Absent,
    Unsupported,
    Malformed,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Absent => "absent",
            Self::Unsupported => "unsupported",
            Self::Malformed => "malformed",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "recorded" => Some(Self::Recorded),
            "absent" => Some(Self::Absent),
            "unsupported" => Some(Self::Unsupported),
            "malformed" => Some(Self::Malformed),
            _ => None,
        }
    }
}

impl ToSql for SourceKind {
    fn to_sql(&self) -> SqlResult<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

impl FromSql for SourceKind {
    fn column_result(value: ValueRef<'_>) -> Result<Self, FromSqlError> {
        let text = value.as_str()?;
        Self::parse(text).ok_or(FromSqlError::InvalidType)
    }
}

pub fn encode<T: serde::Serialize>(value: &SourceValue<T>) -> (SourceKind, Option<String>) {
    match value {
        SourceValue::Recorded(inner) => (
            SourceKind::Recorded,
            Some(serde_json::to_string(inner).expect("serialize recorded value")),
        ),
        SourceValue::Absent => (SourceKind::Absent, None),
        SourceValue::Unsupported => (SourceKind::Unsupported, None),
        SourceValue::Malformed => (SourceKind::Malformed, None),
    }
}

pub fn decode<T: for<'de> serde::Deserialize<'de>>(
    kind: SourceKind,
    json: Option<&str>,
) -> SourceValue<T> {
    match kind {
        SourceKind::Recorded => json
            .and_then(|text| serde_json::from_str(text).ok())
            .map(SourceValue::Recorded)
            .unwrap_or(SourceValue::Malformed),
        SourceKind::Absent => SourceValue::Absent,
        SourceKind::Unsupported => SourceValue::Unsupported,
        SourceKind::Malformed => SourceValue::Malformed,
    }
}

pub fn decode_required<T: for<'de> serde::Deserialize<'de>>(
    kind: SourceKind,
    json: Option<&str>,
) -> Result<SourceValue<T>, IndexCodecError> {
    match kind {
        SourceKind::Recorded => {
            let Some(text) = json else {
                return Err(IndexCodecError::MissingRecordedPayload);
            };
            serde_json::from_str(text)
                .map(SourceValue::Recorded)
                .map_err(|_| IndexCodecError::InvalidRecordedPayload)
        }
        SourceKind::Absent => Ok(SourceValue::Absent),
        SourceKind::Unsupported => Ok(SourceValue::Unsupported),
        SourceKind::Malformed => Ok(SourceValue::Malformed),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexCodecError {
    MissingRecordedPayload,
    InvalidRecordedPayload,
}

impl fmt::Display for IndexCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRecordedPayload => write!(f, "recorded value missing payload"),
            Self::InvalidRecordedPayload => write!(f, "recorded value payload is invalid JSON"),
        }
    }
}

impl std::error::Error for IndexCodecError {}
