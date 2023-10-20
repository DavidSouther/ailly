import { expect, test } from "vitest";

import {
  FileSystem,
  ObjectFileSystemAdapter,
} from "@davidsouther/jiffies/lib/esm/fs.js";
import { loadContent, splitOrderedName } from "./content.js";

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
