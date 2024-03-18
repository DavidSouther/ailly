/**
 * FileOverview: A very simplistic alternative to Redis, that's backed by a file for persistence.
 *
 * In next.js, lambda, etc you _can_ keep state in memory between handler calls. However, if the file
 * gets unloaded or the process dies for any reason, you lost that state. Redis and Memcache are the
 * canonical solutions, and should be used in production. However, for Node, these are typically run
 * as sidecars as the node bindings are only for the client, not the engine. This class extends Map
 * to keep [string, string] data directly in memory and write it to the backing file.
 */

import { WriteStream } from "fs";
import { FileHandle, open } from "fs/promises";
import { join } from "path";

// The lifecycle states a MemBackedStore goes through. It starts ready. It goes in and out
// of compacting when `compact` gets called. `closing` and `closed` means that the instance
// is finished, and you'll need a new instance to make progress.
type STATUS = "ready" | "compacting" | "closing" | "closed";

/**
 * MemBackedStore is an in-memory Map<string,string> that streams updates to a FileHandle.
 * Data is stored line-oriented `[key]=[value]\n`. This implies keys cannot contain an '=',
 * and values cannot contain an `\n`. This is checked during `.set`, and will result in a
 * thrown error.
 */
export class MemBackedStore extends Map<string, string> {
  // Create a MemBackedStore, reading `path` for values and then appending to it.
  // The file will be created if it does not exist.
  static async open(path: string = join(process.cwd(), ".mem_backed_store")) {
    const data = await readDbFile(path);

    try {
      let file = await open(path, "a+");
      return new MemBackedStore(data, path, file);
    } catch (e) {
      console.error(`Failed to open ${path} for 'a+'`);
      throw e;
    }
  }

  get status(): STATUS {
    return this._status;
  }

  private _status: STATUS = "ready";
  private stream: WriteStream;
  // You cannot create a MemBackedStore directly, use `MemBackedStore.open()`.
  private constructor(
    data: [string, string][],
    private readonly path: string,
    private handle: FileHandle
  ) {
    // The Map constructor is defined in terms of calling `self.set`, which would
    // of course call this instances' `set`, which would trigger `check` before
    // the class is `ready`, and besides, would result in duplicate lines in the
    // backing file on every load. So we take the hit and call super.set ourselves.
    // The alternative is having an internal map and delegating to that. It's 50/50.
    super();
    data.forEach(([k, v]) => super.set(k, v));
    this.stream = this.handle.createWriteStream();
    process.on("beforeExit", () => this.close());
  }

  // Called from the public modifying methods. Throws an Error if the Store isn't
  // in the `ready` state.
  private check() {
    if (this.status != "ready") {
      throw new Error(
        `MemBackedStore is not ready, instead it is ${this.status}`
      );
    }
  }

  private write(k: string, v: string) {
    // this.check(); // DO NOT CALL CHECK HERE! Write gets called during compaction, so instead keep `write` private.
    if (!this.stream?.writable) {
      console.warn(
        "MemBackedStore lost the write stream, not persisting changes"
      );
    } else {
      this.stream.write(`${k}=${v}\n`);
    }
  }

  // Truncate the data file and write a single copy of each map entry.
  // Attempts to set data during this phase will result in an error for the writer,
  // but will not interrupt compaction and reads will still succeed with current data.
  // If the process ends (or the store is GCed) while compaction is in progress,
  // yeah, you'll lose data. But this is ephemeral memory data, so you shouldn't care.
  async compact(closing = false) {
    DEFAULT_LOGGER.debug("Compacting MemBackedStore");
    this._status = "compacting";
    this.handle.close();
    try {
      this.handle = await open(this.path, "w");
    } catch (e) {
      console.error(`Failed to open ${this.path} for 'w'`);
      throw e;
    }
    this.stream = this.handle.createWriteStream();
    this.forEach((v, k) => this.write(k, v));
    this._status = closing ? "closing" : "ready";
    DEFAULT_LOGGER.debug(`Finished compacting, now ${this._status}`);
  }

  // Close the store, with default behavior of compacting first.
  async close(compact = true) {
    DEFAULT_LOGGER.debug("Closing MemBackedStore");
    if (compact) await this.compact(true);
    await new Promise<void>((resolve, reject) =>
      this.stream.close((e) => {
        if (e) reject(e);
        else resolve();
      })
    );
    await this.handle.close();
    this._status = "closed";
  }

  // Clear all data (and compact the db to an empty file).
  clear(): void {
    this.check();
    super.clear();
    this.compact();
  }

  // "Delete" a key in memory, and set its value to "" in the db file. When the
  // file is next loaded, "" will be ignored during data load. When compacted
  // after that, the key will not be in the db at all.
  delete(key: string): boolean {
    this.check();
    if (!super.has(key)) {
      return false;
    }
    super.delete(key);
    this.write(key, "");
    return true;
  }

  // Set the value v to the key k. k may not include an '=', and v may not contain
  // a newline '\n'. The write will be scheduled, but not completed until a later
  // event loop.
  set(k: string, v: string): this {
    this.check();
    if (k.includes("="))
      throw new Error(`Invalid Key: "${k}" cannot contain '='`);
    if (v.includes("\n"))
      throw new Error(`Invalid value: "${v}" cannot contain '\\n'`);
    super.set(k, v);
    this.write(k, v);
    return this;
  }
}

// Read a memory db file. Each line (separated by newline '\n') is one entry.
// The key is separated from the value with a single `=`; the value may have more '=' in it.
// Lines are read in order, and set in order. Therefore, later key entries will overwrite
// earlier entries when creating the next MemBackedStore.
async function readDbFile(path: string): Promise<[string, string][]> {
  let file;
  try {
    file = await open(path, "r+");
  } catch (e) {
    return [];
  }

  const data: [string, string][] = [];

  let lineNo = 0;
  for await (const line of file?.readLines() ?? []) {
    const entry = line.split("=", 2) as [string, string];
    if (!(entry[0] && entry[1])) {
      console.warn(`Invalid key value pair at ${path}:${lineNo} ${line}`);
    } else {
      data.push(entry);
    }
    lineNo += 1;
  }

  await file?.close();
  return data;
}
