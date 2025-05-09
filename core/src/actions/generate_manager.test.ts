import { expect, test } from "vitest";
import type { Content } from "../content/content.js";
import { partitionPrompts } from "./generate_manager.js";

/*

/a
  /b.
  /c.
  /g
    /h.
/d
  /e.
  /f.

=>

/a/b.
/a/c.
/a/g/h.
/d/e.
/d/f.

=> 

[
  /a/b.
  /a/c.
]
[
  /a/g/h.
]
[
  /d/e.
  /d/f.
]
*/

test("partitioning", () => {
  const b: Content = {
    name: "b",
    path: "/a/b",
    outPath: "/a/b",
    prompt: "",
    context: {
      view: {},
    },
  };
  const c: Content = {
    name: "c",
    path: "/a/c",
    outPath: "/a/c",
    prompt: "",
    context: {
      view: {},
    },
  };
  const h: Content = {
    name: "h",
    path: "/a/g/h",
    outPath: "/a/g/h",
    prompt: "",
    context: {
      view: {},
    },
  };
  const e: Content = {
    name: "e",
    path: "/d/e",
    outPath: "/d/e",
    prompt: "",
    context: {
      view: {},
    },
  };
  const f: Content = {
    name: "f",
    path: "/d/f",
    outPath: "/d/f",
    prompt: "",
    context: {
      view: {},
    },
  };

  const content: Content[] = [b, c, h, e, f];
  const actual = partitionPrompts(
    content.map((c) => c.path),
    content.reduce(
      (a, c) => {
        a[c.path] = c;
        return a;
      },
      {} as Record<string, Content>,
    ),
  );
  expect(actual).toEqual([[b, c], [h], [e, f]]);
});
