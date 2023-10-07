import { Content } from "./content";

const DEFAULT_PLUGIN = "openai";

// TODO make this async*
export function generate(content: Content[]): GenerateManager {
  const manager = new GenerateManager(content);

  return manager;
}

type Thread = Content[];

export class GenerateManager {
  done: boolean = false;
  started: boolean = false;
  threads: Thread[];
  threadRunners: PromptThread[] = [];

  static async from(content: Content[]): Promise<GenerateManager> {
    const pluginName = content.at(0)?.meta?.head?.["plugin"] ?? DEFAULT_PLUGIN;
    const plugin = (await import(`./plugin/${pluginName}`)) as Plugin;
    plugin.format(content);
    return new GenerateManager(content);
  }

  constructor(private readonly content: Content[]) {
    // Partition Content into prompt threads
    // TODO Ensure that `previous` content gets generated first and included in future calls.
    this.threads = [content];
  }

  start() {
    this.started = true;
    this.threadRunners = this.threads.map(PromptThread.run);

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

  async allSettled() {
    return Promise.allSettled(
      this.threadRunners.map((r) => r.allSettled()).flat()
    );
  }
}

class PromptThread {
  finished: number = 0;
  isolated: boolean = false;
  done: boolean = false;
  runner?: Promise<unknown>;
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

  static run(content: Content[]) {
    const thread = new PromptThread(content);
    thread.start();
    return thread;
  }

  private constructor(private readonly content: Content[]) {
    this.content = content;
    this.isolated = Boolean(content[0]?.meta?.head?.["isolated"] ?? false);
  }

  start() {
    this.runner = this.isolated ? this.runIsolated() : this.runSequence();
  }

  private runIsolated() {
    return Promise.allSettled(
      this.content.map(async (c, i) => {
        try {
          c.response = await generateOne(c);
          this.finished += 1;
        } catch (e) {
          console.warn("Error generating content", e);
          this.errors.push([i, e as Error]);
        }
      })
    ).finally(() => {
      this.done = true;
    });
  }

  private async runSequence() {
    for (let i = 0; i < this.content.length; i++) {
      const content = this.content[i];
      try {
        content.response = await generateOne(content);
        this.finished += 1;
      } catch (e) {
        console.warn("Error generating content", e);
        this.errors.push([i, e as Error]);
        break;
      }
    }
    this.done = true;
  }

  async allSettled() {
    return this.runner ?? Promise.resolve();
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

async function generateOne(c: Content): Promise<string> {
  // Determine PLUGIN and MODEL, load them.
  const pluginName = c.meta?.head?.["plugin"] ?? DEFAULT_PLUGIN;
  const plugin = (await import(`./plugin/${pluginName}`)) as Plugin;
  const model = c.meta?.head?.["plugin"] ?? plugin.DEFAULT_MODEL;
  const generated = await plugin.generate(c, model);
  return `---\ngenerated: ${new Date().toISOString()}\ndebug: ${JSON.stringify(
    generated.debug
  )}\n---\n\n${generated.message}`;
}

interface Plugin {
  DEFAULT_MODEL: string;
  format: (c: Content[]) => Promise<void>;
  generate: (
    c: Content,
    parameters: Record<string, string>
  ) => Promise<{ debug: unknown; message: string }>;
}
