"use server";

import { Ailly, types } from "@ailly/core";

export async function generateOne(content: types.Content) {
  // const settings = await Ailly.makePipelineSettings({ root: "/ailly" });
  // const manager = await Ailly.GenerateManager.from(
  //   [content.path],
  //   { [content.path]: content },
  //   settings
  // );
  // manager.start();
  // await manager.allSettled();
  content.response = "Did some stuff";
}
