import {
  ObjectFileSystemAdapter,
  RecordFileSystemAdapter,
} from "@davidsouther/jiffies/lib/cjs/fs.js";
import { describe, expect, it } from "vitest";
import { GitignoreFs } from "./gitignore_fs.js";

describe("gitignore fs", () => {
  it("skips files that look like binary", async () => {
    const fs = new GitignoreFs(
      new RecordFileSystemAdapter({
        // Text files
        "/root/text1.txt": "This is a text file",
        "/root/text2.md": "# Markdown heading\n\nSome content",
        "/root/config.json": '{"key": "value"}',

        // Files that should be detected as binary
        "/root/image.png": createBinaryContentMock("PNG"),
        "/root/archive.zip": createBinaryContentMock("ZIP"),
        "/root/executable": createBinaryContentMock("ELF"),
      }),
    );

    const rootDir = await fs.readdir("/root");
    const files = expect(rootDir).toEqual([
      "config.json",
      "text1.txt",
      "text2.md",
    ]);
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
      }),
    );

    expect(await fs.readdir("/")).toEqual(["dir", "file.txt"]);
    expect(await fs.readdir("/dir")).toEqual(["deep", "file.txt"]);
    expect(await fs.readdir("/dir/deep")).toEqual(["file.txt"]);
  });

  it("doesn't filter golang (.go) files", async () => {
    const fs = new GitignoreFs(
      new ObjectFileSystemAdapter({
        "test.go": "gogo",
      }),
    );

    expect(await fs.readdir("/")).toEqual(["test.go"]);
  });
});

const PREFIX = {
  PNG: "\x89PNG\r\n\x1A\n",
  ZIP: "PK\x03\x04",
  ELF: "\x7FELF",
};

/**
 * Helper function to create mock binary content
 * Creates a string that would typically be detected as binary
 */
function createBinaryContentMock(prefix = "", random = 0): string {
  // Create string with high concentration of null bytes and non-printable characters
  let result = "\x00\x01\x02\x03\x04";

  // Add some type-specific "magic bytes" to simulate file headers

  // Add more random binary-looking content
  for (let i = 0; i < random; i++) {
    result += String.fromCharCode(Math.floor(Math.random() * 256));
  }

  return result;
}
