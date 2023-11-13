#! /usr/bin/env node
import { createInterface } from "readline/promises";

import * as ailly from "@ailly/core";
import { makeArgs, help } from "./args.js";
import { loadFs } from "./fs.js";

await main();

async function main() {
  const args = makeArgs(process.argv);

  if (args.values["help"]) {
    help();
    process.exit(1);
  }

  const loaded = await loadFs(args);

  await check_should_run(args, loaded);

  const plugin = await ailly.Ailly.getPlugin(args.values.engine ?? "openai");
  const rag = await ailly.Ailly.RAG.build(plugin, args.values.root);
  switch (true) {
    case loaded.settings.updateDb:
      await ailly.Ailly.updateDatabase(loaded.content, rag);
      break;
    case loaded.settings.tune:
      await tune(loaded);
      break;
    case loaded.settings.augment:
      await ailly.Ailly.augment(loaded.content, rag);
    default:
      await generate(loaded, rag);
      break;
  }
}

async function check_should_run(args, { content }) {
  if (args.values.summary) {
    console.log(
      `Found ${content.length} items, estimated cost TODO: CALCULATE`
    );
    if (!args.values.yes) {
      const rl = createInterface({
        input: process.stdin,
        output: process.stdout,
      });
      const prompt = await rl.question(
        "Continue with generating these prompts? (y/N) ",
        resolve
      );
      if (!prompt.toUpperCase().startsWith("Y")) {
        process.exit(0);
      }
    }
  }
}

async function generate({ fs, content, settings }, rag) {
  console.log("Generating...");

  // Generate
  let generator = new ailly.Ailly.GenerateManager(content, settings, rag);
  generator.start();
  await generator.allSettled();

  console.log("Generated!");
  ailly.content.write(fs, content);
}

async function tune({ content, settings }) {
  return ailly.Ailly.tune(content, settings);
}
