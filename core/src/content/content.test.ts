import { expect, test } from "vitest";

import {
  FileSystem,
  ObjectFileSystemAdapter,
} from "@davidsouther/jiffies/lib/esm/fs.js";
import {
  Content,
  loadContent,
  writeContent,
  splitOrderedName,
} from "./content.js";

test("it loads content", async () => {
  const testFs = new FileSystem(
    new ObjectFileSystemAdapter({
      "01_start.md": "The quick brown",
      "20b": {
        "40_part.md": "fox jumped",
        "56_part.md": "over the lazy",
      },
      "_tweedle_dum.md": "Tweedle Dee",
      "54_a/12_section.md": "dog.",
    })
  );
  const content = await loadContent(testFs);

  expect(content.length).toBe(4);
});

test("it loads responses", async () => {
  const testFs = new FileSystem(
    new ObjectFileSystemAdapter({
      "01_start.md": "The quick brown",
      "01_start.md.ailly": "fox jumped",
    })
  );
  const content = await loadContent(testFs);

  expect(content.length).toBe(1);
  expect(content[0].prompt).toEqual("The quick brown");
  expect(content[0].response).toEqual("fox jumped");
});

test.each([
  ["prompt.md", "prompt"],
  ["prompt.md.ailly", "response"],
])("it splits ordered filenames: %s -> %s", (file, type) => {
  const split = splitOrderedName(file);
  expect(split.type).toBe(type);
});

test("it loads combined prompt and responses", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      ".aillyrc": "---\nisolated: true\n---",
      "prompt.md": "---\nprompt: prompt\n---",
      "content.md": "---\nprompt: content\n---\nResponse",
    })
  );

  const content = await loadContent(fs);
  expect(content.length).toBe(2);
  expect(content).toEqual([
    {
      name: "content.md",
      path: "/content.md",
      outPath: "/content.md",
      prompt: "content",
      response: "Response",
      system: [""],
      meta: { isolated: true, combined: true },
    },
    {
      name: "prompt.md",
      path: "/prompt.md",
      outPath: "/prompt.md",
      prompt: "prompt",
      response: "",
      system: [""],
      meta: { isolated: true, combined: true },
    },
  ] as Content[]);
});

test("it writes combined prompt and responses", async () => {
  const fs = new FileSystem(new ObjectFileSystemAdapter({}));

  const content: Content[] = [
    {
      name: "content.md",
      path: "/",
      outPath: "/",
      prompt: "content",
      response: "Response",
      system: [""],
      meta: { isolated: true, combined: true },
    },
  ];

  await writeContent(fs, content);

  expect((fs as any).adapter.fs).toEqual({
    "/content.md":
      "---\ncombined: true\nisolated: true\nprompt: content\n---\nResponse",
  });
});

test("it loads separate prompt and responses", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      ".aillyrc": "---\nisolated: true\n---",
      "prompt.md": "prompt",
      "content.md": "content",
      "content.md.ailly": "Response",
    })
  );

  const content = await loadContent(fs);
  expect(content.length).toBe(2);
  expect(content).toEqual([
    {
      name: "content.md",
      path: "/content.md",
      outPath: "/content.md",
      prompt: "content",
      response: "Response",
      system: [""],
      meta: { isolated: true, combined: false },
    },
    {
      name: "prompt.md",
      path: "/prompt.md",
      outPath: "/prompt.md",
      prompt: "prompt",
      response: "",
      system: [""],
      meta: { isolated: true, combined: false },
    },
  ] as Content[]);
});

test("it loads separate prompt and responses in different out directors", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      root: {
        ".aillyrc": "---\nisolated: true\n---",
        "prompt.md": "prompt",
        "content.md": "content",
      },
      out: {
        "content.md.ailly": "Response",
      },
    })
  );

  const content = await loadContent(fs, [], {
    root: "/root",
    out: "/out",
  });
  expect(content.length).toBe(2);
  expect(content).toEqual([
    {
      name: "content.md",
      path: "/root/content.md",
      outPath: "/out/content.md",
      prompt: "content",
      response: "Response",
      system: ["", ""],
      meta: { isolated: true, combined: false, root: "/root", out: "/out" },
    },
    {
      name: "prompt.md",
      path: "/root/prompt.md",
      outPath: "/out/prompt.md",
      prompt: "prompt",
      response: "",
      system: ["", ""],
      meta: { isolated: true, combined: false, root: "/root", out: "/out" },
    },
  ] as Content[]);
});

test("it writes separate prompt and responses", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      "content.md": "---\ncombined: false\nisolated: true\n---\ncontent",
    })
  );

  const content: Content[] = [
    {
      name: "content.md",
      path: "/content.md",
      outPath: "/content.md",
      prompt: "content",
      response: "Response",
      system: [""],
      meta: { isolated: true, combined: false },
    },
  ];

  await writeContent(fs, content);

  expect((fs as any).adapter.fs).toEqual({
    "/content.md": "---\ncombined: false\nisolated: true\n---\ncontent",
    "/content.md.ailly": "---\ncombined: false\nisolated: true\n---\nResponse",
  });
});

test("it writes separate prompt and responses in outPath", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      root: {
        "content.md": "---\ncombined: false\nisolated: true\n---\ncontent",
      },
    })
  );

  const content: Content[] = [
    {
      name: "content.md",
      path: "/root/content.md",
      outPath: "/out/content.md",
      prompt: "content",
      response: "Response",
      system: [""],
      meta: { isolated: true, combined: false },
    },
  ];

  await writeContent(fs, content);

  expect((fs as any).adapter.fs).toEqual({
    "/root/content.md": "---\ncombined: false\nisolated: true\n---\ncontent",
    "/out/content.md.ailly":
      "---\ncombined: false\nisolated: true\n---\nResponse",
  });
});
