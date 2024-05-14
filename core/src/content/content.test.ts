import { expect, test, describe } from "vitest";

import {
  FileSystem,
  ObjectFileSystemAdapter,
} from "@davidsouther/jiffies/lib/esm/fs.js";
import {
  Content,
  loadContent,
  writeContent,
  splitOrderedName,
  loadAillyRc,
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

  expect(Object.keys(content)).toEqual([
    "/01_start.md",
    "/20b/40_part.md",
    "/20b/56_part.md",
    "/54_a/12_section.md",
  ]);
  expect(content).toEqual({
    "/01_start.md": {
      name: "01_start.md",
      path: "/01_start.md",
      outPath: "/01_start.md.ailly.md",
      prompt: "The quick brown",
      response: undefined,
      context: {
        system: [],
        view: {},
      },
      meta: {
        combined: false,
        root: "/",
        parent: "root",
        text: "The quick brown",
      },
    },
    "/20b/40_part.md": {
      name: "40_part.md",
      path: "/20b/40_part.md",
      outPath: "/20b/40_part.md.ailly.md",
      prompt: "fox jumped",
      response: undefined,
      context: {
        system: [],
        view: {},
      },
      meta: {
        combined: false,
        root: "/",
        parent: "root",
        text: "fox jumped",
      },
    },
    "/20b/56_part.md": {
      name: "56_part.md",
      path: "/20b/56_part.md",
      outPath: "/20b/56_part.md.ailly.md",
      prompt: "over the lazy",
      response: undefined,
      context: {
        system: [],
        view: {},
        predecessor: "/20b/40_part.md",
      },
      meta: {
        combined: false,
        root: "/",
        parent: "root",
        text: "over the lazy",
      },
    },
    "/54_a/12_section.md": {
      name: "12_section.md",
      path: "/54_a/12_section.md",
      outPath: "/54_a/12_section.md.ailly.md",
      prompt: "dog.",
      response: undefined,
      context: {
        system: [],
        view: {},
      },
      meta: { combined: false, root: "/", parent: "root", text: "dog." },
    },
  });
});

test("it loads responses", async () => {
  const testFs = new FileSystem(
    new ObjectFileSystemAdapter({
      "01_start.md": "The quick brown",
      "01_start.md.ailly.md": "fox jumped",
    })
  );
  const content = await loadContent(testFs);

  expect(content["/01_start.md"].prompt).toEqual("The quick brown");
  expect(content["/01_start.md"].response).toEqual("fox jumped");
});

test.each([
  ["prompt.md", "prompt"],
  ["prompt.md.ailly.md", "response"],
])("it splits ordered filenames: %s -> %s", (file: string, type: string) => {
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
  expect(content).toEqual({
    "/content.md": {
      name: "content.md",
      path: "/content.md",
      outPath: "/content.md",
      prompt: "content",
      response: "Response",
      context: {
        system: [],
        view: {},
      },
      meta: {
        isolated: true,
        combined: true,
        root: "/",
        parent: "root",
        text: "---\nprompt: content\n---\nResponse",
      },
    },
    "/prompt.md": {
      name: "prompt.md",
      path: "/prompt.md",
      outPath: "/prompt.md",
      prompt: "prompt",
      response: undefined,
      context: {
        system: [],
        view: {},
      },
      meta: {
        isolated: true,
        combined: true,
        root: "/",
        parent: "root",
        text: "---\nprompt: prompt\n---",
      },
    },
  });
});

test("it writes combined prompt and responses", async () => {
  const fs = new FileSystem(new ObjectFileSystemAdapter({}));

  const content: Content[] = [
    {
      name: "content.md",
      path: "/content.md",
      outPath: "/content.md",
      prompt: "content",
      response: "Response",
      context: {
        system: [],
        view: {},
      },
      meta: { isolated: true, combined: true, root: "/", parent: "root" },
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
  expect(content).toEqual({
    "/content.md": {
      name: "content.md",
      path: "/content.md",
      outPath: "/content.md.ailly.md",
      prompt: "content",
      response: "Response",
      context: {
        system: [],
        view: {},
      },
      meta: {
        isolated: true,
        combined: false,
        root: "/",
        parent: "root",
        text: "content",
      },
    },
    "/prompt.md": {
      name: "prompt.md",
      path: "/prompt.md",
      outPath: "/prompt.md.ailly.md",
      prompt: "prompt",
      response: undefined,
      context: {
        system: [],
        view: {},
      },
      meta: {
        isolated: true,
        combined: false,
        root: "/",
        parent: "root",
        text: "prompt",
      },
    },
  });
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
  expect(content).toEqual({
    "/root/content.md": {
      name: "content.md",
      path: "/root/content.md",
      outPath: "/out/content.md.ailly.md",
      prompt: "content",
      response: "Response",
      context: {
        system: [],
        view: {},
      },
      meta: {
        isolated: true,
        combined: false,
        root: "/root",
        out: "/out",
        parent: "root",
        text: "content",
      },
    },
    "/root/prompt.md": {
      name: "prompt.md",
      path: "/root/prompt.md",
      outPath: "/out/prompt.md.ailly.md",
      prompt: "prompt",
      response: undefined,
      context: {
        system: [],
        view: {},
      },
      meta: {
        isolated: true,
        combined: false,
        root: "/root",
        out: "/out",
        parent: "root",
        text: "prompt",
      },
    },
  });
});

test("it writes separate prompt and responses", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      "content.md": "---\ncombined: false\nisolated: true\n---\ncontent",
    })
  );

  const content = [
    {
      name: "content.md",
      path: "/content.md",
      outPath: "/content.md.ailly.md",
      prompt: "content",
      response: "Response",
      context: {
        system: [],
        view: {},
      },
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

  const content = [
    {
      name: "content.md",
      path: "/root/content.md",
      outPath: "/out/content.md.ailly.md",
      prompt: "content",
      response: "Response",
      context: {
        system: [],
        view: {},
      },
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

  expect(content).toEqual({
    "/root/src/com/example/Main.java": {
      name: "Main.java",
      path: "/root/src/com/example/Main.java",
      outPath: "/out/src/com/example/Main.java.ailly.md",
      prompt: "class Main {}\n",
      response: undefined,
      context: {
        system: [],
        view: {},
      },
      meta: {
        combined: false,
        out: "/out",
        root: "/root",
        parent: "root",
        text: "class Main {}\n",
      },
    },
  });

  content["/root/src/com/example/Main.java"].response = "Response";

  await writeContent(fs, [...Object.values(content)]);

  expect((fs as any).adapter.fs).toEqual({
    "/root/.gitignore": "target",
    "/root/src/com/example/Main.java": "class Main {}\n",
    "/root/target/com/example/Main.class": "0xCAFEBABE",
    "/out/src/com/example/Main.java.ailly.md":
      "---\ncombined: false\n---\nResponse",
  });
});

describe("Load aillyrc", () => {
  describe("parent = root (default)", () => {
    test("at root with no .aillyrc in cwd", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {},
        })
      );
      fs.cd("/root");

      const [system] = await loadAillyRc(fs, [], {});

      expect(system.map((s) => s.content)).toEqual([]);
    });

    test("at root with .aillyrc in cwd", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            ".aillyrc": "system",
          },
        })
      );
      fs.cd("/root");

      const [system] = await loadAillyRc(fs, [], {});

      expect(system.map((s) => s.content)).toEqual(["system"]);
    });

    test("below root with no .aillyrc", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            ".aillyrc": "system",
            below: {},
          },
        })
      );
      fs.cd("/root/below");

      const [system] = await loadAillyRc(
        fs,
        [{ content: "root", view: {} }],
        {}
      );

      expect(system.map((s) => s.content)).toEqual(["root"]);
    });

    test("below root with .aillyrc", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            ".aillyrc": "root",
            below: {
              ".aillyrc": "below",
            },
          },
        })
      );
      fs.cd("/root/below");

      const [system] = await loadAillyRc(
        fs,
        [{ content: "root", view: {} }],
        {}
      );

      expect(system.map((s) => s.content)).toEqual(["root", "below"]);
    });
  });

  describe("parent = always", () => {
    test("at root with .aillyrc in cwd parent", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            ".aillyrc": "root",
            below: {
              ".aillyrc": "below",
            },
          },
        })
      );
      fs.cd("/root/below");

      const [system] = await loadAillyRc(fs, [], { parent: "always" });

      expect(system.map((s) => s.content)).toEqual(["root", "below"]);
    });

    test("at root with .aillyrc in cwd parent x2", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            ".aillyrc": "root",
            below: {
              ".aillyrc": "---\nparent: always\n---\nbelow",
              deep: {
                ".aillyrc": "---\nparent: always\n---\ndeep",
              },
            },
          },
        })
      );
      fs.cd("/root/below/deep");

      const [system] = await loadAillyRc(fs, [], { parent: "always" });

      expect(system.map((s) => s.content)).toEqual(["root", "below", "deep"]);
    });

    test("at root without .aillyrc in cwd parent", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            ".aillyrc": "root",
            below: {
              deep: {
                ".aillyrc": "---\nparent: always\n---\ndeep",
              },
            },
          },
        })
      );
      fs.cd("/root/below/deep");

      const [system] = await loadAillyRc(fs, [], { parent: "always" });

      expect(system.map((s) => s.content)).toEqual(["deep"]);
    });

    test("below root with system context", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            ".aillyrc": "root",
            below: {
              deep: {
                ".aillyrc": "---\nparent: always\n---\ndeep",
              },
            },
          },
        })
      );
      fs.cd("/root/below/deep");

      const [system] = await loadAillyRc(fs, [{ content: "below", view: {} }], {
        parent: "always",
      });

      expect(system.map((s) => s.content)).toEqual(["below", "deep"]);
    });
  });

  describe("parent = never", () => {
    test("below root with .aillyrc", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            ".aillyrc": "root",
            below: {
              ".aillyrc": "below",
            },
          },
        })
      );
      fs.cd("/root/below");

      const [system] = await loadAillyRc(fs, [{ content: "root", view: {} }], {
        parent: "never",
      });

      expect(system.map((s) => s.content)).toEqual(["below"]);
    });

    test("below root without .aillyrc", async () => {
      const fs = new FileSystem(
        new ObjectFileSystemAdapter({
          root: {
            below: {},
          },
        })
      );
      fs.cd("/root/below");

      const [system] = await loadAillyRc(fs, [{ content: "root", view: {} }], {
        parent: "never",
      });

      expect(system.map((s) => s.content)).toEqual([]);
    });
  });

  test("context = none removes all context", async () => {
    const fs = new FileSystem(
      new ObjectFileSystemAdapter({
        root: {
          ".aillyrc": "system",
          a: "a",
          "a.ailly.md": "aa",
          b: "b",
        },
      })
    );
    fs.cd("/root");

    const content = await loadContent(fs, [], { context: "none" });

    expect(content).toEqual({
      "/root/a": {
        name: "a",
        path: "/root/a",
        outPath: "/root/a.ailly.md",
        prompt: "a",
        response: "aa",
        context: {
          view: {},
        },
        meta: {
          combined: false,
          context: "none",
          parent: "root",
          root: "/root",
          text: "a",
        },
      },
      "/root/b": {
        name: "b",
        path: "/root/b",
        outPath: "/root/b.ailly.md",
        prompt: "b",
        response: undefined,
        context: {
          view: {},
        },
        meta: {
          combined: false,
          context: "none",
          parent: "root",
          root: "/root",
          text: "b",
        },
      },
    });
  });

  test("context = folder uses folders, not predecessors", async () => {
    const fs = new FileSystem(
      new ObjectFileSystemAdapter({
        root: {
          ".aillyrc": "system",
          a: "a",
          "a.ailly.md": "aa",
          b: "b",
          "b.ailly.md": "bb",
          c: "c",
          "c.ailly.md": "cc",
        },
      })
    );
    fs.cd("/root");

    const content = await loadContent(fs, [], { context: "folder" });

    expect(content).toEqual({
      "/root/a": {
        name: "a",
        path: "/root/a",
        outPath: "/root/a.ailly.md",
        prompt: "a",
        response: "aa",
        context: {
          system: [{ content: "system", view: {} }],
          folder: ["/root/a", "/root/b", "/root/c"],
          view: {},
        },
        meta: {
          combined: false,
          context: "folder",
          parent: "root",
          root: "/root",
          text: "a",
        },
      },
      "/root/b": {
        name: "b",
        path: "/root/b",
        outPath: "/root/b.ailly.md",
        prompt: "b",
        response: "bb",
        context: {
          system: [{ content: "system", view: {} }],
          folder: ["/root/a", "/root/b", "/root/c"],
          view: {},
        },
        meta: {
          combined: false,
          context: "folder",
          parent: "root",
          root: "/root",
          text: "b",
        },
      },
      "/root/c": {
        name: "c",
        path: "/root/c",
        outPath: "/root/c.ailly.md",
        prompt: "c",
        response: "cc",
        context: {
          system: [{ content: "system", view: {} }],
          folder: ["/root/a", "/root/b", "/root/c"],
          view: {},
        },
        meta: {
          combined: false,
          context: "folder",
          parent: "root",
          root: "/root",
          text: "c",
        },
      },
    });
  });
});

test("it loads template views for prompts", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      root: {
        "content.md":
          "---\nview:\n  foo: bar\n---\n{{output.prose}}\nBrainstorm an ad campaign.",
      },
    })
  );

  const content = await loadContent(fs, [], {});

  expect(content["/root/content.md"].prompt).toEqual(
    "{{output.prose}}\nBrainstorm an ad campaign."
  );
  expect(content["/root/content.md"].context.view).toEqual({ foo: "bar" });
});

test("it loads template views from system files", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      ".aillyrc": "---\nview:\n  whiz: 12\n---\nprompt",
      root: {
        "content.md":
          "---\nview:\n  foo: bar\n---\n{{output.prose}}\nBrainstorm an ad campaign.",
      },
    })
  );

  const content = await loadContent(fs, [], {});

  expect(content["/root/content.md"].context.system?.[0].content).toEqual(
    "prompt"
  );
  expect(content["/root/content.md"].context.system?.[0].view).toEqual({
    whiz: 12,
  });
});

test("it loads template-view in .aillyrc", async () => {
  const fs = new FileSystem(
    new ObjectFileSystemAdapter({
      root: {
        ".aillyrc": "---\ntemplate-view: ../view.yaml\n---",
        "content.md": "{{view}}",
      },
      "view.yaml": "view: foo",
    })
  );
  const content = await loadContent(fs);

  expect(content["/root/content.md"].context.system?.[0].view).toEqual({
    view: "foo",
  });
});
