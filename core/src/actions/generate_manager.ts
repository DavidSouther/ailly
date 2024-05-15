import { DEFAULT_ENGINE, LOGGER, PipelineSettings, Thread } from "../ailly.js";
import { partitionPrompts } from "../content/partition.js";
import {
  PromptThread,
  PromptThreadSummary,
  PromptThreadsSummary,
} from "./prompt_thread.js";

import type { Content } from "../content/content.js";
import { getPlugin, type Plugin } from "../plugin/index.js";
import { getEngine, type Engine } from "../engine/index.js";

export class GenerateManager {
  done: boolean = false;
  settled: PromiseWithResolvers<PromiseSettledResult<Content>[]> =
    Promise.withResolvers();
  started: boolean = false;
  threads: Thread[];
  threadRunners: PromptThread[] = [];

  static async from(
    content: string[],
    context: Record<string, Content>,
    settings: PipelineSettings
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
    private rag: Plugin
  ) {
    this.threads = partitionPrompts(content, context);
    LOGGER.debug(`Ready to generate ${this.threads.length} messages`);
  }
  start() {
    this.started = true;
    this.threadRunners = this.threads.map((t) =>
      PromptThread.run(t, this.context, this.settings, this.engine, this.rag)
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
      } as PromptThreadsSummary
    );
  }

  drainAll() {
    this.threads.forEach((thread) =>
      thread.forEach(async (c) => {
        while (c.responseStream === undefined) {
          await Promise.resolve();
        }
        if (!c.responseStream.locked) {
          for await (const _ of c.responseStream as unknown as AsyncIterable<string>) {
            // Drain c.responseStream
          }
        }
      })
    );
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
}
