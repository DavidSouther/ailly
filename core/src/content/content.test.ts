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
import { GitignoreFs } from "./gitignore_fs";

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
  expect(content.map((c) => c.path)).toEqual([
    "/01_start.md",
    "/20b/40_part.md",
    "/20b/56_part.md",
    "/54_a/12_section.md",
  ]);
  expect(content).toEqual([
    {
      name: "01_start.md",
      path: "/01_start.md",
      outPath: "/01_start.md.ailly.md",
      prompt: "The quick brown",
      response: "",
      system: [""],
      meta: { combined: false, root: "/" },
    },
    {
      name: "40_part.md",
      path: "/20b/40_part.md",
      outPath: "/20b/40_part.md.ailly.md",
      prompt: "fox jumped",
      response: "",
      system: ["", ""],
      meta: { combined: false, root: "/" },
    },
    {
      name: "56_part.md",
      path: "/20b/56_part.md",
      outPath: "/20b/56_part.md.ailly.md",
      prompt: "over the lazy",
      response: "",
      system: ["", ""],
      meta: { combined: false, root: "/" },
      predecessor: {
        name: "40_part.md",
        path: "/20b/40_part.md",
        outPath: "/20b/40_part.md.ailly.md",
        prompt: "fox jumped",
        response: "",
        system: ["", ""],
        meta: { combined: false, root: "/" },
      },
    },
    {
      name: "12_section.md",
      path: "/54_a/12_section.md",
      outPath: "/54_a/12_section.md.ailly.md",
      prompt: "dog.",
      response: "",
      system: ["", ""],
      meta: { combined: false, root: "/" },
    },
  ]);
});

test("it loads responses", async () => {
  const testFs = new FileSystem(
    new ObjectFileSystemAdapter({
      "01_start.md": "The quick brown",
      "01_start.md.ailly.md": "fox jumped",
    })
  );
  const content = await loadContent(testFs);

  expect(content.length).toBe(1);
  expect(content[0].prompt).toEqual("The quick brown");
  expect(content[0].response).toEqual("fox jumped");
});

test.each([
  ["prompt.md", "prompt"],
  ["prompt.md.ailly.md", "response"],
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
      meta: { isolated: true, combined: true, root: "/" },
    },
    {
      name: "prompt.md",
      path: "/prompt.md",
      outPath: "/prompt.md",
      prompt: "prompt",
      response: "",
      system: [""],
      meta: { isolated: true, combined: true, root: "/" },
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
      "content.md.ailly.md": "Response",
    })
  );

  const content = await loadContent(fs);
  expect(content.length).toBe(2);
  expect(content).toEqual([
    {
      name: "content.md",
      path: "/content.md",
      outPath: "/content.md.ailly.md",
      prompt: "content",
      response: "Response",
      system: [""],
      meta: { isolated: true, combined: false, root: "/" },
    },
    {
      name: "prompt.md",
      path: "/prompt.md",
      outPath: "/prompt.md.ailly.md",
      prompt: "prompt",
      response: "",
      system: [""],
      meta: { isolated: true, combined: false, root: "/" },
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
        "content.md.ailly.md": "Response",
      },
    })
  );

  const content = await loadContent(fs, [], {
    root: "/root",
    out: "/out",
  });
  expect(content).toEqual([
    {
      name: "content.md",
      path: "/root/content.md",
      outPath: "/out/content.md.ailly.md",
      prompt: "content",
      response: "Response",
      system: ["", ""],
      meta: { isolated: true, combined: false, root: "/root", out: "/out" },
    },
    {
      name: "prompt.md",
      path: "/root/prompt.md",
      outPath: "/out/prompt.md.ailly.md",
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
      outPath: "/content.md.ailly.md",
      prompt: "content",
      response: "Response",
      system: [""],
      meta: { isolated: true, combined: false },
    },
  ];

  await writeContent(fs, content);

  expect((fs as any).adapter.fs).toEqual({
    "/content.md": "---\ncombined: false\nisolated: true\n---\ncontent",
    "/content.md.ailly.md":
      "---\ncombined: false\nisolated: true\n---\nResponse",
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
      outPath: "/out/content.md.ailly.md",
      prompt: "content",
      response: "Response",
      system: [""],
      meta: { isolated: true, combined: false },
    },
  ];

  await writeContent(fs, content);

  expect((fs as any).adapter.fs).toEqual({
    "/root/content.md": "---\ncombined: false\nisolated: true\n---\ncontent",
    "/out/content.md.ailly.md":
      "---\ncombined: false\nisolated: true\n---\nResponse",
  });
});

test("it writes deep java prompts and responses", async () => {
  const fs = new GitignoreFs(
    new ObjectFileSystemAdapter({
      root: {
        ".gitignore": "target",
        src: {
          com: {
            example: {
              "Main.java": "class Main {}\n",
            },
          },
        },
        target: {
          com: {
            example: {
              "Main.class": "0xCAFEBABE",
            },
          },
        },
      },
    })
  );

  const content = await loadContent(fs, [], {
    root: "/root",
    out: "/out",
  });

  expect(content).toEqual([
    {
      name: ".gitignore",
      path: "/root/.gitignore",
      outPath: "/out/.gitignore.ailly.md",
      prompt: "target",
      response: "",
      system: ["", ""],
      meta: { combined: false, out: "/out", root: "/root" },
    },
    {
      name: "Main.java",
      path: "/root/src/com/example/Main.java",
      outPath: "/out/src/com/example/Main.java.ailly.md",
      prompt: "class Main {}\n",
      response: "",
      system: ["", "", "", "", ""],
      meta: { combined: false, out: "/out", root: "/root" },
    },
  ]);

  content[0].response = "Response";
  content[1].response = "Response";

  await writeContent(fs, content);

  expect((fs as any).adapter.fs).toEqual({
    "/root/.gitignore": "target",
    "/root/src/com/example/Main.java": "class Main {}\n",
    "/root/target/com/example/Main.class": "0xCAFEBABE",
    "/out/.gitignore.ailly.md": "---\ncombined: false\n---\nResponse",
    "/out/src/com/example/Main.java.ailly.md":
      "---\ncombined: false\n---\nResponse",
  });
});
