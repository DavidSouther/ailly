import { test, expect } from "vitest";
import { partitionPrompts } from "./partition";
import { Content } from "./content";

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
    view: {},
  };
  const c: Content = {
    name: "c",
    path: "/a/c",
    outPath: "/a/c",
    prompt: "",
    view: {},
  };
  const h: Content = {
    name: "h",
    path: "/a/g/h",
    outPath: "/a/g/h",
    prompt: "",
    view: {},
  };
  const e: Content = {
    name: "e",
    path: "/d/e",
    outPath: "/d/e",
    prompt: "",
    view: {},
  };
  const f: Content = {
    name: "f",
    path: "/d/f",
    outPath: "/d/f",
    prompt: "",
    view: {},
  };

  const content: Content[] = [b, c, h, e, f];
  const actual = partitionPrompts(content);
  expect(actual).toEqual([[b, c], [h], [e, f]]);
});
