"use server";
import { join } from "path";

import {
  Content,
  NodeFileSystemAdapter,
  addMessagesToContent,
  generateAll,
  generateOne,
  loadContent,
  tune,
} from "@ailly/core";
import { FileSystem } from "@davidsouther/jiffies/lib/esm/fs";

const adapter = new NodeFileSystemAdapter();
const fs = new FileSystem(adapter);
fs.cd(join(process.cwd(), process.env["CONTENT"] || "content"));

export async function reloadContent() {
  const content = await loadContent(fs);
  const summary = await addMessagesToContent(content);
  return [content, summary] as const;
}

export async function generateAllAction() {
  await generateAll();
  return { message: "Generating" };
}

export async function generateOneAction(content: Content) {
  await generateOne(content);
  return { message: "Generated" };
}

export async function tuneAction() {
  await tune();
  return { message: "Tuning" };
}
