import * as vscode from "vscode";
// import { FileSystem } from "@davidsouther/jiffies/lib/esm/fs";
import { VSCodeFileSystemAdapter } from "./fs";
import type * as AillyT from "@ailly/core";
// import type * as fsNode from "@davidsouther/jiffies/lib/esm/fs_node";

export async function generate(path: string) {
  // CommonJS <> ESM Shenanigans
  const Ailly = (await eval('import("@ailly/core")')) as typeof AillyT;
  const { FileSystem } = await eval(
    'import("@davidsouther/jiffies/lib/esm/fs.js")'
  );

  console.log(`Generating for ${path}`);
  const apiKey = await getOpenAIKey();
  if (!apiKey) {
    return;
  }
  console.log(`apikey is ${apiKey}`);

  const fs = new FileSystem(new VSCodeFileSystemAdapter());
  // TODO: Work with multi-workspace editors
  fs.cd(vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? process.cwd());

  // Load content
  let content = await Ailly.content.load(fs as any);
  content = content.filter((c) => c.path.startsWith(path));
  console.log(`Generating ${content.length} files`);

  // Generate
  let generator = Ailly.Ailly.generate(content, { apiKey });
  generator.start();
  await generator.allSettled();

  // Write
  Ailly.content.write(fs as any, content);
}

async function getOpenAIKey(): Promise<string | undefined> {
  if (process.env["OPENAI_API_KEY"]) {
    return process.env["OPENAI_API_KEY"];
  }
  let aillyConfig = vscode.workspace.getConfiguration("ailly");
  if (aillyConfig.has("openai-api-key")) {
    const key = aillyConfig.get<string>("openai-api-key");
    if (key) {
      return key;
    }
  }
  const key = await vscode.window.showInputBox({
    title: "Ailly: OpenAI API Key",
    prompt: "API Key from OpenAI for requests",
  });
  aillyConfig.update("openai-api-key", key);
  return key;
}
