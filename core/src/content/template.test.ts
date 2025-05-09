import { expect, test, vi } from "vitest";
import { LOGGER } from "../index.js";
import { withResolvers } from "../util";
import type { Content } from "./content.js";
import { mergeContentViews, mergeViews } from "./template.js";

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
    responseStream: withResolvers(),
  };
  mergeContentViews(content, { base: "base", test: "bang" }, {});
  expect(content.context.system?.[0].content).toBe("system base");
  expect(content.context.system?.[0].view).toEqual({
    system: "system",
    test: "baz",
  });
  expect(content.prompt).toEqual("base system foo");
  expect(content.context.view).toEqual({ test: "foo" });
  expect(content.meta?.prompt).toEqual("{{base}} {{system}} {{test}}");
  expect(content.meta?.view).toEqual({ test: "foo" });
});

test("recursion settle", () => {
  const content: Content = {
    name: "test",
    outPath: "/test",
    path: "/test",
    prompt: "{{foo}}",
    context: {
      view: { foo: "{{bar}}", bar: "baz" },
    },
    responseStream: withResolvers(),
  };
  mergeContentViews(content, {}, {});
  expect(content.prompt).toBe("baz");
});

test("recursion convergence limit", () => {
  const spy = vi.spyOn(LOGGER, "warn");
  const content: Content = {
    name: "test",
    outPath: "/test",
    path: "/test",
    prompt: "{{foo}}",
    context: {
      view: { foo: "{{bar}}", bar: "{{foo}}" },
    },
    responseStream: withResolvers(),
  };
  mergeContentViews(content, {}, {});
  expect(content.prompt).toBe("{{foo}}");
  expect(spy).toHaveBeenCalledWith(
    "Reached TEMPLATE_RECURSION_CONVERGENCE limit of 10",
  );
});
