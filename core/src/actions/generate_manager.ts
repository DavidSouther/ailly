import { DEFAULT_ENGINE, RAG, Thread, getPlugin } from "../ailly";
import { Content, ContentMeta } from "../content/content";
import { partitionPrompts } from "./partition_prompts";
import {
  PromptThread,
  PromptThreadSummary,
  PromptThreadsSummary,
} from "./prompt_thread";

export class GenerateManager {
  done: boolean = false;
  started: boolean = false;
  threads: Thread[];
  threadRunners: PromptThread[] = [];

  static async from(
    content: Content[],
    settings: ContentMeta = {}
  ): Promise<GenerateManager> {
    const meta = content.at(0)?.meta;
    const pluginName = meta?.engine ?? settings?.engine ?? DEFAULT_ENGINE;
    const plugin = await getPlugin(pluginName);
    plugin.format(content);
    const rag = await RAG.build(plugin, settings.root!);
    return new GenerateManager(content, settings, rag);
  }

  constructor(
    private readonly content: Content[],
    private settings: ContentMeta,
    private rag: RAG
  ) {
    this.threads = partitionPrompts(content);
    console.log(`Ready to generate ${this.threads.length} messages`);
  }

  start() {
    this.started = true;
    this.threadRunners = this.threads.map((t) =>
      PromptThread.run(t, this.settings, this.rag)
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
