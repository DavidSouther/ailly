import { DEFAULT_LOGGER } from "@davidsouther/jiffies/lib/esm/log.js";
import { LocalIndex } from "vectra/lib/LocalIndex.js";
import type { Content } from "../content/content";
import type { Engine } from "../engine";
import { join } from "node:path";
import type { PipelineSettings } from "../ailly";

function ragDb(path: string) {
  return join(path, ".vectors");
}

export class RAG {
  index: LocalIndex;

  static async build(
    engine: Engine,
    { root: path }: PipelineSettings
  ): Promise<RAG> {
    const rag = new RAG(engine, ragDb(path));
    if (!(await rag.index.isIndexCreated())) await rag.index.createIndex();
    return rag;
  }

  static async empty(engine: Engine, path: string): Promise<RAG> {
    return new NoopRAG(engine, path);
  }

  protected constructor(readonly engine: Engine, path: string) {
    this.index = new LocalIndex(path);
  }

  protected async add(content: Content) {
    const text = content.prompt;
    const vector = await this.engine.vector(content.prompt, {});
    await this.index.insertItem({
      vector,
      metadata: { text, name: content.name, path: content.path },
    });
  }

  async update(content: Content[]) {
    const _content = [...content];
    await this.index.beginUpdate();
    await new Promise<void>(async (resolve, reject) => {
      const nextPiece = async () => {
        const piece = _content.pop()!;
        if (!piece) {
          return resolve();
        }
        try {
          DEFAULT_LOGGER.info(`Sending ${piece.name} (${piece.path})`);
          await this.add(piece);
          DEFAULT_LOGGER.info(`Completed ${piece.name} (${piece.path})`);
        } catch (e) {
          DEFAULT_LOGGER.info(`Error on ${piece.name} (${piece.path})`);
          DEFAULT_LOGGER.info(`${e}`);
        }
        nextPiece();
      };
      nextPiece();
    });
    await this.index.endUpdate();
  }

  async query(data: string, results = 3) {
    const vector = await this.engine.vector(data, {});
    const query = await this.index.queryItems(vector, results);
    return query.map(({ score, item }) => ({
      score,
      content: item.metadata.text as string,
      name: "unknown",
    }));
  }

  async augment(content: Content) {
    content.meta = content.meta ?? {};
    const results = await this.query(content.prompt);
    content.augment = results;
  }

  async clean(content: Content) {}
}

export class NoopRAG extends RAG {
  override update(content: Content[]): Promise<void> {
    return Promise.resolve();
  }
  override augment(content: Content): Promise<void> {
    return Promise.resolve();
  }

  override query(
    data: string,
    results?: number
  ): Promise<{ score: number; content: string; name: string }[]> {
    return Promise.resolve([]);
  }
}
