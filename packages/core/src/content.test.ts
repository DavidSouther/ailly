import { expect, test } from "vitest";

import {
  FileSystem,
  ObjectFileSystemAdapter,
} from "@davidsouther/jiffies/lib/esm/fs";
import { loadContent } from "./content";

test("it loads content", async () => {
  const content = await loadContent(
    new FileSystem(
      new ObjectFileSystemAdapter({
        "01p_start.md": "The quick brown",
        "20b": {
          "40p_part.md": "fox jumped",
          "56p_part_d.md": "over the lazy",
        },
        "_tweedle_dum.md": "Tweedle Dee",
        "54_a/12p_section.md": "dog.",
      })
    )
  );

  expect(content.length).toBe(4);
});
