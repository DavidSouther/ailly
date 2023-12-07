import { LocalIndex } from "vectra";
import { Content } from "./content";
import { Plugin } from "./plugin";
import { join } from "node:path";

function ragDb(path: string) {
  return join(path, ".vectors");
}

export class RAG {
  index: LocalIndex;

  static async build(plugin: Plugin, path: string): Promise<RAG> {
    const rag = new RAG(plugin, ragDb(path));
    if (!(await rag.index.isIndexCreated())) await rag.index.createIndex();
    return rag;
  }

  static async empty(plugin: Plugin, path: string): Promise<RAG> {
    return new NoopRAG(plugin, path);
  }

  protected constructor(readonly plugin: Plugin, path: string) {
    this.index = new LocalIndex(path);
  }

  async add(content: Content) {
    const text = content.prompt;
    const vector = await this.plugin.vector(content.prompt, {});
    await this.index.insertItem({
      vector,
      metadata: { text, name: content.name, path: content.path },
    });
  }

  async query(data: string, results = 3) {
    const vector = await this.plugin.vector(data, {});
    const query = await this.index.queryItems(vector, results);
    return query.map(({ score, item }) => ({
      score,
      content: item.metadata.text as string,
    }));
  }

  async augment(content: Content) {
    content.meta = content.meta ?? {};
    const results = await this.query(content.prompt);
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

  override query(
    data: string,
    results?: number
  ): Promise<{ score: number; content: string }[]> {
    return Promise.resolve([]);
  }
}
