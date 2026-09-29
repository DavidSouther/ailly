//! Session-level token and dollar figures, computed once at index time.
//!
//! The client used to fold these out of an event page with its own copy of the
//! rate table. It no longer does: a session's headline tokens and price are
//! columns on the session row, readable without opening a transcript, and this
//! module is the only place the arithmetic lives.
//!
//! Five facts come out of one pass over a session's events:
//!
//! - `token_total` — the harness's own recorded totals, summed. Absent for
//!   Claude, which writes no `total_tokens` anywhere.
//! - `recorded_price_micros` — the dollars the harness itself charged. Only Pi
//!   writes any.
//! - `estimated_tokens` — the deduped four-bucket sum this estimate priced.
//! - `estimated_price_micros` — that sum multiplied out of the pinned catalog.
//! - `estimated_as_of` — the date of the catalog that priced it.
//!
//! All five are written only when there is no recorded price *and* the
//! session is recent enough for today's rates to plausibly be the rates it
//! ran at (see [`ESTIMATE_MAX_AGE_DAYS`]). `estimated_tokens` is then always
//! recorded, since summing the four buckets needs no rate table at all; the
//! two catalog-derived fields, `estimated_price_micros` and
//! `estimated_as_of`, are written together or not at all, depending on
//! whether any response's model had a published rate.
//!
//! The catalog itself can be refreshed while the app runs (see [`catalog`]),
//! which is exactly why the date it was priced against is stored beside the
//! price: a session keeps the figure it was given, and can still say how old
//! the rates behind it are.

mod catalog;

pub use catalog::pricing_as_of;

use crate::index::domain::token_total_from_events;
use crate::model::{Event, Harness, SourceValue, TokenUsage};
use std::collections::HashSet;

/// Activates the cached rate table, then asks upstream for a newer one on a
/// background thread.
///
/// Best effort from end to end. Pricing is an estimate: a fresh checkout with no
/// network has to index exactly as well as a connected one, so every failure
/// here is logged and leaves the previous catalog — cached, or the one compiled
/// into the binary — in place. Nothing the index does waits on the fetch.
///
/// Safe to call on every rescan. The cache is read once per process, and a
/// fetch happens at most once a day, so the second call and the fiftieth cost
/// nothing.
pub fn start_catalog_refresh(cache_path: std::path::PathBuf) {
    static CACHE_READ: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !CACHE_READ.swap(true, std::sync::atomic::Ordering::SeqCst) && cache_path.exists() {
        match catalog::load_cache(&cache_path) {
            Ok(as_of) => eprintln!("ailly-analyzer: pricing rates as of {as_of}"),
            Err(err) => eprintln!(
                "ailly-analyzer: ignoring the cached pricing catalog at {}: {err}",
                cache_path.display()
            ),
        }
    }

    let today = today_iso_date();
    if !catalog::refresh_needed(&today) {
        return;
    }
    std::thread::spawn(
        move || match catalog::refresh_from_upstream(&cache_path, &today) {
            Ok(as_of) => eprintln!("ailly-analyzer: pricing rates refreshed to {as_of}"),
            Err(err) => eprintln!(
                "ailly-analyzer: pricing refresh failed, keeping rates as of {}: {err}",
                pricing_as_of()
            ),
        },
    );
}

/// How old a session may be, at the moment it is first indexed, and still get
/// an estimated price.
///
/// The catalog is today's rate table. Multiplying a two-year-old session's
/// tokens by it produces a dollar figure that session never paid, dressed as
/// arithmetic. One month is the window in which the pinned rates are plausibly
/// the rates that ran, so beyond it the estimate columns stay Absent and the
/// surfaces read "Not recorded" rather than inventing a historical price.
pub const ESTIMATE_MAX_AGE_DAYS: i64 = 30;

/// The four disjoint buckets every figure is built from.
///
/// Normalizing into these first is what keeps one arithmetic correct across
/// three harnesses that mean different things by "input": Codex counts its
/// cached portion *inside* `input_tokens` while Claude and Pi count theirs
/// beside it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenBuckets {
    pub fresh_input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl TokenBuckets {
    pub fn total(self) -> u64 {
        self.fresh_input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }

    fn add(self, other: Self) -> Self {
        Self {
            fresh_input: self.fresh_input.saturating_add(other.fresh_input),
            output: self.output.saturating_add(other.output),
            cache_read: self.cache_read.saturating_add(other.cache_read),
            cache_write: self.cache_write.saturating_add(other.cache_write),
        }
    }
}

fn recorded_amount(value: &SourceValue<u64>) -> u64 {
    match value {
        SourceValue::Recorded(amount) => *amount,
        _ => 0,
    }
}

/// One record's usage, normalized per harness.
///
/// The harness's own `total` is deliberately never read here: every figure the
/// estimate rests on is derived from these parts, so a composition can never
/// disagree with the total above it. A Codex record whose cache figures exceed
/// its input would derive a negative remainder; the recorded cache figures are
/// kept and only the derivation is floored, since negative spend is not a fact
/// any harness recorded.
pub fn buckets_from_usage(usage: &TokenUsage, harness: Harness) -> TokenBuckets {
    let input = recorded_amount(&usage.input);
    let cache_read = recorded_amount(&usage.cache_read);
    let cache_write = recorded_amount(&usage.cache_write);
    TokenBuckets {
        fresh_input: match harness {
            Harness::Codex => input.saturating_sub(cache_read).saturating_sub(cache_write),
            Harness::ClaudeCode | Harness::Pi => input,
        },
        output: recorded_amount(&usage.output),
        cache_read,
        cache_write,
    }
}

/// What one response's buckets cost at the catalog's rates, in millionths of a
/// dollar, or nothing when the catalog has no rate for the model.
///
/// The buckets are already disjoint, so each is charged exactly once at its own
/// rate. Where the catalog names no separate cache rate, the plain input rate
/// stands in: that overstates a cache read and understates nothing, which is
/// preferable to dropping a model's price entirely over a missing discount.
fn estimated_micros(buckets: TokenBuckets, model: &str) -> Option<f64> {
    let rates = catalog::rates_for_model(model)?;
    let cache_read_rate = rates.cache_read.unwrap_or(rates.input);
    let cache_write_rate = rates.cache_write.unwrap_or(rates.input);
    let usd = buckets.fresh_input as f64 * rates.input
        + buckets.output as f64 * rates.output
        + buckets.cache_read as f64 * cache_read_rate
        + buckets.cache_write as f64 * cache_write_rate;
    Some(usd * 1_000_000.0)
}

/// The session-row figures, each independently recorded-or-not.
///
/// Scoped to one session's *own* events. A spawned child is its own indexed
/// session with its own row, so folding a child's spend in here would double
/// count the moment both rows are listed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTokenFigures {
    pub token_total: SourceValue<u64>,
    pub recorded_price_micros: SourceValue<u64>,
    pub estimated_tokens: SourceValue<u64>,
    pub estimated_price_micros: SourceValue<u64>,
    /// The date of the catalog `estimated_price_micros` was multiplied out of,
    /// stored because that catalog can be refreshed afterwards while this
    /// figure cannot. Without it, a surface could only say "estimated" and not
    /// how old the rates behind the estimate are.
    pub estimated_as_of: SourceValue<String>,
}

impl Default for SessionTokenFigures {
    /// A session whose source recorded none of these. Every field is Absent,
    /// never a zero, so "spent nothing" and "never said" stay distinguishable.
    fn default() -> Self {
        Self {
            token_total: SourceValue::Absent,
            recorded_price_micros: SourceValue::Absent,
            estimated_tokens: SourceValue::Absent,
            estimated_price_micros: SourceValue::Absent,
            estimated_as_of: SourceValue::Absent,
        }
    }
}

/// An estimate a previous index run already wrote, which a re-index must not
/// restate against a newer catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenEstimate {
    pub estimated_tokens: SourceValue<u64>,
    pub estimated_price_micros: SourceValue<u64>,
    /// Frozen with the price it dates. A re-index that kept the old figure but
    /// stamped it with today's catalog would claim the price was checked
    /// against rates it never saw.
    pub estimated_as_of: SourceValue<String>,
}

impl FrozenEstimate {
    /// Whether these columns hold an estimate at all. A row that was indexed
    /// while too old, or with a recorded price, holds nothing to freeze and
    /// must stay eligible for an estimate if it later becomes one.
    pub fn is_written(&self) -> bool {
        matches!(self.estimated_tokens, SourceValue::Recorded(_))
            || matches!(self.estimated_price_micros, SourceValue::Recorded(_))
    }
}

/// One session's events, counted once per API response.
///
/// Claude writes one response as several records that each repeat that
/// response's identical usage, so the response identity is the key that
/// collapses them — summing records overcounts a real session by 56%. A record
/// that carried no identity stands on its own: nothing in the transcript
/// asserts it is a repeat of its neighbour.
fn counted_responses(events: &[Event]) -> Vec<(&Event, TokenBuckets)> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut counted = Vec::new();
    for event in events {
        let SourceValue::Recorded(usage) = &event.token_usage else {
            continue;
        };
        if let SourceValue::Recorded(response_id) = &event.response_id {
            if !seen.insert(response_id.as_str()) {
                continue;
            }
        }
        counted.push((event, buckets_from_usage(usage, event.source.harness)));
    }
    counted
}

/// The latest recorded event timestamp, which is the session row's
/// `last_activity` and the age the estimate gate reads.
fn last_activity(events: &[Event]) -> Option<&str> {
    events
        .iter()
        .filter_map(|event| match &event.timestamp {
            SourceValue::Recorded(timestamp) => Some(timestamp.as_str()),
            _ => None,
        })
        .max()
}

/// Whole days since the Unix epoch for an ISO-8601 timestamp's date part.
///
/// Only `YYYY-MM-DD` is read. The gate it feeds is thirty days wide, so an
/// unread time-of-day or UTC offset cannot move a session across it by more
/// than a day, and no date library needs to enter the dependency tree for it.
fn days_since_epoch(timestamp: &str) -> Option<i64> {
    let date = timestamp.get(..10)?;
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Howard Hinnant's `days_from_civil`: a proleptic Gregorian date as a day
/// count from 1970-01-01, exact for every date these transcripts can hold.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The inverse of [`days_since_epoch`]: an epoch day count as `YYYY-MM-DD`.
///
/// Two callers want a date rather than a day count: the catalog, which compares
/// its own `asOf` against today to decide whether a refresh could tell it
/// anything, and any test that needs "a session recorded today" — which must
/// build that date from `today_utc_days()` or it expires the month after it is
/// written.
pub fn iso_date_from_days(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Today in UTC as `YYYY-MM-DD`, the form the catalog dates itself in.
pub fn today_iso_date() -> String {
    iso_date_from_days(today_utc_days())
}

/// Howard Hinnant's `civil_from_days`, the exact inverse of `days_from_civil`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Today, as whole days since the Unix epoch in UTC.
pub fn today_utc_days() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| (since.as_secs() / 86_400) as i64)
        .unwrap_or(0)
}

/// Whether a session is recent enough to price from today's catalog.
///
/// A session whose every event is untimestamped has no age to judge, so it is
/// not estimated: an unknown age is not evidence of a recent one.
fn within_estimate_window(events: &[Event], today_days: i64) -> bool {
    last_activity(events)
        .and_then(days_since_epoch)
        .is_some_and(|day| today_days - day <= ESTIMATE_MAX_AGE_DAYS)
}

/// One session's figures, as the index stores them.
///
/// `frozen` is whatever a previous run already wrote. When it holds an
/// estimate, that estimate is kept verbatim: re-reading a transcript can
/// refresh what the harness recorded, but it must never restate a dollar figure
/// against a catalog the first estimate never saw.
pub fn session_figures(
    events: &[Event],
    today_days: i64,
    frozen: Option<&FrozenEstimate>,
) -> SessionTokenFigures {
    let (token_total, _) = token_total_from_events(events);
    let counted = counted_responses(events);

    let mut recorded_micros = 0u64;
    let mut any_recorded_price = false;
    for (event, _) in &counted {
        let SourceValue::Recorded(usage) = &event.token_usage else {
            continue;
        };
        if let SourceValue::Recorded(micros) = usage.cost_total_micros {
            recorded_micros = recorded_micros.saturating_add(micros);
            any_recorded_price = true;
        }
    }
    let recorded_price_micros = if any_recorded_price {
        SourceValue::Recorded(recorded_micros)
    } else {
        SourceValue::Absent
    };

    if let Some(frozen) = frozen.filter(|frozen| frozen.is_written()) {
        return SessionTokenFigures {
            token_total,
            recorded_price_micros,
            estimated_tokens: frozen.estimated_tokens.clone(),
            estimated_price_micros: frozen.estimated_price_micros.clone(),
            estimated_as_of: frozen.estimated_as_of.clone(),
        };
    }

    // A harness that charged its own price needs no estimate, and overwriting a
    // receipt with arithmetic would be the one substitution this whole design
    // exists to prevent.
    if any_recorded_price || counted.is_empty() || !within_estimate_window(events, today_days) {
        return SessionTokenFigures {
            token_total,
            recorded_price_micros,
            estimated_tokens: SourceValue::Absent,
            estimated_price_micros: SourceValue::Absent,
            estimated_as_of: SourceValue::Absent,
        };
    }

    let mut buckets = TokenBuckets::default();
    let mut priced_micros = 0f64;
    let mut any_priced = false;
    for (event, response) in &counted {
        buckets = buckets.add(*response);
        if let SourceValue::Recorded(model) = &event.model {
            if let Some(micros) = estimated_micros(*response, model) {
                priced_micros += micros;
                any_priced = true;
            }
        }
    }

    SessionTokenFigures {
        token_total,
        recorded_price_micros,
        estimated_tokens: SourceValue::Recorded(buckets.total()),
        // Rounded once, at the end: rounding each response to a whole millionth
        // first would drift over a long session.
        estimated_price_micros: if any_priced {
            SourceValue::Recorded(priced_micros.round().max(0.0) as u64)
        } else {
            SourceValue::Absent
        },
        // Dated only when there is a price to date. A session whose tokens were
        // counted but whose model has no published rate was not priced against
        // any catalog, so naming one would imply a figure that is not there.
        estimated_as_of: if any_priced {
            SourceValue::Recorded(catalog::pricing_as_of())
        } else {
            SourceValue::Absent
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, Provenance};

    fn usage(parts: &[(&str, u64)]) -> TokenUsage {
        let read = |name: &str| {
            parts
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| SourceValue::Recorded(*value))
                .unwrap_or(SourceValue::Absent)
        };
        TokenUsage {
            input: read("input"),
            output: read("output"),
            cache_read: read("cache_read"),
            cache_write: read("cache_write"),
            total: read("total"),
            cost_total_micros: read("cost"),
            scope: "message".to_string(),
        }
    }

    struct Response {
        harness: Harness,
        model: Option<&'static str>,
        response_id: Option<&'static str>,
        timestamp: Option<&'static str>,
        usage: TokenUsage,
    }

    impl Response {
        fn new(usage: TokenUsage) -> Self {
            Self {
                harness: Harness::ClaudeCode,
                model: None,
                response_id: None,
                timestamp: Some("2026-08-14T10:00:00Z"),
                usage,
            }
        }

        fn model(mut self, model: &'static str) -> Self {
            self.model = Some(model);
            self
        }

        fn harness(mut self, harness: Harness) -> Self {
            self.harness = harness;
            self
        }

        fn response_id(mut self, id: &'static str) -> Self {
            self.response_id = Some(id);
            self
        }

        fn at(mut self, timestamp: &'static str) -> Self {
            self.timestamp = Some(timestamp);
            self
        }

        fn untimestamped(mut self) -> Self {
            self.timestamp = None;
            self
        }

        fn build(self, ordinal: usize) -> Event {
            Event {
                id: format!("evt-{ordinal}"),
                session_id: "session-1".to_string(),
                kind: EventKind::AssistantTurn,
                source: Provenance {
                    harness: self.harness,
                    path: "/tmp/session.jsonl".to_string(),
                    line: ordinal,
                    ordinal,
                },
                native_id: SourceValue::Absent,
                response_id: match self.response_id {
                    Some(id) => SourceValue::Recorded(id.to_string()),
                    None => SourceValue::Absent,
                },
                model: match self.model {
                    Some(model) => SourceValue::Recorded(model.to_string()),
                    None => SourceValue::Absent,
                },
                timestamp: match self.timestamp {
                    Some(timestamp) => SourceValue::Recorded(timestamp.to_string()),
                    None => SourceValue::Absent,
                },
                turn: SourceValue::Absent,
                tool_call: SourceValue::Absent,
                tool_result: SourceValue::Absent,
                token_usage: SourceValue::Recorded(self.usage),
                files: SourceValue::Absent,
                detail: SourceValue::Absent,
                subagent: SourceValue::Absent,
            }
        }
    }

    fn events(responses: Vec<Response>) -> Vec<Event> {
        responses
            .into_iter()
            .enumerate()
            .map(|(index, response)| response.build(index + 1))
            .collect()
    }

    /// The day 2026-08-14 falls on, so an "indexed today" case can be written
    /// against the fixtures' own timestamps rather than against the wall clock.
    fn indexed_on(date: &str) -> i64 {
        days_since_epoch(date).expect("a readable ISO date")
    }

    #[test]
    fn claudes_cache_figures_sit_beside_its_input_and_codexs_sit_inside_it() {
        let recorded = usage(&[
            ("input", 1000),
            ("output", 50),
            ("cache_read", 800),
            ("cache_write", 100),
        ]);

        let claude = buckets_from_usage(&recorded, Harness::ClaudeCode);
        let codex = buckets_from_usage(&recorded, Harness::Codex);

        assert_eq!(claude.fresh_input, 1000);
        assert_eq!(claude.total(), 1950);
        assert_eq!(codex.fresh_input, 100, "1000 input already holds 800 + 100");
        assert_eq!(codex.total(), 1050);
    }

    /// A record whose cache figures exceed its input would derive a negative
    /// remainder, which is not a fact any harness recorded.
    #[test]
    fn a_codex_record_whose_cache_exceeds_its_input_floors_at_no_fresh_input() {
        let buckets = buckets_from_usage(
            &usage(&[("input", 100), ("cache_read", 900)]),
            Harness::Codex,
        );

        assert_eq!(buckets.fresh_input, 0);
        assert_eq!(buckets.cache_read, 900);
    }

    /// Claude writes one response as several records carrying one identical
    /// usage object. Counting records rather than responses doubles this
    /// session's tokens.
    #[test]
    fn two_records_of_one_response_are_estimated_as_one_response() {
        let figures = session_figures(
            &events(vec![
                Response::new(usage(&[("output", 1000)])).response_id("msg-a"),
                Response::new(usage(&[("output", 1000)])).response_id("msg-a"),
            ]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.estimated_tokens, SourceValue::Recorded(1000));
    }

    /// Codex and Pi write no response identity, so nothing in their transcripts
    /// asserts that a record is a repeat.
    #[test]
    fn records_with_no_response_identity_each_count_once() {
        let figures = session_figures(
            &events(vec![
                Response::new(usage(&[("output", 10)])).harness(Harness::Codex),
                Response::new(usage(&[("output", 10)])).harness(Harness::Codex),
            ]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.estimated_tokens, SourceValue::Recorded(20));
    }

    /// A million output tokens at Sonnet 4.5's $15/M, checkable by hand.
    #[test]
    fn prices_each_disjoint_bucket_at_its_own_rate() {
        let figures = session_figures(
            &events(vec![Response::new(usage(&[
                ("input", 1_000_000),
                ("output", 1_000_000),
                ("cache_read", 1_000_000),
                ("cache_write", 1_000_000),
            ]))
            .model("claude-sonnet-4-5-20250929")]),
            indexed_on("2026-08-14"),
            None,
        );

        // $3/M input, $15/M output, $0.30/M cache read, $3.75/M cache write.
        assert_eq!(
            figures.estimated_price_micros,
            SourceValue::Recorded(22_050_000)
        );
    }

    /// Pi is the only harness that writes what it charged, and its own figure
    /// outranks any arithmetic this module could do. Overwriting a receipt with
    /// an estimate is the one substitution the design forbids outright.
    #[test]
    fn a_harness_recorded_price_is_kept_and_no_estimate_is_written_beside_it() {
        let figures = session_figures(
            &events(vec![Response::new(usage(&[
                ("input", 1_000_000),
                ("cost", 17_900),
            ]))
            .harness(Harness::Pi)
            .model("claude-opus-5")]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.recorded_price_micros, SourceValue::Recorded(17_900));
        assert_eq!(figures.estimated_tokens, SourceValue::Absent);
        assert_eq!(figures.estimated_price_micros, SourceValue::Absent);
    }

    /// Zero dollars charged is a figure the harness wrote, not a missing one.
    #[test]
    fn a_recorded_price_of_zero_is_a_price() {
        let figures = session_figures(
            &events(vec![
                Response::new(usage(&[("input", 12), ("cost", 0)])).harness(Harness::Pi)
            ]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.recorded_price_micros, SourceValue::Recorded(0));
    }

    /// A session a month and a day old is priced from a rate table it may never
    /// have run at, so it gets no dollar figure at all rather than a plausible
    /// looking wrong one.
    #[test]
    fn a_session_older_than_the_estimate_window_is_not_estimated() {
        let figures = session_figures(
            &events(vec![Response::new(usage(&[("output", 1000)]))
                .model("claude-sonnet-4-5-20250929")
                .at("2026-06-01T10:00:00Z")]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.estimated_tokens, SourceValue::Absent);
        assert_eq!(figures.estimated_price_micros, SourceValue::Absent);
    }

    #[test]
    fn a_session_inside_the_estimate_window_is_estimated_at_todays_rates() {
        let today = indexed_on("2026-08-14");

        for age_days in [0, 1, ESTIMATE_MAX_AGE_DAYS] {
            let figures = session_figures(
                &events(vec![Response::new(usage(&[("output", 1000)]))
                    .model("claude-sonnet-4-5-20250929")
                    .at("2026-08-14T10:00:00Z")]),
                today + age_days,
                None,
            );

            assert_eq!(
                figures.estimated_price_micros,
                SourceValue::Recorded(15_000),
                "a session {age_days} days old is inside the window"
            );
        }
    }

    /// An unknown age is not evidence of a recent one.
    #[test]
    fn a_session_whose_events_are_all_untimestamped_is_not_estimated() {
        let figures = session_figures(
            &events(vec![Response::new(usage(&[("output", 1000)]))
                .model("claude-sonnet-4-5-20250929")
                .untimestamped()]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.estimated_price_micros, SourceValue::Absent);
    }

    /// A model with no published rate leaves the dollars unwritten. Its tokens
    /// are real and stay countable — only the price is withheld, so no surface
    /// reports the spend as free.
    #[test]
    fn tokens_survive_a_model_the_catalog_cannot_price_but_the_dollars_do_not() {
        let figures = session_figures(
            &events(vec![Response::new(usage(&[("output", 1000)]))
                .harness(Harness::Codex)
                .model("codex-auto-review")]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.estimated_tokens, SourceValue::Recorded(1000));
        assert_eq!(figures.estimated_price_micros, SourceValue::Absent);
    }

    /// An estimate says which day's rates produced it, so a reader looking at a
    /// price the index wrote months ago can tell how old those rates are — and
    /// so a catalog refreshed since then cannot be mistaken for the one that
    /// priced this session.
    #[test]
    fn an_estimate_carries_the_date_of_the_catalog_that_priced_it() {
        let figures = session_figures(
            &events(vec![
                Response::new(usage(&[("output", 1000)])).model("claude-sonnet-4-5-20250929")
            ]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(
            figures.estimated_as_of,
            SourceValue::Recorded(pricing_as_of())
        );
    }

    /// Tokens with no price were multiplied by no rate table, so there is no
    /// catalog date to name beside them.
    #[test]
    fn tokens_the_catalog_could_not_price_carry_no_catalog_date() {
        let figures = session_figures(
            &events(vec![Response::new(usage(&[("output", 1000)]))
                .harness(Harness::Codex)
                .model("codex-auto-review")]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.estimated_tokens, SourceValue::Recorded(1000));
        assert_eq!(figures.estimated_as_of, SourceValue::Absent);
    }

    /// Re-reading a transcript refreshes what the harness recorded. It must not
    /// restate a dollar figure against a catalog the first estimate never saw.
    #[test]
    fn a_reindex_keeps_the_estimate_the_first_index_froze() {
        let frozen = FrozenEstimate {
            estimated_tokens: SourceValue::Recorded(999),
            estimated_price_micros: SourceValue::Recorded(4_242),
            estimated_as_of: SourceValue::Recorded("2026-01-02".to_string()),
        };

        let figures = session_figures(
            &events(vec![
                Response::new(usage(&[("output", 1000)])).model("claude-sonnet-4-5-20250929")
            ]),
            indexed_on("2026-08-14"),
            Some(&frozen),
        );

        assert_eq!(figures.estimated_tokens, SourceValue::Recorded(999));
        assert_eq!(figures.estimated_price_micros, SourceValue::Recorded(4_242));
        assert_eq!(
            figures.estimated_as_of,
            SourceValue::Recorded("2026-01-02".to_string()),
            "the frozen price keeps the catalog date it was priced against"
        );
    }

    /// Columns that were left Absent — because the session was too old, or
    /// because the harness charged its own price — hold nothing to freeze, so a
    /// session that later becomes eligible still gets its estimate.
    #[test]
    fn absent_estimate_columns_are_not_treated_as_a_frozen_estimate() {
        let unwritten = FrozenEstimate {
            estimated_tokens: SourceValue::Absent,
            estimated_price_micros: SourceValue::Absent,
            estimated_as_of: SourceValue::Absent,
        };

        let figures = session_figures(
            &events(vec![
                Response::new(usage(&[("output", 1000)])).model("claude-sonnet-4-5-20250929")
            ]),
            indexed_on("2026-08-14"),
            Some(&unwritten),
        );

        assert_eq!(
            figures.estimated_price_micros,
            SourceValue::Recorded(15_000)
        );
    }

    /// The harness's own totals stay their own field, unaffected by the
    /// estimate beside them: Claude writes none, and Absent must not become 0.
    #[test]
    fn a_harness_that_wrote_no_total_leaves_token_total_absent() {
        let figures = session_figures(
            &events(vec![Response::new(usage(&[("output", 1000)]))]),
            indexed_on("2026-08-14"),
            None,
        );

        assert_eq!(figures.token_total, SourceValue::Absent);
        assert_eq!(figures.estimated_tokens, SourceValue::Recorded(1000));
    }

    #[test]
    fn a_session_with_no_usage_at_all_records_nothing() {
        let figures = session_figures(&[], indexed_on("2026-08-14"), None);

        assert_eq!(figures, SessionTokenFigures::default());
    }

    #[test]
    fn days_since_epoch_reads_the_date_part_of_the_timestamps_the_harnesses_write() {
        assert_eq!(days_since_epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(days_since_epoch("1970-01-02"), Some(1));
        assert_eq!(days_since_epoch("2026-08-14T10:00:00.123Z"), Some(20679));
        assert_eq!(
            days_since_epoch("2026-08-14T10:00:00+02:00"),
            Some(20679),
            "an offset shifts the instant by hours, never across a 30-day gate"
        );
        assert_eq!(days_since_epoch("not a timestamp"), None);
        assert_eq!(days_since_epoch("2026-13-01"), None);
    }

    /// The date arithmetic has to be exact in both directions, including across
    /// leap days and century boundaries, or the gate would let a session
    /// through on the wrong side of its window.
    #[test]
    fn an_epoch_day_count_round_trips_through_its_iso_date() {
        for date in [
            "1970-01-01",
            "2000-02-29",
            "2024-02-29",
            "2026-08-14",
            "2100-03-01",
        ] {
            let days = days_since_epoch(date).expect("a readable ISO date");
            assert_eq!(iso_date_from_days(days), date);
        }
    }
}
