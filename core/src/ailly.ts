import { Content } from "./content.js";
import { dirname } from "path";

const DEFAULT_PLUGIN = "openai";

// TODO make this async*
export function generate(
  content: Content[],
  settings: Record<string, string> = {}
): GenerateManager {
  const manager = new GenerateManager(content, settings);

  return manager;
}

type Thread = Content[];

export class GenerateManager {
  done: boolean = false;
  started: boolean = false;
  threads: Thread[];
  threadRunners: PromptThread[] = [];

  static async from(
    content: Content[],
    settings: Record<string, string>
  ): Promise<GenerateManager> {
    const pluginName = content.at(0)?.meta?.head?.["plugin"] ?? DEFAULT_PLUGIN;
    const plugin = (await import(`./plugin/${pluginName}.js`)) as Plugin;
    plugin.format(content);
    return new GenerateManager(content, settings);
  }

  constructor(
    private readonly content: Content[],
    private settings: Record<string, string>
  ) {
    this.threads = partitionPrompts(content);
    console.log(`Ready to generate ${this.threads.length} messages`);
  }

  start() {
    this.started = true;
    this.threadRunners = this.threads.map((t) =>
      PromptThread.run(t, this.settings)
    );

    Promise.allSettled(this.threadRunners.map((t) => t.allSettled())).then(
      () => {
        this.done = true;
      }
    );
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

  async allSettled(): Promise<PromiseSettledResult<Content>[]> {
    return (
      await Promise.all(this.threadRunners.map((r) => r.allSettled()))
    ).flat();
  }
}

class PromptThread {
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

  static run(content: Content[], settings: Record<string, string>) {
    const thread = new PromptThread(content, settings);
    thread.start();
    return thread;
  }

  private constructor(
    private readonly content: Content[],
    private settings: Record<string, string>
  ) {
    this.content = content;
    this.isolated = Boolean(content[0]?.meta?.head?.["isolated"] ?? false);
  }

  start() {
    this.runner = this.isolated ? this.runIsolated() : this.runSequence();
  }

  private runIsolated(): Promise<PromiseSettledResult<Content>[]> {
    console.log(`Running thread for ${this.content.length} isolated prompts`);
    const promises: Promise<Content>[] = this.content.map(
      async (c, i): Promise<Content> => {
        try {
          c.response = await generateOne(c, this.settings);
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
        content.response = await generateOne(content, this.settings);
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

interface PromptThreadsSummary {
  totalPrompts: number;
  finished: number;
  errors: number;
  done: boolean;
}

interface PromptThreadSummary extends PromptThreadsSummary {
  isolated: boolean;
}

async function generateOne(
  c: Content,
  settings: Record<string, string>
): Promise<string> {
  // Determine PLUGIN and MODEL, load them.
  const pluginName = c.meta?.head?.["plugin"] ?? DEFAULT_PLUGIN;
  const plugin = (await import(`./plugin/${pluginName}.js`)) as Plugin;
  plugin.format([c]);
  const model = c.meta?.head?.["plugin"] ?? plugin.DEFAULT_MODEL;
  const generated = await plugin.generate(c, { ...settings, model });
  const debug = JSON.stringify(generated.debug);
  return [
    "---",
    `generated: ${new Date().toISOString()}`,
    `debug: ${debug}`,
    "---",
    generated.message,
  ].join("\n");
}

interface Plugin {
  DEFAULT_MODEL: string;
  format: (c: Content[]) => Promise<void>;
  generate: (
    c: Content,
    parameters: Record<string, string>
  ) => Promise<{ debug: unknown; message: string }>;
}

/*

/a
  /b.
  /c.
  /g
    /h.
/d
  /e.
  /f.

=>

/a/b.
/a/c.
/a/g/h.
/d/e.
/d/f.

=> 

[
  /a/b.
  /a/c.
]
[
  /a/g/h.
]
[
  /d/e.
  /d/f.
]
*/

export function partitionPrompts(content: Content[]): Content[][] {
  const directories = new Map<string, Content[]>();
  for (const c of content) {
    const prefix = dirname(c.path);
    const entry = directories.get(prefix) ?? [];
    entry.push(c);
    directories.set(prefix, entry);
  }

  for (const thread of directories.values()) {
    thread.sort((a, b) => a.order - b.order);
    if (!Boolean(thread.at(0)?.meta?.head?.["isolated"])) {
      for (let i = thread.length - 1; i > 0; i--) {
        thread[i].predecessor = thread[i - 1];
      }
    }
  }

  return [...directories.values()];
}
