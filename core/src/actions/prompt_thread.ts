import { DEFAULT_SCHEDULER_LIMIT, LOGGER, PipelineSettings } from "../index.js";
import { View, type Content } from "../content/content.js";
import type { Engine } from "../engine/index.js";
import type { Plugin } from "../plugin/index.js";
import {
  GLOBAL_VIEW,
  mergeContentViews,
  mergeViews,
} from "../content/template.js";

export interface PromptThreadsSummary {
  totalPrompts: number;
  finished: number;
  errors: number;
  done: boolean;
}

export interface PromptThreadSummary extends PromptThreadsSummary {
  isolated: boolean;
}

export async function scheduler<T>(
  taskQueue: Array<() => Promise<T>>,
  limit: number = DEFAULT_SCHEDULER_LIMIT
): Promise<PromiseSettledResult<T>[]> {
  taskQueue = [...taskQueue].reverse();
  let finished: Array<Promise<T>> = [];
  let outstanding = new Set<Promise<T>>();
  while (taskQueue.length > 0) {
    if (outstanding.size >= limit) {
      // Wait for something in outstanding to finish
      await Promise.race([...outstanding]);
    } else {
      const task = taskQueue.pop();
      if (task) {
        const run = task();
        finished.push(run);
        outstanding.add(run);
        run.finally(() => outstanding.delete(run));
      }
    }
  }
  return Promise.allSettled(finished);
}

export class PromptThread {
  finished: number = 0;
  isolated: boolean = false;
  private done: boolean = false;
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

  private view: undefined | View = undefined;

  static run(
    content: Content[],
    context: Record<string, Content>,
    settings: PipelineSettings,
    engine: Engine,
    rag: Plugin
  ) {
    const thread = new PromptThread(content, context, settings, engine, rag);
    thread.start();
    return thread;
  }

  private constructor(
    private readonly content: Content[],
    private readonly context: Record<string, Content>,
    private settings: PipelineSettings,
    private engine: Engine,
    private plugin: Plugin
  ) {
    this.content = content;
    this.isolated = Boolean(settings.isolated ?? false);
  }

  start() {
    this.runner = this.isolated ? this.runIsolated() : this.runSequence();
    this.runner.catch((err) => {
      LOGGER.error("Error in prompt thread", { err });
    });
  }

  private async runOne(c: Content, i: number): Promise<Content> {
    if (this.view === undefined) {
      const engineView = (await this.engine.view?.().catch(() => ({}))) ?? {};
      const pluginView = (await this.plugin.view?.().catch(() => ({}))) ?? {};
      this.view = mergeViews(
        GLOBAL_VIEW,
        engineView,
        pluginView,
        this.settings.templateView
      );
    }
    try {
      await this.template(c, this.view);
      await this.plugin.augment(c);
      await generateOne(c, this.context, this.settings, this.engine);
      await this.plugin.clean(c);
      this.finished += 1;
    } catch (err) {
      const { message, stack } = err as Error;
      LOGGER.warn("Error generating content", { err: { message, stack } });
      this.errors.push([i, err as Error]);
      throw err;
    }
    return c;
  }

  private async template(c: Content, view: View) {
    mergeContentViews(c, view, this.context);
  }

  private runIsolated(): Promise<PromiseSettledResult<Content>[]> {
    LOGGER.debug(`Running thread for ${this.content.length} isolated prompts`);
    return scheduler(
      this.content.map((c, i) => () => this.runOne(c, i)),
      this.settings.requestLimit
    ).finally(() => (this.done = true));
  }

  private async runSequence(): Promise<PromiseSettledResult<Content>[]> {
    LOGGER.debug(
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

export function generateOne(
  c: Content,
  context: Record<string, Content>,
  settings: PipelineSettings,
  engine: Engine
): Promise<void> {
  const has_response = (c.response?.length ?? 0) > 0;

  if (c.meta?.skip || (!settings.overwrite && has_response)) {
    LOGGER.info(`Skipping ${c.path}`);
    const stream = new TextEncoderStream();
    stream.writable.getWriter().write(c.response ?? "");
    c.responseStream.resolve(
      stream.readable.pipeThrough(new TextDecoderStream())
    );
    return Promise.resolve();
  }

  LOGGER.info(`Preparing ${c.path}`);

  const meta = c.meta;
  engine.format([c], context);

  LOGGER.info(`Calling ${engine.name}`);
  LOGGER.debug(`Generating response`, {
    engine: engine.name,
    messages: meta?.messages
      ?.map((m) => ({
        role: m.role,
        content: m.content,
        // tokens: m.tokens,
      }))
      // Skip the last `assistant` message
      .filter((m, i, a) => !(m.role == "assistant" && i === a.length - 1)),
  });
  c.meta = {
    ...c.meta,
    debug: {
      engine: settings.engine,
      model: settings.model,
    },
  };
  try {
    const generator = engine.generate(c, settings);
    c.responseStream.resolve(generator.stream);
    return generator.done.then(
      () => {
        c.response = generator.message();
        c.meta!.debug = { ...c.meta!.debug, ...generator.debug() };
      },
      (err) => {
        c.meta!.debug = { ...c.meta!.debug, ...generator.debug() };
      }
    );
  } catch (err) {
    LOGGER.error(`Uncaught error in ${engine.name} generator`, { err });
    return Promise.resolve();
  }
}

export async function drain(content: Content) {
  const stream = await content.responseStream.promise;
  if (stream.locked) {
    return;
  }
  for await (const _ of stream) {
  }
}
