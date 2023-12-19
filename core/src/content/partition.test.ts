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
  const b: Content = { name: "b", path: "/a/b", outPath: "/a/b", prompt: "" };
  const c: Content = { name: "c", path: "/a/c", outPath: "/a/c", prompt: "" };
  const h: Content = {
    name: "h",
    path: "/a/g/h",
    outPath: "/a/g/h",
    prompt: "",
  };
  const e: Content = { name: "e", path: "/d/e", outPath: "/d/e", prompt: "" };
  const f: Content = { name: "f", path: "/d/f", outPath: "/d/f", prompt: "" };

  const content: Content[] = [b, c, h, e, f];
  const actual = partitionPrompts(content);
  expect(actual).toEqual([[b, c], [h], [e, f]]);
});
