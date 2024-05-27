import { GenerateManager } from "@ailly/core/lib/actions/generate_manager.js";
import {
  loadContent,
  makeCLIContent,
  writeContent,
  type AillyEdit,
  type Content,
} from "@ailly/core/lib/content/content.js";
import { GitignoreFs } from "@ailly/core/lib/content/gitignore_fs.js";
import {
  makePipelineSettings,
  type PipelineSettings,
} from "@ailly/core/lib/index.js";
import { FileSystem } from "@davidsouther/jiffies/lib/cjs/fs.js";
import { dirname } from "node:path";
import * as vscode from "vscode";
import { deleteEdit, insert, updateSelection } from "./editor.js";
import { VSCodeFileSystemAdapter } from "./fs.js";
import { LOGGER, SETTINGS, resetLogger } from "./settings.js";
import type { StatusManager } from "./status_manager";

export interface ExtensionEdit {
  prompt: string;
  start: number;
  end: number;
}

export async function generate(
  path: string,
  {
    extensionEdit,
    manager,
    depth = 1,
  }: { extensionEdit?: ExtensionEdit; manager: StatusManager, depth?: number }
) {
  resetLogger();
  LOGGER.info(`Generating for ${path}`);

  // Prepare configuration
  const fs = new GitignoreFs(new VSCodeFileSystemAdapter());
  const stat = await fs.stat(path);
  const root = stat.isFile() ? dirname(path) : path;
  fs.cd(root);

  const engine = await SETTINGS.getAillyEngine();
  const model = await SETTINGS.getAillyModel(engine);
  if (engine === "bedrock") {
    await SETTINGS.prepareBedrock();
  }

  const settings = await makePipelineSettings({
    root,
    out: root,
    context: "folder",
    engine,
    model,
  });

  // Load content
  const [content, context] = await loadContentParts({
    fs,
    path,
    extensionEdit,
    settings,
    depth
  });

  if (content.length === 0) {
    return;
  }

  // Generate
  let generator = await GenerateManager.from(
    content.map((c) => c.path),
    context,
    settings
  );
  manager.track(generator);
  generator.start();

  const doEdit = extensionEdit && content[0].context.edit;

  if (doEdit) {
    await executeStreaming(content, content[0].context.edit!);
  }

  await generator.allSettled();
  if (content[0].meta?.debug?.finish! === "failed") {
    throw new Error(generator.formatError(content[0]));
  }

  // Write
  if (!doEdit) {
    writeContent(fs as any, content);
  }
}

async function executeEdit(
  content: Content[],
  edit: AillyEdit // { prompt: string; start: number; end: number }
) {
  const replace = content[0].response ?? "";
  await deleteEdit(edit);
  await insert(edit, "", replace);
  updateSelection(edit, replace);
}

async function executeStreaming(content: Content[], edit: AillyEdit) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  editor.selections = [];
  // Lazy spin until the request starts
  const stream = await content[0].responseStream.promise;
  let replace = "";
  let first = true;
  for await (let token of stream) {
    if (vscode.window.activeTextEditor !== editor) {
      LOGGER.debug(
        `Active window changed during streaming, stopping future updates.`
      );
      return;
    }
    if (typeof token !== "string") {
      token = new TextDecoder().decode(token);
    }
    if (first) {
      token = token.trimStart();
      await deleteEdit(edit);
      first = false;
    }
    await insert(edit, replace, token);
    replace += token;
  }
  updateSelection(edit, replace);
}

async function loadContentParts(
  {fs, path, extensionEdit, settings, depth = 1}:{
  fs: FileSystem,
  path: string,
  extensionEdit?: ExtensionEdit,
  settings: PipelineSettings
  depth?: number
  },
): Promise<[Content[], Record<string, Content>]> {
  const context = await loadContent(fs, [], settings, depth);
  const content: Content[] = Object.values(context).filter((c) =>
    c.path.startsWith(path)
  );
  if (content.length === 0) {
    return [[], {}];
  }
  if (extensionEdit) {
    const editContext: AillyEdit =
      extensionEdit.start === extensionEdit.end
        ? { file: content[0].path, after: extensionEdit.start + 1 }
        : {
            file: content[0].path,
            start: extensionEdit.start,
            end: extensionEdit.end,
          };
    content.splice(
      -1,
      1,
      makeCLIContent({
        prompt: extensionEdit.prompt,
        argContext: "folder",
        context,
        root: dirname(content[0].path),
        edit: editContext,
        isolated: true,
      })
    );
    context[content[0].path] = content[0];
    LOGGER.info(`Editing ${content.length} files`);
  } else {
    LOGGER.info(`Generating ${content.length} files`);
  }

  return [content, context];
}

export const TEST_ONLY = {
  loadContentParts,
  executeEdit,
  executeStreaming,
};
