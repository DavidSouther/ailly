import { type Harness, type SourceValue, type TokenUsage, isRecorded } from "../../tauri";

/**
 * The four disjoint buckets every figure in the Tokens lens is built from.
 * Normalizing into these first is what keeps one arithmetic correct across
 * three harnesses whose usage records mean different things by "input".
 *
 * They live apart from the rollup because both the token fold and the price fold
 * are built on them, and neither should have to import the other to get at them.
 */
export interface TokenBuckets {
  freshInput: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}

export const EMPTY_BUCKETS: TokenBuckets = {
  freshInput: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
};

/** A recorded figure, or nothing at all — never a zero the source did not write. */
function recordedAmount(value: SourceValue<number>): number {
  return isRecorded(value) ? value.Recorded : 0;
}

/**
 * One record's usage, normalized per harness. Pure: no I/O, no descendant walk.
 *
 * Claude and Pi write four figures that sit beside one another, so each is
 * already the bucket it looks like. Codex counts both of its cache figures
 * *inside* `input_tokens`, so reading its input straight would count the cached
 * portion twice. Both relations are settled by the membership tests in the Rust
 * adapters rather than assumed here.
 *
 * The harness's own `total` is deliberately never read: every figure the lens
 * shows is derived from these parts, so a composition can never disagree with
 * the total above it.
 */
export function bucketsFromUsage(usage: TokenUsage, harness: Harness): TokenBuckets {
  const input = recordedAmount(usage.input);
  const cacheRead = recordedAmount(usage.cache_read);
  const cacheWrite = recordedAmount(usage.cache_write);
  return {
    // A record whose cache figures exceed its input would derive a negative
    // remainder; the recorded cache figures are kept and only the derivation is
    // floored, since negative spend is not a fact any harness recorded.
    freshInput: harness === "codex" ? Math.max(0, input - cacheRead - cacheWrite) : input,
    output: recordedAmount(usage.output),
    cacheRead,
    cacheWrite,
  };
}

export function bucketTotal(buckets: TokenBuckets): number {
  return buckets.freshInput + buckets.output + buckets.cacheRead + buckets.cacheWrite;
}

export function addBuckets(left: TokenBuckets, right: TokenBuckets): TokenBuckets {
  return {
    freshInput: left.freshInput + right.freshInput,
    output: left.output + right.output,
    cacheRead: left.cacheRead + right.cacheRead,
    cacheWrite: left.cacheWrite + right.cacheWrite,
  };
}
