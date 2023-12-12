import { FileSystem } from "@davidsouther/jiffies/lib/esm/fs";
import { Content } from "./content";
import { Plugin } from "./plugin";
import { join } from "node:path";
import { brotliDecompressSync } from "node:zlib";

export interface Ragger {
  add(content: Content): Promise<void>;
  query(
    data: string,
    filter: (name: string) => boolean
  ): AsyncIterator<{ name: string; score: number; content: string }>;
  augment(
    content: Content,
    filter: (name: string) => boolean,
    stop: (_: { score: number; content: string; name: string }) => boolean
  ): Promise<void>;
}

interface RagIndex {
  items: Array<{ vector: Float32Array; norm: number; name: string }>;
}

export class RAG {
  index!: RagIndex;

  static async build(
    plugin: Plugin,
    path: string,
    fs: FileSystem
  ): Promise<Ragger> {
    const rag = new RAG(plugin, path, fs);
    await rag.loadIndex();
    return rag;
  }

  private async loadIndex() {
    const vectorJson = await this.fs.readFile(join(this.path, "index.json"));
    const vectorDb = JSON.parse(vectorJson) as {
      items: Array<{
        norm: number;
        vector: string;
        metadata: { name: string };
      }>;
    };
    this.index = {
      items: vectorDb.items.map((item) => {
        const buffer = Buffer.from(item.vector, "base64"); // Ta-da
        const expanded = brotliDecompressSync(buffer);
        const bytes = new Uint8Array(expanded);
        const vector = new Float32Array(bytes.buffer);
        return {
          vector,
          norm: item.norm,
          name: item.metadata.name,
        };
      }),
    };
  }

  static async empty(
    plugin: Plugin,
    path: string,
    fs: FileSystem
  ): Promise<RAG> {
    return new NoopRAG(plugin, path, fs);
  }

  protected constructor(
    readonly plugin: Plugin,
    private readonly path: string,
    private readonly fs: FileSystem
  ) {}

  async add(content: Content) {
    const numVec = await this.plugin.vector(content.prompt, {});
    const vector = new Float32Array(numVec);

    await this.index.items.push({
      vector,
      norm: f32aNorm(vector),
      name: content.name,
    });
  }

  async *query(data: string, filter: (name: string) => boolean = (_) => true) {
    const nums = await this.plugin.vector(data, {});
    const vector = new Float32Array(nums);
    const norm = f32aNorm(vector);
    const items = this.index.items
      .filter((i) => filter(i.name))
      .map((i) => ({
        ...i,
        distance: similarity(i.vector, i.norm, vector, norm),
      }));
    items.sort((a, b) => b.distance - a.distance);

    for (const item of items) {
      const content = await this.fs.readFile(join(this.path, item.name));
      yield { name: item.name, score: item.distance, content };
    }
  }

  async augment(
    content: Content,
    filter: (name: string) => boolean,
    stop: (_: { score: number; content: string; name: string }) => boolean
  ) {
    content.meta = content.meta ?? {};
    const query = this.query(content.prompt, filter);
    const results: Array<{ score: number; content: string }> = [];

    let cont = true;
    while (cont) {
      const next = await query.next();
      if (next.done) {
        cont = false;
        continue;
      }
      results.push(next.value);
      cont = stop(next.value);
    }

    content.meta.augment = results;
  }
}

export class NoopRAG extends RAG {
  override add(content: Content): Promise<void> {
    return Promise.resolve();
  }
  override augment(content: Content): Promise<void> {
    return Promise.resolve();
  }

  async *query(
    data: string,
    filter?: (name: string) => boolean
  ): AsyncGenerator<
    { name: string; score: number; content: string },
    void,
    unknown
  > {
    return;
  }
}

const STRIDE = 8;
export function f32aNorm(arr: Float32Array): number {
  let norm = 0;
  const remainder = arr.length % STRIDE;
  const q = arr.length - remainder;
  let i = 0;
  while (i < q) {
    norm += arr[i + 0] * arr[i + 0];
    norm += arr[i + 1] * arr[i + 1];
    norm += arr[i + 2] * arr[i + 2];
    norm += arr[i + 3] * arr[i + 3];
    norm += arr[i + 4] * arr[i + 4];
    norm += arr[i + 5] * arr[i + 5];
    norm += arr[i + 6] * arr[i + 6];
    norm += arr[i + 7] * arr[i + 7];
    i += 8;
  }

  while (i < arr.length) {
    norm += arr[i] * arr[i];
    i += 1;
  }

  return Math.sqrt(norm);
}

export function similarity(
  a: Float32Array,
  an: number,
  b: Float32Array,
  bn: number
): number {
  let dot = 0;
  const norm = an * bn;
  const length = Math.min(a.length, b.length);
  const remainder = length % STRIDE;
  const q = length - remainder;
  let i = 0;
  while (i < q) {
    dot += (a[i + 0] * b[i + 0]) / norm;
    dot += (a[i + 1] * b[i + 1]) / norm;
    dot += (a[i + 2] * b[i + 2]) / norm;
    dot += (a[i + 3] * b[i + 3]) / norm;
    dot += (a[i + 4] * b[i + 4]) / norm;
    dot += (a[i + 5] * b[i + 5]) / norm;
    dot += (a[i + 6] * b[i + 6]) / norm;
    dot += (a[i + 7] * b[i + 7]) / norm;
    i += STRIDE;
  }
  while (i < length) {
    dot += (a[i] * b[i]) / norm;
    i += 1;
  }
  return dot;
}
