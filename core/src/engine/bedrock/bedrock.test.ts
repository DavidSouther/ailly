import { beforeEach, describe, expect, it } from "vitest";

import {
  FileSystem,
  RecordFileSystemAdapter,
} from "@davidsouther/jiffies/lib/cjs/fs.js";
import { cleanState } from "@davidsouther/jiffies/lib/cjs/scope/state.js";

import { loadContent } from "../../content/content";
import { makePipelineSettings } from "../../index.js";
import { format } from "./bedrock";
import { converseBuilder } from "./prompt_builder.js";

describe("bedrock claude3", () => {
  describe("prompt builder", () => {
    it("combines system prompts", () => {
      const actual = converseBuilder([
        { role: "system", content: "sysa" },
        { role: "system", content: "sysb" },
      ]);
      expect(actual.system).toEqual("sysa\nsysb\n");
    });

    it("combines user prompts", () => {
      const actual = converseBuilder([
        { role: "user", content: "usera" },
        { role: "user", content: "userb" },
      ]);
      expect(actual.messages).toEqual([
        { role: "user", content: "usera\nuserb" },
      ]);
    });

    it("combines assistant prompts", () => {
      const actual = converseBuilder([
        { role: "assistant", content: "assista" },
        { role: "assistant", content: "assistb" },
      ]);
      expect(actual.messages).toEqual([
        { role: "assistant", content: "assista\nassistb" },
      ]);
    });
  });

  describe("format", () => {
    const state = cleanState(async () => {
      const root = "/root";
      const settings = await makePipelineSettings({ root });
      const fs = new FileSystem(
        new RecordFileSystemAdapter({
          "/root/a": "prompt a",
          "/root/a.ailly.md": "response a",
          "/root/b": "prompt b",
        }),
      );
      const context = await loadContent(fs, { meta: settings }, 2);
      return { root, settings, context };
    }, beforeEach);

    it("formats contents into messages", async () => {
      const contents = Object.values(state.context);
      await format(contents, state.context);

      const system = { role: "system", content: "" };
      expect(contents[0].meta?.messages).toEqual([
        system,
        { role: "user", content: "prompt a" },
      ]);
      expect(contents[1].meta?.messages).toEqual([
        system,
        { role: "user", content: "prompt a" },
        { role: "assistant", content: "response a" },
        { role: "user", content: "prompt b" },
      ]);
    });
  });
});
