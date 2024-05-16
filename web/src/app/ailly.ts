"use server";
import { type Content } from "@ailly/core/lib/content/content";
import { makePipelineSettings } from "@ailly/core/lib/index";
import { GenerateManager } from "@ailly/core/lib/actions/generate_manager";

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

  const returnedContent: Omit<Content, "responseStream"> & {
    responseStream?: Content["responseStream"];
  } = { ...content };
  delete returnedContent.responseStream;
  return returnedContent;
}
