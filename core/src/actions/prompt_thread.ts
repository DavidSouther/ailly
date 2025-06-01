import { Temporal } from "temporal-polyfill";

import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert.js";

import type { Content, View } from "../content/content.js";
import {
  GLOBAL_VIEW,
  mergeContentViews,
  mergeViews,
} from "../content/template.js";
import type { Engine } from "../engine/index.js";
import {
  DEFAULT_SCHEDULER_LIMIT,
  LOGGER,
  type PipelineSettings,
} from "../index.js";
import { MCPClient } from "../mcp.js";
import type { Plugin } from "../plugin/index.js";

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
  limit: number = DEFAULT_SCHEDULER_LIMIT,
): Promise<PromiseSettledResult<T>[]> {
  const taskQueueRev = [...taskQueue].reverse();
  const finished: Array<Promise<T>> = [];
  const outstanding = new Set<Promise<T>>();
  while (taskQueueRev.length > 0) {
    if (outstanding.size >= limit) {
      // Wait for something in outstanding to finish
      await Promise.race([...outstanding]);
    } else {
      const task = taskQueueRev.pop();
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
  finished = 0;
  isolated = false;
  private done = false;
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
    return this.done && this.errors.length === 0;
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
    rag: Plugin,
  ) {
    settings.isolated =
      settings.isolated || content.every((c) => c.meta?.isolated);
    const thread = new PromptThread(content, context, settings, engine, rag);
    thread.start();
    return thread;
  }

  private constructor(
    private readonly content: Content[],
    private readonly context: Record<string, Content>,
    private settings: PipelineSettings,
    private engine: Engine,
    private plugin: Plugin,
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
        this.settings.templateView,
      );
    }

    if (c.meta?.mcp && !c.context.mcpClient) {
      const mcpClient = new MCPClient();

      // Extract MCP information from meta server and attach MCP Clients to Context
      await mcpClient.initialize({ servers: c.meta.mcp });

      // Assign all the tools to meta.tools
      c.meta.tools = mcpClient.getAllTools();

      // Attach MCP Clients to context
      c.context.mcpClient = mcpClient;
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
    } finally {
      if (c.context.mcpClient) {
        c.context.mcpClient.cleanup();
      }
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
      this.settings.requestLimit,
    ).finally(() => {
      this.done = true;
    });
  }

  private async runSequence(): Promise<PromiseSettledResult<Content>[]> {
    LOGGER.debug(
      `Running thread for sequence of ${this.content.length} prompts`,
    );
    const results: PromiseSettledResult<Content>[] = [];
    for (let i = 0; i < this.content.length; i++) {
      const content = this.content[i];
      try {
        await this.runOne(content, i);
        results.push({ status: "fulfilled", value: content } as const);
        if (content.response) {
          content.meta?.messages?.push({
            role: "assistant",
            content: content.response,
            tokens: Number.NaN,
          });
        }
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
  engine: Engine,
): Promise<void> {
  const has_response = (c.response?.length ?? 0) > 0;

  if (c.meta?.skip || (!settings.overwrite && has_response)) {
    LOGGER.info(`Skipping ${c.path}`);
    const stream = new TextEncoderStream();
    stream.writable.getWriter().write(c.response ?? "");
    assertExists(c.responseStream).resolve(
      stream.readable.pipeThrough(new TextDecoderStream()),
    );
    stream.writable.close();
    return Promise.resolve();
  }

  LOGGER.info(`Running ${c.path}`);

  const meta = c.meta;
  engine.format([c], context);

  LOGGER.debug("Generating response", {
    engine: engine.name,
    messages: meta?.messages?.map((m) => ({
      role: m.role,
      content: m.content,
      // tokens: m.tokens,
    })),
  });

  c.meta = {
    ...c.meta,
    debug: {
      engine: settings.engine,
      model: settings.model,
      lastRun: Temporal.Now.instant(),
    },
  };

  // Start a new stream for streams
  const generatorStream = new TransformStream();
  assertExists(c.responseStream).resolve(generatorStream.readable);

  async function runWithTools() {
    const generator = engine.generate(c, settings);
    generator.stream.pipeTo(generatorStream.writable, { preventClose: true });
    await generator.done.finally(() => {
      c.response = generator.message();
      assertExists(c.meta).debug = { ...c.meta?.debug, ...generator.debug() };
    });
    const { toolUse } = generator.debug();
    if (toolUse && c.meta && c.context.mcpClient) {
      LOGGER.info("Invoking tool", {
        tool: {
          name: toolUse.name,
          input: toolUse.input,
        },
      });
      LOGGER.debug("Tool details", { toolUse });
      const result = await c.context.mcpClient.invokeTool(
        toolUse.name,
        toolUse.input,
      );
      LOGGER.debug("Tool response", { toolUse, result });

      c.meta.messages ??= [];
      c.meta.messages.push({
        role: "assistant",
        content: c.response ?? "",
      });

      c.meta.messages.push({
        role: "user",
        content: "",
        toolUse: {
          name: toolUse.name,
          input: toolUse.input,
          result: result,
          id: toolUse.id,
        },
      });
      c.meta.continue = true;
      return runWithTools();
    }
    generatorStream.writable.close();
  }

  try {
    return runWithTools();
  } catch (err) {
    LOGGER.error(`Uncaught error in ${engine.name} generator`, { err });
    if (c.meta?.debug) {
      c.meta.debug.finish = "failed";
      c.meta.debug.error = err as Error;
    }
    return Promise.resolve();
  }
}

export async function drain(content: Content) {
  const stream = await assertExists(content.responseStream).promise;
  if (stream.locked) {
    return;
  }
  for await (const _ of stream) {
    // Do nothing, just need to run the AsyncIterable
  }
}
