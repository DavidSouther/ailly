import { DEFAULT_ENGINE, getPlugin } from "../ailly";
import { Content, ContentMeta } from "../content/content";
import { RAG } from "../rag";

export interface PromptThreadsSummary {
  totalPrompts: number;
  finished: number;
  errors: number;
  done: boolean;
}

export interface PromptThreadSummary extends PromptThreadsSummary {
  isolated: boolean;
}

export class PromptThread {
  finished: number = 0;
  isolated: boolean = false;
  done: boolean = false;
  runner?: Promise<PromiseSettledResult<Content>[]>;
  // Results holds a list of errors that occurred and the index the occurred at.
  // If the thread is isolated, this can have many entries. If the thread is not
  // isolated, it will have at most one entry. In either case, if the list is
  // empty, all prompts in the thread generated successfully.
  errors: Array<[number, Error]> = [];

  get isDone() {
    return this.done;
  }
  get isSuccess() {
    return this.done && this.errors.length == 0;
  }
  get isError() {
    return this.done && this.errors.length > 0;
  }

  static run(content: Content[], settings: ContentMeta, rag: RAG) {
    const thread = new PromptThread(content, settings, rag);
    thread.start();
    return thread;
  }

  private constructor(
    private readonly content: Content[],
    private settings: ContentMeta,
    private rag: RAG
  ) {
    this.content = content;
    this.isolated = Boolean(content[0]?.meta?.isolated ?? false);
  }

  start() {
    this.runner = this.isolated ? this.runIsolated() : this.runSequence();
  }

  private runIsolated(): Promise<PromiseSettledResult<Content>[]> {
    console.log(`Running thread for ${this.content.length} isolated prompts`);
    const promises: Promise<Content>[] = this.content.map(
      async (c, i): Promise<Content> => {
        try {
          await this.rag.augment(c);
          await generateOne(c, this.settings);
          this.finished += 1;
        } catch (e) {
          console.warn("Error generating content", e);
          this.errors.push([i, e as Error]);
        }
        return c;
      }
    );

    return Promise.allSettled(promises).finally(() => {
      this.done = true;
    });
  }

  private async runSequence(): Promise<PromiseSettledResult<Content>[]> {
    console.log(
      `Running thread for sequence of ${this.content.length} prompts`
    );
    const results: PromiseSettledResult<Content>[] = [];
    for (let i = 0; i < this.content.length; i++) {
      const content = this.content[i];
      try {
        await this.rag.augment(content);
        await generateOne(content, this.settings);
        results.push({ status: "fulfilled", value: content } as const);
        this.finished += 1;
      } catch (e) {
        console.warn("Error generating content", e);
        this.errors.push([i, e as Error]);
        results.push({ status: "rejected", reason: e } as const);
        break;
      }
    }
    this.done = true;
    return results;
  }

  async allSettled(): Promise<PromiseSettledResult<Content>[]> {
    return this.runner ?? Promise.resolve([]);
  }

  summary(): PromptThreadSummary {
    return {
      totalPrompts: this.content.length,
      isolated: this.isolated,
      finished: this.finished,
      errors: this.errors.length,
      done: this.done,
    };
  }
}

async function generateOne(
  c: Content,
  settings: ContentMeta
): Promise<Content> {
  const no_overwrite = c.meta?.no_overwrite || settings.no_overwrite;
  const has_response = (c.response?.length ?? 0) > 0;
  if (no_overwrite && has_response) {
    return c;
  }

  // Determine PLUGIN and MODEL, load them.
  const meta = c.meta;
  const engineName = meta?.engine ?? settings.engine ?? DEFAULT_ENGINE;
  const engine = await getPlugin(engineName);
  engine.format([c]);
  const model = c.meta?.model ?? engine.DEFAULT_MODEL;

  console.log(
    `Calling ${engineName}`,
    meta?.messages?.map((m) => ({
      role: m.role,
      content: m.content.replaceAll("\n", "").substring(0, 50) + "...",
      tokens: m.tokens,
    }))
  );
  const generated = await engine.generate(c, { ...settings, model });
  c.response = generated.message;
  c.meta = { ...c.meta, debug: generated.debug };
  return c;
}
