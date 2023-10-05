"use server";
import { join } from "path";

import { NodeFileSystemAdapter } from "@/lib/fs";
import { FileSystem } from "@davidsouther/jiffies/lib/esm/fs";

import { addMessagesToContent, loadContent } from "@/lib/content";

const adapter = new NodeFileSystemAdapter();
const fs = new FileSystem(adapter);
fs.cd(join(process.cwd(), "content"));

export async function reloadContent() {
  const content = await loadContent(fs);
  const summary = await addMessagesToContent(content);
  return [content, summary] as const;
}
