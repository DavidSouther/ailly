"use server";
import { type Content } from "@ailly/core/src/content/content";
import { makePipelineSettings } from "@ailly/core/src/ailly";
import { GenerateManager } from "@ailly/core/src/actions/generate_manager";

export async function generateOne(
  content: Content
): Promise<Omit<Content, "responseStream">> {
  const settings = await makePipelineSettings({
    root: "/ailly",
    // engine: "noop",
  });
  const manager = await GenerateManager.from(
    [content.path],
    { [content.path]: content },
    settings
  );
  manager.start();
  await manager.allSettled();

  const returnedContent = { ...content };
  delete returnedContent.responseStream;
  return returnedContent;
}
