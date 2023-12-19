import { PipelineSettings } from "../ailly.js";
import type { Content } from "../content/content";
import type { Engine } from "../engine";
import type { Plugin } from "../plugin";

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

  static run(
    content: Content[],
    settings: PipelineSettings,
    engine: Engine,
    rag: Plugin
  ) {
    const thread = new PromptThread(content, settings, engine, rag);
    thread.start();
    return thread;
  }

  private constructor(
    private readonly content: Content[],
    private settings: PipelineSettings,
    private engine: Engine,
    private rag: Plugin
  ) {
    this.content = content;
    this.isolated = Boolean(content[0]?.meta?.isolated ?? false);
  }

  start() {
    this.runner = this.isolated ? this.runIsolated() : this.runSequence();
  }

  private async runOne(c: Content, i: number): Promise<Content> {
    try {
      await this.rag.augment(c);
      await generateOne(c, this.settings, this.engine);
      this.finished += 1;
    } catch (e) {
      console.warn("Error generating content", e);
      this.errors.push([i, e as Error]);
      throw e;
    }
    return c;
  }

  private runIsolated(): Promise<PromiseSettledResult<Content>[]> {
    console.log(`Running thread for ${this.content.length} isolated prompts`);
    const promises: Promise<Content>[] = this.content.map(async (c, i) =>
      this.runOne(c, i).catch((_e) => c)
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
        await this.runOne(content, i);
        results.push({ status: "fulfilled", value: content } as const);
      } catch (e) {
        results.push({ status: "rejected", reason: e } as const);
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
  settings: PipelineSettings,
  engine: Engine
): Promise<Content> {
  const overwrite = c.meta?.skip ?? settings.overwrite;
  const has_response = (c.response?.length ?? 0) > 0;

  if (!overwrite && has_response) {
    return c;
  }

  // Determine PLUGIN and MODEL, load them.
  const meta = c.meta;
  engine.format([c]);

  console.log(
    `Calling ${engine.name}`,
    meta?.messages?.map((m) => ({
      role: m.role,
      content: m.content.replaceAll("\n", "").substring(0, 50) + "...",
      tokens: m.tokens,
    }))
  );
  const generated = await engine.generate(c, settings);
  c.response = generated.message;
  c.meta = {
    ...c.meta,
    debug: {
      ...generated.debug,
      engine: settings.engine,
      model: settings.model,
    },
  };
  return c;
}
