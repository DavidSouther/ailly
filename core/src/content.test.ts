import { expect, test } from "vitest";

import {
  FileSystem,
  ObjectFileSystemAdapter,
} from "@davidsouther/jiffies/lib/esm/fs";
import { loadContent } from "./content";

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
