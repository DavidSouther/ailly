import { dirname } from "node:path";

import EventEmitter from "node:events";
import type { Content } from "../content/content";
import { type Engine, getEngine } from "../engine/index.js";
import {
  DEFAULT_ENGINE,
  LOGGER,
  type PipelineSettings,
  type Thread,
  getPlugin,
} from "../index.js";
import type { Plugin } from "../plugin/index.js";
import { type PromiseWithResolvers, withResolvers } from "../util.js";
import {
  PromptThread,
  type PromptThreadSummary,
  type PromptThreadsSummary,
  drain,
} from "./prompt_thread.js";

export class GenerateManager {
  done = false;
  settled: PromiseWithResolvers<PromiseSettledResult<Content>[]> =
    withResolvers();
  started = false;
  threads: Thread[];
  threadRunners: PromptThread[] = [];
  events = new EventEmitter();

  static async from(
    content: string[],
    context: Record<string, Content>,
    settings: PipelineSettings,
  ): Promise<GenerateManager> {
    const engineName = settings?.engine ?? DEFAULT_ENGINE;
    const engine = await getEngine(engineName);
    const pluginBuilder = await getPlugin(settings.plugin);
    const plugin = await pluginBuilder.default(engine, settings);
    return new GenerateManager(content, context, settings, engine, plugin);
  }

  constructor(
    content: string[],
    private context: Record<string, Content>,
    private settings: PipelineSettings,
    private engine: Engine,
    private rag: Plugin,
  ) {
    this.threads = partitionPrompts(content, context);
    LOGGER.debug(`Ready to generate ${this.threads.length} messages`);
  }

  start() {
    this.started = true;
    this.threadRunners = this.threads.map((t) =>
      PromptThread.run(
        t,
        this.context,
        this.settings,
        this.engine,
        this.rag,
        this.events,
      ),
    );

    this.settled.promise.then(() => {
      this.done = true;
    });
  }

  summaries(): PromptThreadSummary[] {
    return this.threadRunners.map((t) => t.summary());
  }

  summary(): PromptThreadsSummary {
    return this.summaries().reduce(
      (summary, threadSummary) => {
        summary.totalPrompts += threadSummary.totalPrompts;
        summary.finished += threadSummary.finished;
        summary.done &&= threadSummary.done;
        return summary;
      },
      {
        totalPrompts: 0,
        errors: 0,
        finished: 0,
        done: true,
      } as PromptThreadsSummary,
    );
  }

  drainAll() {
    for (const thread of this.threads) {
      for (const fiber of thread) {
        drain(fiber);
      }
    }
  }

  formatError(content: Content): string | undefined {
    const error = this.engine.formatError?.(content);
    if (error !== undefined) {
      return error;
    }

    return content.meta?.debug?.error?.message;
  }

  async allSettled(): Promise<PromiseSettledResult<Content>[]> {
    const runners = this.threadRunners.map((r) => r.allSettled());
    const runnersPromises = Promise.all(runners);
    this.drainAll();
    const settled = await runnersPromises;
    const flattened = settled.flat();
    this.settled.resolve(flattened);
    return flattened;
  }

  async updateDatabase(): Promise<void> {
    await this.rag.update(this.threads.flat());
  }

  errors() {
    return this.threads.flatMap((thread) =>
      thread
        .filter((content) => content.meta?.debug?.finish === "failed")
        .map((content) => ({
          content,
          errorMessage: this.formatError(content) ?? "Unknown Failure",
        })),
    );
  }
}

export function partitionPrompts(
  content: string[],
  context: Record<string, Content>,
): Content[][] {
  const directories = new Map<string, Content[]>();
  for (const c of content) {
    if (!context[c]) continue;
    const prefix = dirname(c);
    const entry = directories.get(prefix) ?? [];
    entry.push(context[c]);
    directories.set(prefix, entry);
  }

  return [...directories.values()];
}
