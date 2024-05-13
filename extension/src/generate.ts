import vscode from "vscode";

import * as ailly from "@ailly/core";
import { FileSystem } from "@davidsouther/jiffies/lib/esm/fs.js";
import { VSCodeFileSystemAdapter } from "./fs.js";
import {
  LOGGER,
  getAillyEngine,
  getAillyModel,
  getPreferStreamingEdit,
  resetLogger,
} from "./settings";
import { AillyEdit, Content } from "@ailly/core/src/content/content";
import { dirname } from "node:path";
import { prepareBedrock } from "./settings.js";
import { PipelineSettings } from "@ailly/core/src/ailly";

export async function generate(
  path: string,
  edit?: { prompt: string; start: number; end: number }
) {
  resetLogger();
  LOGGER.info(`Generating for ${path}`);

  // Prepare configuration
  const fs = new FileSystem(new VSCodeFileSystemAdapter());
  const root = dirname(path);
  fs.cd(root);

  const engine = await getAillyEngine();
  const model = await getAillyModel(engine);
  if (engine == "bedrock") {
    await prepareBedrock();
  }

  const settings = await ailly.Ailly.makePipelineSettings({
    root,
    out: root,
    context: "folder",
    engine,
    model,
  });

  // Load content
  const [content, context] = await loadContentParts(fs, path, edit, settings);

  // Generate
  let generator = await ailly.Ailly.GenerateManager.from(
    content.map((c) => c.path),
    context,
    settings
  );
  generator.start();

  const doEdit = edit && content[0].context.edit;
  const doStreaming = doEdit && getPreferStreamingEdit();

  if (doStreaming) {
    await executeStreaming(content, edit);
  }

  const editor = vscode.window.activeTextEditor;
  await generator.allSettled();
  if (content[0].meta?.debug?.finish! == "failed") {
    throw new Error(content[0].meta.debug?.error.message);
  }

  // Write
  if (!doStreaming) {
    if (doEdit && editor == vscode.window.activeTextEditor) {
      await executeEdit(content, edit);
    } else {
      ailly.content.write(fs as any, content);
    }
  }
}

async function executeEdit(
  content: Content[],
  edit: { prompt: string; start: number; end: number }
) {
  const replace = content[0].response ?? "";
  await deleteEdit(edit);
  await insert(edit, "", replace);
  updateSelection(edit, replace);
}

async function executeStreaming(
  content: Content[],
  edit: { prompt: string; start: number; end: number }
) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;
  editor.selections = [];
  // Lazy spin until the request starts
  while (content[0].responseStream == undefined) {
    await Promise.resolve();
  }
  let replace = "";
  let first = true;
  for await (let token of content[0].responseStream ?? []) {
    if (vscode.window.activeTextEditor != editor) {
      LOGGER.debug(
        `Active window changed during streaming, stopping future updates.`
      );
      return;
    }
    if (typeof token !== "string") token = new TextDecoder().decode(token);
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

function insert(edit: AillyEdit, after: string, token: string) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;
  const afterArr = after.split("\n");
  return editor.edit(
    (builder) => {
      const line = edit.start + afterArr.length - 1;
      const col = afterArr.at(-1)?.length ?? 0;
      const pos = new vscode.Position(line, col);
      LOGGER.debug(`Insert ${JSON.stringify(token)}`, { pos });
      builder.insert(pos, token);
    },
    { undoStopAfter: false, undoStopBefore: false }
  );
}

function deleteEdit(edit: AillyEdit) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;
  return editor.edit(
    (builder) => {
      const start = new vscode.Position(edit.start, 0);
      const end = new vscode.Position(edit.end + 1, 0);
      const range = new vscode.Range(start, end);
      LOGGER.debug(`Deleting`, { range });
      builder.delete(range);
    },
    { undoStopAfter: false, undoStopBefore: false }
  );
}

function updateSelection(edit: AillyEdit, newText: string) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;
  const newArr = newText.split("\n");
  const startLine = edit.start;
  const startCol = 0;
  const endLine = edit.start + newArr.length - 1;
  const endCol = newArr.at(-1)?.length ?? 0;
  LOGGER.debug(
    `Setting selection to [${startLine}:${startCol}, ${endLine}:${endCol}]`
  );
  editor.selection = new vscode.Selection(startLine, startCol, endLine, endCol);
}

async function loadContentParts(
  fs: FileSystem,
  path: string,
  edit: AillyEdit,
  settings: PipelineSettings
): Promise<[Content[], Record<string, Content>]> {
  const context = await ailly.content.load(fs, [], settings, 1);
  const content: Content[] = Object.values(context).filter((c) =>
    c.path.startsWith(path)
  );
  if (content.length == 0) [[], {}];
  if (edit) {
    const editContext: AillyEdit =
      edit.start === edit.end
        ? { file: content[0].path, after: edit.start + 1 }
        : { file: content[0].path, start: edit.start + 1, end: edit.end + 2 };
    content.splice(0, content.length, {
      context: {
        view: {},
        folder: [content[0].path],
        edit: editContext,
      },
      meta: {
        text: content[0].meta?.text,
      },
      path: "/dev/ailly",
      name: "ailly",
      outPath: "/dev/ailly",
      prompt: edit.prompt,
    });
    context[content[0].path] = content[0];
    LOGGER.info(`Editing ${content.length} files`);
  } else {
    LOGGER.info(`Generating ${content.length} files`);
  }

  return [content, context];
}
