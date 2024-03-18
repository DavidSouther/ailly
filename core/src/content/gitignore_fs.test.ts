import { ObjectFileSystemAdapter } from "@davidsouther/jiffies/lib/esm/fs.js";
import { GitignoreFs } from "./gitignore_fs";
import { describe, expect, it } from "vitest";

describe("gitignore fs", () => {
  it("reads while obeying .gitignores", async () => {
    const fs = new GitignoreFs(
      new ObjectFileSystemAdapter({
        file: "abc",
        skip: "def",
        ".gitignore": "skip\nskipdir",
        ".git": {
          objects: {
            aa: {
              "012345": "code",
            },
          },
        },
        dir: {
          file: "ghi",
          skip: "def2",
          deep: {
            ".gitignore": "other",
            file: "jkl",
            skip: "still skipped",
            other: "skipped",
          },
        },
        skipdir: {
          file: "abc",
        },
      })
    );

    expect(await fs.readdir("")).toEqual([".gitignore", "dir", "file"]);
    fs.cd("dir");
    expect(await fs.readdir("")).toEqual(["deep", "file"]);
    fs.cd("deep");
    expect(await fs.readdir("")).toEqual([".gitignore", "file"]);
  });
});
