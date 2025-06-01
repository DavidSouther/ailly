import {
  FileSystem,
  ObjectFileSystemAdapter,
} from "@davidsouther/jiffies/lib/cjs/fs";
import { LEVEL } from "@davidsouther/jiffies/lib/cjs/log";
import { range } from "@davidsouther/jiffies/lib/cjs/range.js";
import { cleanState } from "@davidsouther/jiffies/lib/cjs/scope/state";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getPlugin, makePipelineSettings } from "..";
import { loadContent } from "../content/content.js";
import { getEngine } from "../engine/index.js";
import { TIMEOUT } from "../engine/noop.js";
import { LOGGER } from "../index.js";
import { MockClient } from "../mcp";
import { withResolvers } from "../util.js";
import {
  PromptThread,
  drain,
  generateOne,
  scheduler,
} from "./prompt_thread.js";

describe("scheduler", () => {
  it("limits outstanding tasks", async () => {
    const tasks = range(0, 5).map((i) => ({
      i,
      started: false,
      finished: false,
      ...withResolvers<void>(),
    }));
    const runners = tasks.map((task) => async () => {
      console.log(`starting ${task.i}`);
      task.started = true;
      await task.promise;
      console.log(`finishing ${task.i}`);
      task.finished = true;
    });

    scheduler(runners, 2);

    expect(tasks[0].started).toBe(true);
    expect(tasks[1].started).toBe(true);
    expect(tasks[2].started).toBe(false);
    expect(tasks[3].started).toBe(false);
    expect(tasks[4].started).toBe(false);

    await Promise.resolve().then(() => tasks[0].resolve());
    expect(tasks[0].finished).toBe(true);
    await Promise.resolve(); // Allow outstanding to clear
    await Promise.resolve(); // Allow loop to continue

    expect(tasks[1].started).toBe(true);
    expect(tasks[2].started).toBe(true);
    expect(tasks[3].started).toBe(false);
    expect(tasks[4].started).toBe(false);
  });
});

describe("generateOne", () => {
  const level = LOGGER.level;
  const state = cleanState(async () => {
    const logger = {
      info: vi.spyOn(LOGGER, "info"),
      debug: vi.spyOn(LOGGER, "debug"),
    };
    LOGGER.level = LEVEL.SILENT;
    const fs = new FileSystem(
      new ObjectFileSystemAdapter({
        "a.txt": "prompt a",
        "a.txt.ailly.md": "response a",
        "b.txt":
          "---\nprompt: prompt b\nskip: true\ncombined: true\n---\nresponse b",
        "c.txt": "tell me a joke",
      }),
    );
    const context = await loadContent(fs);
    const engine = await getEngine("noop");
    TIMEOUT.setTimeout(0);
    expect(logger.debug).toHaveBeenCalledWith("Loading content from /");
    expect(logger.debug).toHaveBeenCalledWith("Found 3 at or below /");
    expect(logger.info).toHaveBeenCalledTimes(0);
    logger.debug.mockClear();
    logger.info.mockClear();
    return { logger, context, engine, fs };
  }, beforeEach);

  afterEach(() => {
    vi.restoreAllMocks();
    LOGGER.level = level;
    TIMEOUT.resetTimeout();
  });

  it("skips some and runs others", async () => {
    await generateOne(
      state.context["/a.txt"],
      state.context,
      await makePipelineSettings({ root: "/", overwrite: false }),
      state.engine,
    );
    expect(state.logger.info).toHaveBeenCalledWith("Skipping /a.txt");
    state.logger.info.mockClear();

    await generateOne(
      state.context["/b.txt"],
      state.context,
      await makePipelineSettings({ root: "/" }),
      state.engine,
    );
    expect(state.logger.info).toHaveBeenCalledWith("Skipping /b.txt");
    state.logger.info.mockClear();

    const content = state.context["/c.txt"];
    expect(content.response).toBeUndefined();
    await generateOne(
      content,
      state.context,
      await makePipelineSettings({ root: "/" }),
      state.engine,
    );
    await drain(content);
    expect(state.logger.info).toHaveBeenCalledWith("Running /c.txt");
    expect(state.logger.debug).toHaveBeenCalledWith("Generating response", {
      engine: "noop",
      messages: [
        { role: "system", content: "" },
        { role: "user", content: "prompt a" },
        { role: "assistant", content: "response a" },
        { role: "user", content: "prompt b" },
        { role: "assistant", content: "response b" },
        { role: "user", content: "tell me a joke" },
      ],
    });
    expect(content.response).toMatch(/^noop response for c.txt:/);
  });
});

describe("PromptThread", () => {
  const level = LOGGER.level;
  const state = cleanState(async () => {
    const logger = {
      info: vi.spyOn(LOGGER, "info"),
      debug: vi.spyOn(LOGGER, "debug"),
    };
    LOGGER.level = LEVEL.SILENT;
    const fs = new FileSystem(
      new ObjectFileSystemAdapter({
        "a.txt": "prompt a",
        "a.txt.ailly.md": "response a",
        "b.txt": "---\nprompt: prompt b\nskip: true\n---\nresponse b",
        "c.txt": "tell me a joke\n",
      }),
    );
    const engine = await getEngine("noop");
    TIMEOUT.setTimeout(0);
    return { logger, fs, engine };
  }, beforeEach);

  afterEach(() => {
    vi.restoreAllMocks();
    LOGGER.level = level;
    TIMEOUT.resetTimeout();
  });

  it("runs isolated", async () => {
    const settings = await makePipelineSettings({ root: "/", isolated: true });
    const context = await loadContent(state.fs, {
      system: [],
      meta: { isolated: true },
    });
    const content = [...Object.values(context)];
    const plugin = await (await getPlugin("none")).default(
      state.engine,
      settings,
    );
    const thread = PromptThread.run(
      content,
      context,
      settings,
      state.engine,
      plugin,
    );
    expect(thread.isDone).toBe(false);
    expect(thread.finished).toBe(0);
    expect(thread.errors.length).toBe(0);

    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    // Enough to get one resolved

    expect(thread.isDone).toBe(false);
    expect(thread.finished).toBe(1);
    expect(thread.errors.length).toBe(0);

    await thread.allSettled();

    expect(thread.isDone).toBe(true);
    expect(thread.finished).toBe(3);
    expect(thread.errors.length).toBe(0);
  });

  it("runs sequence", async () => {
    const settings = await makePipelineSettings({ root: "/" });
    const context = await loadContent(state.fs);
    const content = [...Object.values(context)];
    const plugin = await (await getPlugin("none")).default(
      state.engine,
      settings,
    );
    const thread = PromptThread.run(
      content,
      context,
      settings,
      state.engine,
      plugin,
    );
    expect(thread.isDone).toBe(false);
    expect(thread.finished).toBe(0);
    expect(thread.errors.length).toBe(0);

    await thread.allSettled();

    expect(thread.isDone).toBe(true);
    expect(thread.finished).toBe(3);
    expect(thread.errors.length).toBe(0);
  });

  it("runs with MCP", async () => {
    const settings = await makePipelineSettings({
      root: "/",
      isolated: true,
      combined: true,
    });
    const fs = new FileSystem(
      new ObjectFileSystemAdapter({
        ".ailly.md": "---\nmcp:\n  mock:\n    type: mock\n---\n",
        "a.txt": "USE add WITH 40 7",
      }),
    );
    const context = await loadContent(fs);
    const content = [...Object.values(context)];
    for (const f of content) {
      f.context.mcpClient = new MockClient();
    }
    const plugin = await (await getPlugin("none")).default(
      state.engine,
      settings,
    );
    const thread = PromptThread.run(
      content,
      context,
      settings,
      state.engine,
      plugin,
    );

    await thread.allSettled();

    expect(thread.isDone).toBe(true);
    expect(thread.finished).toBe(1);
    expect(thread.errors.length).toBe(0);

    expect(content.at(-1)?.response).toBe(
      "USING TOOL add WITH ARGS [40, 7]\nTOOL RETURNED 47\n",
    );
  });
});
