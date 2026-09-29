/**
 * Regenerates the rate table the Rust index embeds, from LiteLLM upstream.
 *
 * Two subsets come out of one trim, because the committed file and the runtime
 * cache answer different questions. `--subset embedded` writes the file
 * `shell/src/index/pricing/catalog.rs` compiles in: it pays binary size and
 * repository history for its breadth, and it only has to price the model
 * families the three harnesses this app reads can name. `--subset full` writes
 * what a running app fetches for itself, which pays neither cost and so keeps
 * every provider LiteLLM prices.
 *
 * The subset rule and the measurements behind it are in
 * `.ailly/developer/2026-08-14-A-review-token-usage/research/pricing-subset.md`.
 * The full trim is duplicated in `catalog.rs` so a running app can refresh
 * without Node; change one and change the other.
 *
 * Usage, from the repository root:
 *
 *   mise run pull-litellm-snapshot
 *   npm --prefix scripts run pull-litellm-snapshot -- --subset full --out /tmp/full.json
 *   npm --prefix scripts run pull-litellm-snapshot -- --from /tmp/upstream.json
 */

import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const UPSTREAM_URL =
  "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

/**
 * The modes that name a text-generating model. An embedding, rerank, or
 * image-generation entry prices something no session in this app spends.
 */
const TEXT_MODES = new Set(["chat", "responses", "completion"]);

/** Providers whose whole text catalog the embedded subset keeps. */
const EMBEDDED_PROVIDERS = new Set([
  "anthropic",
  "openai",
  "gemini",
  "vertex_ai-language-models",
  "vertex_ai-anthropic_models",
  "xai",
]);

/**
 * Model families the harnesses name, matched on the id itself so a model
 * released after this file was last regenerated still has a chance to resolve.
 *
 * Only ids that carry no `provider/` prefix are matched this way. A prefixed
 * `azure/gpt-4o` would be redundant: `rates_for_model` strips leading provider
 * segments before it gives up, so the unprefixed key already answers it.
 */
const EMBEDDED_ID_PATTERN = /^(claude|gpt|o[1345]-|codex|gemini|grok)/;

const EMBEDDED_OUT = new URL("../shell/src/index/pricing/litellm-snapshot.json", import.meta.url);

type Subset = "embedded" | "full";

interface Rates {
  input: number;
  output: number;
  cacheRead?: number;
  cacheWrite?: number;
}

interface Snapshot {
  asOf: string;
  usdPerToken: Record<string, Rates>;
}

interface Options {
  subset: Subset;
  out: string;
  from: string | null;
}

function parseArgs(argv: string[]): Options {
  let subset: Subset = "embedded";
  let out: string | null = null;
  let from: string | null = null;

  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = argv[index + 1];
    switch (flag) {
      case "--subset":
        if (value !== "embedded" && value !== "full") {
          throw new Error(`--subset takes "embedded" or "full", got ${value ?? "nothing"}`);
        }
        subset = value;
        index += 1;
        break;
      case "--out":
        if (value === undefined) {
          throw new Error("--out takes a path");
        }
        out = value;
        index += 1;
        break;
      case "--from":
        if (value === undefined) {
          throw new Error("--from takes a path");
        }
        from = value;
        index += 1;
        break;
      default:
        throw new Error(`unknown argument ${flag}`);
    }
  }

  return { subset, out: out ?? fileURLToPath(EMBEDDED_OUT), from };
}

async function upstreamCatalog(from: string | null): Promise<Record<string, unknown>> {
  const text = from === null ? await fetchUpstream() : await readFile(from, { encoding: "utf8" });
  const parsed: unknown = JSON.parse(text);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    throw new Error("upstream catalog is not a JSON object of model entries");
  }
  return parsed as Record<string, unknown>;
}

async function fetchUpstream(): Promise<string> {
  const response = await fetch(UPSTREAM_URL);
  if (!response.ok) {
    throw new Error(`${UPSTREAM_URL} answered ${response.status} ${response.statusText}`);
  }
  return response.text();
}

function numberOr(entry: Record<string, unknown>, key: string): number | undefined {
  const value = entry[key];
  return typeof value === "number" ? value : undefined;
}

/**
 * One entry's rates, or nothing when it prices something other than text
 * generation by the token. An entry missing either side of the pair cannot
 * price a session at all, so it is dropped rather than half-kept.
 */
function ratesFrom(entry: unknown): Rates | null {
  if (typeof entry !== "object" || entry === null) {
    return null;
  }
  const fields = entry as Record<string, unknown>;
  const mode = fields.mode;
  if (typeof mode !== "string" || !TEXT_MODES.has(mode)) {
    return null;
  }
  const input = numberOr(fields, "input_cost_per_token");
  const output = numberOr(fields, "output_cost_per_token");
  if (input === undefined || output === undefined) {
    return null;
  }
  const cacheRead = numberOr(fields, "cache_read_input_token_cost");
  const cacheWrite = numberOr(fields, "cache_creation_input_token_cost");
  return {
    input,
    output,
    ...(cacheRead === undefined ? {} : { cacheRead }),
    ...(cacheWrite === undefined ? {} : { cacheWrite }),
  };
}

function keptByEmbeddedSubset(id: string, entry: unknown): boolean {
  const provider = (entry as Record<string, unknown>).litellm_provider;
  return (
    (typeof provider === "string" && EMBEDDED_PROVIDERS.has(provider)) ||
    (!id.includes("/") && EMBEDDED_ID_PATTERN.test(id))
  );
}

function trim(catalog: Record<string, unknown>, subset: Subset): Snapshot {
  const usdPerToken: Record<string, Rates> = {};
  // Sorted so regenerating on an unchanged upstream produces an unchanged file,
  // and `sample_spec` dropped because upstream ships it as documentation rather
  // than as a model.
  const ids = Object.keys(catalog)
    .filter((id) => id !== "sample_spec")
    .sort();
  for (const id of ids) {
    const entry = catalog[id];
    const rates = ratesFrom(entry);
    if (rates === null) {
      continue;
    }
    if (subset === "embedded" && !keptByEmbeddedSubset(id, entry)) {
      continue;
    }
    usdPerToken[id] = rates;
  }
  return { asOf: todayUtc(), usdPerToken };
}

function todayUtc(): string {
  return new Date().toISOString().slice(0, 10);
}

async function main(): Promise<void> {
  const options = parseArgs(process.argv.slice(2));
  const snapshot = trim(await upstreamCatalog(options.from), options.subset);
  const count = Object.keys(snapshot.usdPerToken).length;
  if (count === 0) {
    throw new Error("the trim kept no models, which would leave every session unpriced");
  }
  // No indent: this file is data the crate embeds, not a file anyone reads by
  // hand, and pretty-printing it costs about eight times its own size.
  const serialized = `${JSON.stringify(snapshot)}\n`;
  await writeFile(options.out, serialized, { encoding: "utf8" });
  console.log(
    `${options.subset} subset: ${count} models, ${Buffer.byteLength(serialized)} bytes, asOf ${
      snapshot.asOf
    } → ${options.out}`,
  );
}

main().catch((error: unknown) => {
  console.error(`pull-litellm-snapshot: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
