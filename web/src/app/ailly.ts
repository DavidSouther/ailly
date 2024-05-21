"use server";
import { type WritableContent } from "@ailly/core/lib/content/content";
import { makePipelineSettings } from "@ailly/core/lib/index";
import { GenerateManager } from "@ailly/core/lib/actions/generate_manager";
import { withResolvers } from "@ailly/core/src/util";

export async function generateOne(
  content: WritableContent
): Promise<WritableContent> {
  const settings = await makePipelineSettings({
    root: "/ailly",
  });
  const manager = await GenerateManager.from(
    [content.path],
    { [content.path]: { ...content, responseStream: withResolvers() } },
    settings
  );
  manager.start();
  await manager.allSettled();

  const returnedContent: WritableContent = { ...manager.threads[0][0] };
  if (returnedContent.meta?.debug) {
    returnedContent.meta.debug.lastRun = String(
      returnedContent.meta.debug.lastRun ?? ""
    );
  }
  returnedContent.meta;
  delete returnedContent.responseStream;
  return returnedContent;
}
