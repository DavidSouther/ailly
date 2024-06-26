import { ObjectFileSystemAdapter } from "@davidsouther/jiffies/lib/cjs/fs.js";
import { describe, expect, it } from "vitest";
import { GitignoreFs } from "./gitignore_fs.js";

describe("gitignore fs", () => {
  it("skips files with no extension", async () => {
    const fs = new GitignoreFs(
      new ObjectFileSystemAdapter({
        "file.txt": "abc",
        skip: "skip",
      })
    );

    expect(await fs.readdir("/")).toEqual(["file.txt"]);
  });
  it("reads while obeying .gitignores", async () => {
    const fs = new GitignoreFs(
      new ObjectFileSystemAdapter({
        "file.txt": "abc",
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
          "file.txt": "ghi",
          skip: "def2",
          deep: {
            ".gitignore": "other",
            "file.txt": "jkl",
            skip: "still skipped",
            other: "skipped",
          },
        },
        skipdir: {
          "file.txt": "abc",
        },
      })
    );

    expect(await fs.readdir("/")).toEqual(["dir", "file.txt"]);
    expect(await fs.readdir("/dir")).toEqual(["deep", "file.txt"]);
    expect(await fs.readdir("/dir/deep")).toEqual(["file.txt"]);
  });
  it("doesn't filter golang (.go) files", async () => {
    const fs = new GitignoreFs(
      new ObjectFileSystemAdapter({
        "test.go": "gogo",
      })
    );

    expect(await fs.readdir("/")).toEqual(["test.go"]);
  });
});
