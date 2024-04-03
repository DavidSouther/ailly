import { expect, test } from "vitest";
import { mergeContentViews, mergeViews } from "./template.js";
import { Content } from "./content";

test("merge views", () => {
  const c = mergeViews({ a: "a" }, { b: "b" });
  expect(c).toEqual({ a: "a", b: "b" });
});

test("merge content views", () => {
  const content: Content = {
    name: "test",
    outPath: "/test",
    path: "/test",
    prompt: "{{base}} {{system}} {{test}}",
    view: { test: "foo" },
    system: [
      { content: "system {{base}}", view: { system: "system", test: "baz" } },
    ],
  };
  mergeContentViews(content, { base: "base", test: "bang" });
  expect(content.system?.[0].content).toBe("system base");
  expect(content.system?.[0].view).toBe(false);
  expect(content.prompt).toEqual("base system foo");
  expect(content.view).toEqual(false);
  expect(content.meta?.prompt).toEqual("{{base}} {{system}} {{test}}");
  expect(content.meta?.view).toEqual({ test: "foo" });
});
