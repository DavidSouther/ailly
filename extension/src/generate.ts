import * as vscode from "vscode";
import {
  loadContent,
  writeContent,
  type AillyEdit,
  type Content,
} from "@ailly/core/lib/content/content.js";
import {
  makePipelineSettings,
  type PipelineSettings,
} from "@ailly/core/lib/ailly.js";
import { GenerateManager } from "@ailly/core/lib/actions/generate_manager.js";
import { FileSystem } from "@davidsouther/jiffies/lib/cjs/fs.js";
import { VSCodeFileSystemAdapter } from "./fs.js";
import { LOGGER, Settings, resetLogger } from "./settings.js";
import { dirname } from "node:path";
import { deleteEdit, insert, updateSelection } from "./editor.js";

export interface ExtensionEdit {
  prompt: string;
  start: number;
  end: number;
}

export async function generate(path: string, extensionEdit?: ExtensionEdit) {
  resetLogger();
  LOGGER.info(`Generating for ${path}`);

  // Prepare configuration
  const fs = new FileSystem(new VSCodeFileSystemAdapter());
  const stat = await fs.stat(path);
  const root = stat.isFile() ? dirname(path) : path;
  fs.cd(root);

  const engine = await Settings.getAillyEngine();
  const model = await Settings.getAillyModel(engine);
  if (engine === "bedrock") {
    await Settings.prepareBedrock();
  }

  const settings = await makePipelineSettings({
    root,
    out: root,
    context: "folder",
    engine,
    model,
  });

  // Load content
  const [content, context] = await loadContentParts(
    fs,
    path,
    extensionEdit,
    settings
  );

  if (content.length === 0) {
    return;
  }

  // Generate
  let generator = await GenerateManager.from(
    content.map((c) => c.path),
    context,
    settings
  );
  generator.start();

  const doEdit = extensionEdit && content[0].context.edit;
  const doStreaming = doEdit && Settings.getPreferStreamingEdit();

  if (doStreaming) {
    await executeStreaming(content, content[0].context.edit!);
  }

  const editor = vscode.window.activeTextEditor;
  await generator.allSettled();
  if (content[0].meta?.debug?.finish! === "failed") {
    throw new Error(
      content[0].meta?.debug?.error?.message ?? "unknown failure"
    );
  }

  // Write
  if (!doStreaming) {
    if (doEdit && editor === vscode.window.activeTextEditor) {
      await executeEdit(content, content[0].context.edit!);
    } else {
      writeContent(fs as any, content);
    }
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
  while (content[0].responseStream === undefined) {
    await Promise.resolve();
  }
  let replace = "";
  let first = true;
  for await (let token of content[0]
    .responseStream as unknown as AsyncIterable<string>) {
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
  fs: FileSystem,
  path: string,
  extensionEdit: ExtensionEdit | undefined,
  settings: PipelineSettings
): Promise<[Content[], Record<string, Content>]> {
  const context = await loadContent(fs, [], settings, 1);
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
      prompt: extensionEdit.prompt,
    });
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
