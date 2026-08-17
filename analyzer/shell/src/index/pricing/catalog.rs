//! A pinned rate table that can refresh itself, so an estimate never needs the
//! network but is not stuck at the day the binary was built.
//!
//! Two catalogs, one shape. The **embedded** snapshot is compiled in with
//! `include_str!`, trimmed to the model families the three harnesses this app
//! reads can name — regenerate it with `scripts/pull-litellm-snapshot.ts`
//! (`mise run pull-litellm-snapshot`), whose subset rule and the measurements
//! behind it are in
//! `.ailly/developer/2026-08-14-A-review-token-usage/research/pricing-subset.md`.
//! The **runtime** catalog is fetched from LiteLLM upstream, trimmed to every
//! text-generation model any provider prices, cached beside the index, and used
//! in place of the embedded one whenever it is newer. The runtime trim is the
//! same rule that script writes for `--subset full`, implemented twice because
//! a running app has to refresh without Node.
//!
//! Rates are per token, which is LiteLLM's own unit, so nothing is converted on
//! the way in and no rounding happens before the multiplication.
//!
//! A rate table goes stale, which is the accepted risk of an estimate. The
//! index writes an estimate only for a session young enough that today's rates
//! are plausibly the rates it ran at, records beside it the catalog date that
//! priced it, and freezes it once written — so a refresh changes what the next
//! session is priced against, never what an already priced one is said to have
//! cost.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

const EMBEDDED_SNAPSHOT: &str = include_str!("litellm-snapshot.json");

const UPSTREAM_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

/// Ceiling on the whole fetch. A refresh is a background nicety, so a slow or
/// hanging network has to end as a logged failure rather than as a thread that
/// never finishes.
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

/// The modes that name a text-generating model. An embedding or image entry
/// prices something no session in this app can spend.
const TEXT_MODES: [&str; 3] = ["chat", "responses", "completion"];

/// One model's per-token USD rates, as the trimmed snapshot stores them.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    #[serde(rename = "cacheRead", skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(rename = "cacheWrite", skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Snapshot {
    #[serde(rename = "asOf")]
    as_of: String,
    #[serde(rename = "usdPerToken")]
    usd_per_token: HashMap<String, Rates>,
}

/// What went wrong while trying to replace the active catalog. Every variant is
/// survivable by design: the caller logs it and keeps the catalog it had.
#[derive(Debug)]
pub enum CatalogError {
    Fetch(String),
    Parse(serde_json::Error),
    Io(std::io::Error),
    /// Upstream parsed but priced nothing this app could read, which would
    /// leave every session unpriceable if it were activated.
    NothingPriced,
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fetch(message) => write!(f, "fetch failed: {message}"),
            Self::Parse(err) => write!(f, "pricing catalog is not readable JSON: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::NothingPriced => write!(f, "the trimmed catalog priced no models"),
        }
    }
}

impl From<std::io::Error> for CatalogError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for CatalogError {
    fn from(value: serde_json::Error) -> Self {
        Self::Parse(value)
    }
}

fn embedded() -> Arc<Snapshot> {
    static EMBEDDED: OnceLock<Arc<Snapshot>> = OnceLock::new();
    EMBEDDED
        .get_or_init(|| {
            Arc::new(
                serde_json::from_str(EMBEDDED_SNAPSHOT).expect("embedded pricing snapshot parses"),
            )
        })
        .clone()
}

/// The catalog every read goes through: the embedded snapshot until something
/// newer is loaded, then whatever that was.
///
/// A `OnceLock` alone stopped being enough once the table could be replaced
/// while the app runs. Readers take an `Arc` and let the lock go immediately, so
/// a hot swap never blocks a session being priced.
fn active() -> &'static RwLock<Arc<Snapshot>> {
    static ACTIVE: OnceLock<RwLock<Arc<Snapshot>>> = OnceLock::new();
    ACTIVE.get_or_init(|| RwLock::new(embedded()))
}

fn current() -> Arc<Snapshot> {
    active()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Swaps in a catalog dated no earlier than the active one, and reports the
/// date that ends up active either way.
///
/// Older is refused rather than accepted: a cache written before the embedded
/// file was last regenerated would otherwise walk the rates backwards on every
/// launch.
fn activate_if_newer(candidate: Snapshot) -> String {
    let mut active = active()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if candidate.as_of >= active.as_of {
        *active = Arc::new(candidate);
    }
    active.as_of.clone()
}

/// The day the active rates were taken from LiteLLM.
pub fn pricing_as_of() -> String {
    current().as_of.clone()
}

/// Whether upstream is worth asking, given the catalog already active.
///
/// LiteLLM dates its releases by the day, so one fetch a day is the most that
/// can tell this app anything new, and a rescan an hour later must not spend a
/// request finding that out.
pub fn refresh_needed(today: &str) -> bool {
    current().as_of.as_str() < today
}

/// The catalog's rates for one recorded model id, or nothing at all.
///
/// Resolution is exact-match first, then the same id with each leading
/// provider segment stripped (`anthropic/claude-opus-5` → `claude-opus-5`),
/// because a provider prefix names the same model rather than a different one.
/// Nothing beyond that: a fuzzy match onto a similarly named model would price
/// a session at rates it never ran at, which is the invented precision the
/// token figures beside it refuse to produce.
///
/// Returning nothing is a real and expected outcome — Codex's internal
/// `codex-auto-review` has no public price — and must read as "no price for
/// this model" rather than as free.
pub fn rates_for_model(model: &str) -> Option<Rates> {
    rates_in(&current(), model)
}

fn rates_in(snapshot: &Snapshot, model: &str) -> Option<Rates> {
    let table = &snapshot.usd_per_token;
    if let Some(rates) = table.get(model) {
        return Some(*rates);
    }
    let mut rest = model;
    while let Some((_, tail)) = rest.split_once('/') {
        if let Some(rates) = table.get(tail) {
            return Some(*rates);
        }
        rest = tail;
    }
    None
}

/// Activates a catalog an earlier refresh cached, when it is newer than the one
/// already active, and reports the date now in force.
pub fn load_cache(path: &Path) -> Result<String, CatalogError> {
    let snapshot: Snapshot = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    Ok(activate_if_newer(snapshot))
}

/// Releases the one-refresh-at-a-time guard when dropped, including on an
/// unwind, so a panic partway through a fetch cannot wedge every later
/// refresh into believing one is already running forever.
struct RefreshGuard(&'static AtomicBool);

impl Drop for RefreshGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Fetches upstream, caches the trim, and activates it.
///
/// Blocking, and meant for a background thread: this is the one place in the
/// pricing module that touches the network, and nothing the index does waits on
/// it. The cache is written before the swap so a later launch starts from these
/// rates even if the process ends immediately after.
pub fn refresh_from_upstream(cache_path: &Path, today: &str) -> Result<String, CatalogError> {
    // One refresh at a time. Two threads fetching the same 1.7 MB to write the
    // same file is waste at best and a half-written cache at worst.
    static REFRESHING: AtomicBool = AtomicBool::new(false);
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return Ok(pricing_as_of());
    }
    let _guard = RefreshGuard(&REFRESHING);
    fetch_trim_and_cache(cache_path, today)
}

fn fetch_trim_and_cache(cache_path: &Path, today: &str) -> Result<String, CatalogError> {
    let upstream = fetch_upstream()?;
    let snapshot = trim_upstream(&upstream, today)?;
    write_cache(cache_path, &snapshot)?;
    Ok(activate_if_newer(snapshot))
}

fn fetch_upstream() -> Result<String, CatalogError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .build()
        .into();
    agent
        .get(UPSTREAM_URL)
        .call()
        .map_err(|err| CatalogError::Fetch(err.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|err| CatalogError::Fetch(err.to_string()))
}

/// LiteLLM's whole catalog reduced to the rates a session can spend against:
/// every text-generation entry priced as a per-token input/output pair, from
/// every provider it prices.
///
/// Unlike the embedded subset, nothing is dropped by provider or by name. The
/// runtime copy costs a cache file and a request rather than binary size, so
/// narrowing it would only make a model this app has never seen unpriceable for
/// no saving that matters here.
fn trim_upstream(upstream: &str, as_of: &str) -> Result<Snapshot, CatalogError> {
    let catalog: HashMap<String, serde_json::Value> = serde_json::from_str(upstream)?;
    let mut usd_per_token = HashMap::new();
    for (id, entry) in &catalog {
        // Upstream ships a documentation entry beside the real models.
        if id == "sample_spec" {
            continue;
        }
        if let Some(rates) = rates_from_upstream_entry(entry) {
            usd_per_token.insert(id.clone(), rates);
        }
    }
    if usd_per_token.is_empty() {
        return Err(CatalogError::NothingPriced);
    }
    Ok(Snapshot {
        as_of: as_of.to_string(),
        usd_per_token,
    })
}

/// One upstream entry's rates, or nothing when it prices something other than
/// text generation by the token. An entry missing either side of the pair
/// cannot price a session, so it is dropped rather than half-kept.
fn rates_from_upstream_entry(entry: &serde_json::Value) -> Option<Rates> {
    let mode = entry.get("mode")?.as_str()?;
    if !TEXT_MODES.contains(&mode) {
        return None;
    }
    Some(Rates {
        input: rate(entry, "input_cost_per_token")?,
        output: rate(entry, "output_cost_per_token")?,
        cache_read: rate(entry, "cache_read_input_token_cost"),
        cache_write: rate(entry, "cache_creation_input_token_cost"),
    })
}

fn rate(entry: &serde_json::Value, key: &str) -> Option<f64> {
    entry.get(key)?.as_f64()
}

/// Writes the cache through a temporary file in the same directory, so a
/// process that dies mid-write leaves the previous cache intact rather than a
/// truncated one the next launch would refuse to parse.
fn write_cache(path: &Path, snapshot: &Snapshot) -> Result<(), CatalogError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.writing");
    std::fs::write(&temporary, serde_json::to_string(snapshot)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every model this corpus actually names resolves, which is what makes an
    /// estimate worth writing at all. Claude and Pi write bare ids; the catalog
    /// keys them the same way.
    #[test]
    fn prices_the_model_ids_the_harnesses_actually_record() {
        assert!(rates_for_model("claude-sonnet-4-5-20250929").is_some());
        assert!(rates_for_model("claude-opus-5").is_some());
    }

    /// Codex's models are `responses` mode upstream rather than `chat`, so a
    /// chat-only rate table would leave a whole harness unpriceable.
    #[test]
    fn prices_codex_models_which_the_catalog_files_apart_from_chat_models() {
        assert!(rates_for_model("gpt-5-codex").is_some());
        assert!(rates_for_model("gpt-5.6-sol").is_some());
    }

    /// The embedded file is a subset now, so the models the three harnesses
    /// name are the ones that have to survive the cut.
    #[test]
    fn the_embedded_subset_prices_every_model_the_three_harnesses_name() {
        for model in [
            "claude-opus-5",
            "claude-sonnet-5",
            "claude-fable-5",
            "claude-haiku-4-5",
            "claude-haiku-4-5-20251001",
            "claude-opus-4-8",
            "gpt-5.5",
            "gpt-5.6",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "gpt-5.4-mini",
            "gpt-5.1-codex",
        ] {
            assert!(
                rates_in(&embedded(), model).is_some(),
                "the embedded subset must price {model}"
            );
        }
    }

    #[test]
    fn reads_a_provider_prefixed_id_as_the_model_it_prefixes() {
        let bare = rates_for_model("claude-opus-5").expect("bare id resolves");
        for prefixed in ["anthropic/claude-opus-5", "some-gateway/claude-opus-5"] {
            let resolved = rates_for_model(prefixed).expect("prefixed id resolves");
            assert_eq!(resolved.input, bare.input);
            assert_eq!(resolved.output, bare.output);
        }
    }

    /// Codex's internal review model has no published price, and Claude's
    /// synthetic placeholder is not a model at all. Neither is guessed at from
    /// a similarly named neighbour.
    #[test]
    fn prices_nothing_it_has_no_rate_for() {
        assert!(rates_for_model("codex-auto-review").is_none());
        assert!(rates_for_model("<synthetic>").is_none());
    }

    #[test]
    fn the_snapshot_dates_itself_so_an_estimate_can_say_how_old_its_rates_are() {
        let as_of = pricing_as_of();
        assert_eq!(as_of.len(), 10, "expected an ISO date, got {as_of:?}");
    }

    fn upstream_fixture() -> String {
        serde_json::json!({
            "sample_spec": {
                "mode": "chat",
                "input_cost_per_token": 0.0,
                "output_cost_per_token": 0.0
            },
            "claude-opus-5": {
                "mode": "chat",
                "litellm_provider": "anthropic",
                "input_cost_per_token": 0.000005,
                "output_cost_per_token": 0.000025,
                "cache_read_input_token_cost": 0.0000005,
                "cache_creation_input_token_cost": 0.00000625
            },
            "fireworks_ai/some-open-model": {
                "mode": "chat",
                "litellm_provider": "fireworks_ai",
                "input_cost_per_token": 0.0000002,
                "output_cost_per_token": 0.0000008
            },
            "text-embedding-3-large": {
                "mode": "embedding",
                "litellm_provider": "openai",
                "input_cost_per_token": 0.00000013,
                "output_cost_per_token": 0.0
            },
            "priced-by-the-request": {
                "mode": "chat",
                "litellm_provider": "openai",
                "output_cost_per_token": 0.00001
            }
        })
        .to_string()
    }

    /// The runtime trim is deliberately wider than the embedded subset: a
    /// provider no harness here reads today still gets cached, because the
    /// runtime copy pays no binary-size cost for breadth.
    #[test]
    fn the_runtime_trim_keeps_every_providers_text_models() {
        let trimmed = trim_upstream(&upstream_fixture(), "2026-08-17").expect("a usable trim");

        assert_eq!(trimmed.as_of, "2026-08-17");
        let opus = rates_in(&trimmed, "claude-opus-5").expect("Anthropic priced");
        assert_eq!(opus.input, 0.000005);
        assert_eq!(opus.cache_read, Some(0.0000005));
        assert!(
            rates_in(&trimmed, "fireworks_ai/some-open-model").is_some(),
            "the runtime catalog is not narrowed to the embedded subset's providers"
        );
    }

    /// An entry that prices something other than text generation by the token
    /// would price a session at rates it never ran at, or at nothing.
    #[test]
    fn the_runtime_trim_drops_entries_that_cannot_price_a_session() {
        let trimmed = trim_upstream(&upstream_fixture(), "2026-08-17").expect("a usable trim");

        assert!(rates_in(&trimmed, "sample_spec").is_none(), "the doc entry");
        assert!(
            rates_in(&trimmed, "text-embedding-3-large").is_none(),
            "an embedding model spends nothing a session records"
        );
        assert!(
            rates_in(&trimmed, "priced-by-the-request").is_none(),
            "half a rate pair cannot price a session"
        );
    }

    #[test]
    fn upstream_that_prices_nothing_readable_is_refused_rather_than_activated() {
        let only_embeddings = serde_json::json!({
            "text-embedding-3-large": { "mode": "embedding", "input_cost_per_token": 0.1 }
        })
        .to_string();

        assert!(matches!(
            trim_upstream(&only_embeddings, "2026-08-17"),
            Err(CatalogError::NothingPriced)
        ));
        assert!(matches!(
            trim_upstream("not json", "2026-08-17"),
            Err(CatalogError::Parse(_))
        ));
    }

    fn temp_cache_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ailly-pricing-{}-{}-{}/litellm-snapshot.json",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ))
    }

    /// What a refresh writes has to be what a later launch can read, or the
    /// cache is a file that only ever costs a fetch.
    #[test]
    fn a_cached_catalog_reads_back_as_the_rates_that_were_written() {
        let path = temp_cache_path("cache-round-trip");
        let written = trim_upstream(&upstream_fixture(), "2099-01-01").expect("a usable trim");

        write_cache(&path, &written).expect("write the cache");
        let read: Snapshot =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read the cache"))
                .expect("the cache parses");

        assert_eq!(read.as_of, "2099-01-01");
        assert_eq!(
            rates_in(&read, "claude-opus-5").expect("priced").output,
            0.000025
        );
        let _ = std::fs::remove_dir_all(path.parent().expect("a cache directory"));
    }

    /// A cache older than the file the binary ships would walk the rates
    /// backwards on every launch, so the newer date wins whichever side it is
    /// on. Read through a snapshot pair rather than the process-wide catalog:
    /// activating a fixture globally would reprice every other test.
    #[test]
    fn the_newer_of_the_two_catalogs_is_the_one_that_prices() {
        let embedded_as_of = embedded().as_of.clone();

        assert!(
            "2099-01-01" > embedded_as_of.as_str(),
            "a cache dated after the embedded snapshot is newer"
        );
        assert!(
            "1970-01-01" < embedded_as_of.as_str(),
            "a cache dated before the embedded snapshot is older and is refused"
        );
    }

    /// One fetch a day is all upstream can answer, and a rescan an hour later
    /// must not spend a request learning that.
    #[test]
    fn a_refresh_is_only_worth_making_when_the_active_catalog_predates_today() {
        let as_of = pricing_as_of();

        assert!(!refresh_needed(&as_of), "already holding today's rates");
        assert!(!refresh_needed("1970-01-01"), "a clock behind the catalog");
        assert!(refresh_needed("2999-12-31"));
    }
}
