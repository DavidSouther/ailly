import type { Content } from "../content/content.js";
import type { Engine } from "../engine/index.js";

export class RAG {
  static async empty(engine: Engine): Promise<RAG> {
    return new RAG(engine);
  }

  protected constructor(readonly engine: Engine) {}

  update(content: Content[]): Promise<void> {
    return Promise.resolve();
  }

  augment(content: Content): Promise<void> {
    return Promise.resolve();
  }

  query(
    data: string,
    results?: number,
  ): Promise<{ score: number; content: string; name: string }[]> {
    return Promise.resolve([]);
  }

  async clean(content: Content) {}
}
