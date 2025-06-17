import { beforeEach, describe, expect, it } from "vitest";

import {
  FileSystem,
  RecordFileSystemAdapter,
} from "@davidsouther/jiffies/lib/cjs/fs.js";
import { Ok } from "@davidsouther/jiffies/lib/cjs/result.js";
import { cleanState } from "@davidsouther/jiffies/lib/cjs/scope/state.js";

import { type Content, loadContent } from "../../content/content.js";
import { makePipelineSettings } from "../../index.js";
import type { Message } from "../index.js";
import { format } from "./bedrock.js";
import {
  type Models,
  contentToToolConfig,
  converseBuilder,
} from "./prompt_builder.js";

const TEST_MODEL: Models = "us.anthropic.claude-3-haiku-20240307-v1:0";

describe("bedrock claude3", () => {
  describe("prompt builder", () => {
    function makeContentForMessages(messages: Message[]) {
      return {
        name: "test",
        path: "test",
        outPath: "test",
        prompt: "test",
        context: { view: {} },
        meta: { messages },
      };
    }
    it("combines system prompts", () => {
      const actual = converseBuilder(
        TEST_MODEL,
        makeContentForMessages([
          { role: "system", content: "sysa" },
          { role: "system", content: "sysb" },
        ]),
      );
      expect(actual.system).toEqual([{ text: "sysa\nsysb" }]);
    });

    it("combines user prompts", () => {
      const actual = converseBuilder(
        TEST_MODEL,
        makeContentForMessages([
          { role: "user", content: "usera" },
          { role: "user", content: "userb" },
        ]),
      );
      expect(actual.messages).toEqual([
        { role: "user", content: [{ text: "usera" }, { text: "userb" }] },
      ]);
    });

    it("combines assistant prompts", () => {
      const actual = converseBuilder(
        TEST_MODEL,
        makeContentForMessages([
          { role: "assistant", content: "assista" },
          { role: "assistant", content: "assistb" },
        ]),
      );
      expect(actual.messages).toEqual([
        {
          role: "assistant",
          content: [{ text: "assista" }, { text: "assistb" }],
        },
      ]);
    });

    it("adds tool use blocks", () => {
      const content = makeContentForMessages([
        { role: "user", content: "USE add WITH 2 7" },
        {
          role: "assistant",
          content: "I'll use the addition tool to add 2 with 7",
        },
        {
          role: "user",
          content: "",
          toolUse: {
            name: "add",
            input: {
              args: [2, 7],
            },
            result: Ok({ content: "9" }),
            id: "test-tool",
          },
        },
      ]);
      const actual = converseBuilder(TEST_MODEL, content);
      expect(actual.messages).toEqual([
        { role: "user", content: [{ text: "USE add WITH 2 7" }] },
        {
          role: "assistant",
          content: [
            { text: "I'll use the addition tool to add 2 with 7" },
            {
              toolUse: {
                name: "add",
                input: { args: [2, 7] },
                toolUseId: "test-tool",
              },
            },
          ],
        },
        {
          role: "user",
          content: [
            {
              toolResult: {
                toolUseId: "test-tool",
                status: "success",
                content: [{ json: { content: "9" } }],
              },
            },
          ],
        },
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
        toolConfig: {
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
        },
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
