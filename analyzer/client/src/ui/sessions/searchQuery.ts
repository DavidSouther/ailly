import type { Harness } from "../../tauri";
import { HARNESS_LABEL } from "./format";

export type ParsedSearch = {
  harnessValues: string[];
  residual: string;
};

export function parseSearchQuery(search: string): ParsedSearch {
  const harnessValues: string[] = [];
  const residual = search
    .replace(/harness:\s*(\S+)/gi, (_, value: string) => {
      harnessValues.push(value);
      return " ";
    })
    .replace(/\s+/g, " ")
    .trim();
  return { harnessValues, residual };
}

function normalizeHarnessToken(value: string): string {
  return value.toLowerCase().replace(/[_\-\s]+/g, "_");
}

const HARNESS_ALIASES: Map<string, Harness> = (() => {
  const aliases = new Map<string, Harness>();
  for (const [id, label] of Object.entries(HARNESS_LABEL) as [Harness, string][]) {
    aliases.set(normalizeHarnessToken(id), id);
    aliases.set(normalizeHarnessToken(label), id);
  }
  return aliases;
})();

/** Map a token value to a harness id, or null if unrecognized. */
export function resolveHarnessValue(value: string): Harness | null {
  return HARNESS_ALIASES.get(normalizeHarnessToken(value)) ?? null;
}

export type ParsedHarnessFilter = {
  /** `null` = no harness filter; array = restrict to exactly those (empty = match nothing). */
  harnesses: Harness[] | null;
  residual: string;
};

export function parseHarnessFilter(search: string): ParsedHarnessFilter {
  const { harnessValues, residual } = parseSearchQuery(search);
  if (harnessValues.length === 0) {
    return { harnesses: null, residual };
  }
  const harnesses: Harness[] = [];
  for (const value of harnessValues) {
    const harness = resolveHarnessValue(value);
    if (harness !== null && !harnesses.includes(harness)) {
      harnesses.push(harness);
    }
  }
  return { harnesses, residual };
}

export type HarnessFilterChipProps = {
  label: string;
  onRemove: () => void;
};

/** Remove the nth `harness:` token (0-based among harness tokens) from search. */
export function removeHarnessTokenAt(search: string, index: number): string {
  let i = 0;
  return search
    .replace(/harness:\s*(\S+)/gi, (match) => (i++ === index ? " " : match))
    .replace(/\s+/g, " ")
    .trim();
}
