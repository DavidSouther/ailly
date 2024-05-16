import {
  type FileSystemAdapter,
  type Stats,
} from "@davidsouther/jiffies/lib/cjs/fs.js";
import { basename } from "path";
import { FileType, Uri, workspace } from "vscode";

export class VSCodeFileSystemAdapter implements FileSystemAdapter {
  copyFile(from: string, to: string): Promise<void> {
    workspace.fs.copy(Uri.file(from), Uri.file(to));
    return Promise.resolve();
  }

  mkdir(path: string): Promise<void> {
    return Promise.resolve(workspace.fs.createDirectory(Uri.file(path)));
  }

  readFile(path: string): Promise<string> {
    return Promise.resolve(
      workspace.fs
        .readFile(Uri.file(path))
        .then((f) => new TextDecoder().decode(f))
    );
  }

  readdir(path: string): Promise<string[]> {
    return workspace.fs.readDirectory(Uri.file(path)).then((dir) =>
      dir.map(([name, fileType]) => {
        return basename(name);
      })
    ) as Promise<string[]>;
  }

  rm(path: string): Promise<void> {
    return Promise.resolve(workspace.fs.delete(Uri.file(path)));
  }

  scandir(path: string): Promise<Stats[]> {
    return Promise.resolve(
      workspace.fs.readDirectory(Uri.file(path)).then((dir) =>
        dir.map<Stats>(([name, fileType]) => ({
          name,
          isDirectory: () => fileType === FileType.Directory,
          isFile: () => fileType === FileType.File,
        }))
      )
    );
  }

  stat(path: string): Promise<Stats> {
    return Promise.resolve(
      workspace.fs.stat(Uri.file(path)).then((stat) => ({
        name: basename(path),
        isDirectory: () => stat.type === FileType.Directory,
        isFile: () => stat.type === FileType.File,
      }))
    );
  }

  writeFile(path: string, contents: string): Promise<void> {
    return Promise.resolve(
      workspace.fs.writeFile(Uri.file(path), new TextEncoder().encode(contents))
    );
  }
}
