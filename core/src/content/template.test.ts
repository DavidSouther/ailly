import { expect, test } from "vitest";
import { mergeContentViews, mergeViews } from "./template.js";
import { Content } from "./content";

test("merge views", () => {
  const c = mergeViews({ a: "a", c: "1" }, { b: "b" }, { c: "2" });
  expect(c).toEqual({ a: "a", b: "b", c: "2" });
});

test("merge content views", () => {
  const content: Content = {
    name: "test",
    outPath: "/test",
    path: "/test",
    prompt: "{{base}} {{system}} {{test}}",
    context: {
      view: { test: "foo" },
      system: [
        { content: "system {{base}}", view: { system: "system", test: "baz" } },
      ],
    },
  };
  mergeContentViews(content, { base: "base", test: "bang" }, {});
  expect(content.context.system?.[0].content).toBe("system base");
  expect(content.context.system?.[0].view).toBe(false);
  expect(content.prompt).toEqual("base system foo");
  expect(content.context.view).toEqual(false);
  expect(content.meta?.prompt).toEqual("{{base}} {{system}} {{test}}");
  expect(content.meta?.view).toEqual({ test: "foo" });
});
