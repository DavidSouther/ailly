let get_encoding: any;
let encoding: any;
export async function encode(src: string) {
  try {
    if (!get_encoding) {
      get_encoding = await import("@dqbd/tiktoken");
    }
    if (!encoding) {
      encoding = get_encoding("cl100k_base");
    }
    return encoding.encode(src);
  } catch {
    // Encoding failed, probably because of a WASM issue. TODO figure out getting
    // WASM into the extension, but fall back to words as tokens.
    return Promise.resolve(src.split(/\s+/));
  }
}
