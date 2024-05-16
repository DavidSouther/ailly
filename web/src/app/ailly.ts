"use server";

import { Content } from "@ailly/core/src/content/content";
import { makePipelineSettings } from "@ailly/core/src/index";
import { GenerateManager } from "@ailly/core/src/actions/generate_manager";

export async function generateOne(content: Content) {
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
  // content.response = "Did some stuff";
  return content;
}
