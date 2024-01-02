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
  // const settings = ailly.Ailly.makePipelineSettings(loaded.settings);

  await check_should_run(args, loaded);

  switch (true) {
    case loaded.settings.updateDb:
      // await ailly.Ailly.updateDatabase(loaded.content, settings);
      break;
    case loaded.settings.queryDb.length > 0:
      // const engine = await getEngine(loaded.settings.engine);
      // const builder = getPlugin(loaded.settings.plugin);
      // const rag = await builder(engine, settings);
      // const results = await rag.query(loaded.settings.queryDb);
      // console.table(
      //   results.map((v) => ({
      //     score: v.score,
      //     item: v.content.substring(0, 45).replaceAll("\n", " ") + "...",
      //   }))
      // );
      break;
    default:
      await generate(loaded);
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
        "Continue with generating these prompts? (y/N) "
      );
      if (!prompt.toUpperCase().startsWith("Y")) {
        process.exit(0);
      }
    }
  }
}

async function generate({ fs, content, settings }) {
  console.log("Generating...");

  // Generate

  let generator = await ailly.Ailly.GenerateManager.from(content, settings);
  generator.start();
  await generator.allSettled();

  console.log("Generated!");
  ailly.content.write(fs, content);
}
