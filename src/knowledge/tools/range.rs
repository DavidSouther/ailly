use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRange(pub Range<usize>);

#[derive(Debug, thiserror::Error)]
pub enum LineRangeError {
    #[error("line range start must be >= 1")]
    InvalidStart,
    #[error("line range end {end} must be >= start {start}")]
    InvalidOrder { start: u32, end: u32 },
}

#[derive(serde::Deserialize)]
struct LineRangeRaw {
    start: u32,
    end: u32,
}

impl<'de> serde::Deserialize<'de> for LineRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = LineRangeRaw::deserialize(deserializer)?;
        Self::try_from(raw.start..raw.end).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<Range<u32>> for LineRange {
    type Error = LineRangeError;

    /// `value.start` and `value.end` are interpreted as the 1-indexed
    /// inclusive form supplied on the JSON arg surface. The stored
    /// `Range<usize>` is the canonical 0-indexed half-open form so
    /// downstream slicing is direct.
    fn try_from(value: Range<u32>) -> Result<Self, Self::Error> {
        let Range { start, end } = value;
        if start == 0 {
            return Err(LineRangeError::InvalidStart);
        }
        if end < start {
            return Err(LineRangeError::InvalidOrder { start, end });
        }
        Ok(LineRange((start - 1) as usize..end as usize))
    }
}

impl LineRange {
    pub fn start_one_indexed(&self) -> usize {
        self.0.start + 1
    }

    pub fn end_one_indexed(&self) -> usize {
        self.0.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_happy_path_canonicalizes_to_zero_indexed_half_open() {
        let json = r#"{"start":1,"end":3}"#;
        let range: LineRange = serde_json::from_str(json).expect("valid range");
        assert_eq!(range.0, 0..3);
        assert_eq!(range.start_one_indexed(), 1);
        assert_eq!(range.end_one_indexed(), 3);
    }

    #[test]
    fn deserialize_rejects_start_zero() {
        let json = r#"{"start":0,"end":3}"#;
        let err = serde_json::from_str::<LineRange>(json).expect_err("must reject start == 0");
        assert!(
            err.to_string().contains("start must be >= 1"),
            "error names InvalidStart: {err}"
        );
    }

    #[test]
    fn deserialize_rejects_end_before_start() {
        let json = r#"{"start":5,"end":2}"#;
        let err = serde_json::from_str::<LineRange>(json).expect_err("must reject end < start");
        assert!(
            err.to_string().contains("end 2 must be >= start 5"),
            "error names InvalidOrder: {err}"
        );
    }

    #[test]
    fn deserialize_single_line_range() {
        let json = r#"{"start":7,"end":7}"#;
        let range: LineRange = serde_json::from_str(json).expect("valid single-line range");
        assert_eq!(range.0, 6..7);
    }

    #[test]
    fn try_from_range_constructs_line_range_from_one_indexed_inclusive() {
        let range = LineRange::try_from(2u32..5u32).expect("valid range");
        assert_eq!(range.0, 1..5);
        assert_eq!(range.start_one_indexed(), 2);
        assert_eq!(range.end_one_indexed(), 5);
    }

    #[test]
    fn try_from_range_rejects_start_zero() {
        let err = LineRange::try_from(0u32..3u32).expect_err("must reject start == 0");
        assert!(matches!(err, LineRangeError::InvalidStart));
    }

    #[test]
    fn try_from_range_rejects_end_before_start() {
        let reversed = std::ops::Range::<u32> { start: 5, end: 2 };
        let err = LineRange::try_from(reversed).expect_err("must reject end < start");
        assert!(matches!(
            err,
            LineRangeError::InvalidOrder { start: 5, end: 2 }
        ));
    }
}
