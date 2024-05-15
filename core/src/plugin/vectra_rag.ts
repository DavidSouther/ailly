import { join } from "node:path";
import { LocalIndex } from "vectra/lib/LocalIndex.js";
import { RAG } from "./rag.js";
import { Engine } from "../engine/index.js";
import { LOGGER, PipelineSettings } from "../ailly.js";
import { Content } from "../content/content.js";

function ragDb(path: string) {
  return join(path, ".vectors");
}

export class VectraRAG extends RAG {
  index: LocalIndex;

  static async build(
    engine: Engine,
    { root: path }: PipelineSettings
  ): Promise<RAG> {
    const rag = new VectraRAG(engine, ragDb(path));
    if (!(await rag.index.isIndexCreated())) await rag.index.createIndex();
    return rag;
  }

  protected constructor(readonly engine: Engine, path: string) {
    super(engine);
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

  override async update(content: Content[]) {
    const _content = [...content];
    await this.index.beginUpdate();
    await new Promise<void>(async (resolve, reject) => {
      const nextPiece = async () => {
        const piece = _content.pop()!;
        if (!piece) {
          return resolve();
        }
        try {
          LOGGER.info(`Adding to RAG ${piece.name} (${piece.path})`);
          await this.add(piece);
          LOGGER.info(`Completed adding to RAG ${piece.name} (${piece.path})`);
        } catch (e) {
          LOGGER.warn(`Error adding to RAG ${piece.name} (${piece.path})`);
          LOGGER.info(`${e}`);
        }
        nextPiece();
      };
      nextPiece();
    });
    await this.index.endUpdate();
  }

  override async query(data: string, results = 3) {
    const vector = await this.engine.vector(data, {});
    const query = await this.index.queryItems(vector, results);
    return query.map(({ score, item }) => ({
      score,
      content: item.metadata.text as string,
      name: "unknown",
    }));
  }

  override async augment(content: Content) {
    content.meta = content.meta ?? {};
    const results = await this.query(content.prompt);
    content.context.augment = results;
  }

  override async clean(content: Content) {}
}
