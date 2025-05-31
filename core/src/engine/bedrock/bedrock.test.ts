import { beforeEach, describe, expect, it } from "vitest";

import {
  FileSystem,
  RecordFileSystemAdapter,
} from "@davidsouther/jiffies/lib/cjs/fs.js";
import { cleanState } from "@davidsouther/jiffies/lib/cjs/scope/state.js";

import { type Content, loadContent } from "../../content/content";
import { makePipelineSettings } from "../../index.js";
import { contentToToolConfig, format } from "./bedrock";
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

  describe("contentToToolConfig", () => {
    it("converts content tools to Bedrock tool config", () => {
      const content: Content = {
        path: "/test/path",
        outPath: "/test/outPath",
        name: "test",
        prompt: "test prompt",
        context: { view: false },
        meta: {
          tools: [
            {
              name: "test_tool",
              description: "A test tool for testing",
              parameters: {
                type: "object",
                properties: {
                  param1: {
                    type: "string",
                    description: "First parameter",
                  },
                  param2: {
                    type: "number",
                    description: "Second parameter",
                  },
                },
                required: ["param1"],
              },
            },
          ],
        },
      };

      const result = contentToToolConfig(content);

      expect(result).toEqual({
        tools: [
          {
            toolSpec: {
              name: "test_tool",
              description: "A test tool for testing",
              inputSchema: {
                json: {
                  type: "object",
                  properties: {
                    param1: {
                      type: "string",
                      description: "First parameter",
                    },
                    param2: {
                      type: "number",
                      description: "Second parameter",
                    },
                  },
                  required: ["param1"],
                },
              },
            },
          },
        ],
      });
    });

    it("returns undefined tools when content has no tools", () => {
      const content: Content = {
        path: "/test/path",
        outPath: "/test/outPath",
        name: "test",
        prompt: "test prompt",
        context: { view: false },
        meta: {},
      };
      const result = contentToToolConfig(content);
      expect(result).toEqual({ tools: undefined });
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
