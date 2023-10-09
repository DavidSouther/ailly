"use server";
import { join } from "path";

import * as ailly from "@ailly/core";
import { NodeFileSystem } from "@davidsouther/jiffies/lib/esm/fs_node";
import { format } from "@ailly/core/src/plugin/openai";
import { GenerateManager } from "@ailly/core/src/ailly";
import { makeGenerateManagerOrganizer } from "./generate_manager_organizer";
import { Content, loadContent, writeContent } from "@ailly/core/src/content";

const CONTENT_ROOT = join(process.cwd(), process.env["CONTENT"] || "content");
const fs = new NodeFileSystem(CONTENT_ROOT);
const organizer = makeGenerateManagerOrganizer();

export async function reloadContent() {
  const content = await ailly.content.load(fs);
  const summary = await format(content);
  return [content, summary] as const;
}

export interface StartMessage {
  id: string;
  message: string;
}

export async function generatorStatus(
  id: string
): Promise<GenerateManager | undefined> {
  return organizer.get(id);
}

async function start(content: Content[]): Promise<StartMessage> {
  const [id, manager] = await organizer.start(content);
  console.log(`Starting new manager ID ${id} with ${content.length} messages`);
  manager.start();
  manager.allSettled().then((promises) => {
    let content = promises
      .filter(
        (p): p is PromiseFulfilledResult<Content> => p.status == "fulfilled"
      )
      .map((p) => p.value);
    console.log(`Finished generating content`);
    writeContent(fs, content);
  });
  return { message: `Started generator on ${content.length} pieces`, id };
}

export async function generateAllAction() {
  const content = await loadContent(fs);
  return start(content);
}

export async function generateOneAction(content: Content) {
  return start([content]);
}

export async function tuneAction() {
  // await tune();
  return { message: "Tuning" };
}
