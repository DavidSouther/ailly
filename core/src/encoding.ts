import type { Tiktoken, get_encoding as get_encoding_fn } from "@dqbd/tiktoken";
let get_encoding: typeof get_encoding_fn | undefined;
let encoding: Tiktoken | undefined;
export async function encode(src: string) {
  try {
    if (!encoding) {
      if (!get_encoding) {
        get_encoding = (await import("@dqbd/tiktoken")).get_encoding;
      }
      encoding = get_encoding("cl100k_base");
    }
    return encoding.encode(src);
  } catch {
    // Encoding failed, probably because of a WASM issue. TODO figure out getting
    // WASM into the extension, but fall back to words as tokens.
    return Promise.resolve(src.split(/\s+/));
  }
}
