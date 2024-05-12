import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { range } from "@davidsouther/jiffies/lib/esm/range.js";
import { PromptThread, generateOne, scheduler } from "./prompt_thread";
import { LOGGER } from "../util";
import { cleanState } from "@davidsouther/jiffies/lib/esm/scope/state";
import { loadContent } from "../content/content";
import {
  FileSystem,
  ObjectFileSystemAdapter,
} from "@davidsouther/jiffies/lib/esm/fs";
import { getPlugin, makePipelineSettings } from "../ailly";
import { ENGINES, getEngine } from "../engine";
import { LEVEL } from "@davidsouther/jiffies/lib/esm/log";
import { TIMEOUT } from "../engine/noop";

Promise.withResolvers =
  Promise.withResolvers ??
  function makePromise<T = void>(): PromiseWithResolvers<T> {
    let resolve: (t: T | PromiseLike<T>) => void = () => {};
    let reject: (reason?: any) => void = () => {};
    const promise = new Promise<T>((r, j) => {
      resolve = r;
      reject = j;
    });
    return { promise, resolve, reject };
  };

describe("scheduler", () => {
  it("limits outstanding tasks", async () => {
    const tasks = range(0, 5).map((i) => ({
      i,
      started: false,
      finished: false,
      ...Promise.withResolvers<void>(),
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
  let level = LOGGER.level;
  const state = cleanState(async () => {
    const logger = {
      info: vi.spyOn(LOGGER, "info"),
      debug: vi.spyOn(LOGGER, "debug"),
    };
    LOGGER.level = LEVEL.SILENT;
    const context = await loadContent(
      new FileSystem(
        new ObjectFileSystemAdapter({
          "a.txt": `prompt a`,
          "a.txt.ailly.md": `response a`,
          "b.txt": `---\nprompt: prompt b\nskip: true\n---\nresponse b`,
          "c.txt": "tell me a joke\n",
        })
      )
    );
    const engine = await getEngine("noop");
    TIMEOUT.setTimeout(0);
    expect(logger.debug).toHaveBeenCalledWith("Loading content from /");
    expect(logger.debug).toHaveBeenCalledWith("Found 3 at or below /");
    expect(logger.info).toHaveBeenCalledTimes(0);
    logger.debug.mockClear();
    logger.info.mockClear();
    return { logger, context, engine };
  }, beforeEach);

  afterEach(() => {
    vi.restoreAllMocks();
    LOGGER.level = level;
    TIMEOUT.resetTimeout();
  });

  it("skips some", async () => {
    generateOne(
      state.context["/a.txt"],
      state.context,
      await makePipelineSettings({ root: "/", overwrite: false }),
      state.engine
    );
    expect(state.logger.info).toHaveBeenCalledWith("Skipping /a.txt");
    state.logger.info.mockClear();

    generateOne(
      state.context["/b.txt"],
      state.context,
      await makePipelineSettings({ root: "/" }),
      state.engine
    );
    expect(state.logger.info).toHaveBeenCalledWith("Skipping /b.txt");
    state.logger.info.mockClear();
  });

  it("generates others", async () => {
    const content = state.context["/c.txt"];
    expect(content.response).toBeUndefined();
    await generateOne(
      content,
      state.context,
      await makePipelineSettings({ root: "/" }),
      state.engine
    );
    expect(state.logger.info).toHaveBeenCalledWith("Preparing /c.txt");
    expect(state.logger.info).toHaveBeenCalledWith("Calling noop");
    expect(content.response).toMatch(/^noop response for c.txt:/);
  });
});

describe("PromptThread", () => {
  let level = LOGGER.level;
  const state = cleanState(async () => {
    const logger = {
      info: vi.spyOn(LOGGER, "info"),
      debug: vi.spyOn(LOGGER, "debug"),
    };
    LOGGER.level = LEVEL.SILENT;
    const fs = new FileSystem(
      new ObjectFileSystemAdapter({
        "a.txt": `prompt a`,
        "a.txt.ailly.md": `response a`,
        "b.txt": `---\nprompt: prompt b\nskip: true\n---\nresponse b`,
        "c.txt": "tell me a joke\n",
      })
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
    const context = await loadContent(state.fs, [], { isolated: true });
    state.logger.debug.mockClear();
    state.logger.info.mockClear();
    const content = [...Object.values(context)];
    const plugin = await (
      await getPlugin("none")
    ).default(state.engine, settings);
    const thread = PromptThread.run(
      content,
      context,
      settings,
      state.engine,
      plugin
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
    // Enough to get one resolved

    expect(thread.isDone).toBe(false);
    expect(thread.finished).toBe(1);
    expect(thread.errors.length).toBe(0);

    await thread.allSettled();

    expect(thread.isDone).toBe(true);
    expect(thread.finished).toBe(3);
    expect(thread.errors.length).toBe(0);

    expect(content[0].response).toEqual(
      `noop response for a.txt:\n\nsystem: \nuser: prompt a\nassistant: response a\nprompt a`
    );
    expect(content[1].response).toBeUndefined();
    expect(content[2].response).toEqual(
      `noop response for c.txt:\n\nsystem: \nuser: tell me a joke\n\ntell me a joke\n`
    );
  });

  it("runs sequence", async () => {
    const settings = await makePipelineSettings({ root: "/" });
    const context = await loadContent(state.fs);
    state.logger.debug.mockClear();
    state.logger.info.mockClear();
    const content = [...Object.values(context)];
    const plugin = await (
      await getPlugin("none")
    ).default(state.engine, settings);
    const thread = PromptThread.run(
      content,
      context,
      settings,
      state.engine,
      plugin
    );
    expect(thread.isDone).toBe(false);
    expect(thread.finished).toBe(0);
    expect(thread.errors.length).toBe(0);

    await thread.allSettled();

    expect(thread.isDone).toBe(true);
    expect(thread.finished).toBe(3);
    expect(thread.errors.length).toBe(0);

    expect(content[0].response).toEqual(
      `noop response for a.txt:\n\nsystem: \nuser: prompt a\nassistant: response a\nprompt a`
    );
    expect(content[1].response).toBeUndefined();
    expect(content[2].response).toEqual(
      `noop response for c.txt:\n\nsystem: \nuser: prompt a\nassistant: noop response for a.txt:\n\nsystem: \nuser: prompt a\nassistant: response a\nprompt a\nuser: response b\nuser: tell me a joke\n\ntell me a joke\n`
    );
  });
});
