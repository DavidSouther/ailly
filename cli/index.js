#! /usr/bin/env node
import { createInterface } from "readline/promises";

import { DEFAULT_LOGGER } from "@davidsouther/jiffies/lib/esm/log.js"
import * as ailly from "@ailly/core";
import { makeArgs, help } from "./args.js";
import { loadFs } from "./fs.js";
import { version } from "./version.js";

await main();

async function main() {
  const args = makeArgs(process.argv);

  if (args.values["help"]) {
    help();
    process.exit(0);
  }

  if (args.values["version"]) {
    version();
    process.exit(0);
  }

  const loaded = await loadFs(args);

  await check_should_run(args, loaded);

  const settings = await ailly.Ailly.makePipelineSettings(loaded.settings);
  let generator = await ailly.Ailly.GenerateManager.from(loaded.content, settings);

  switch (true) {
    case loaded.settings.updateDb:
      await generator.updateDatabase();
      break;
    case loaded.settings.queryDb.length > 0:
      // const engine = await getEngine(loaded.settings.engine);
      // const builder = await getPlugin(loaded.settings.plugin);
      // const rag = await builder.default(engine, settings);
      // const results = await rag.query(loaded.settings.queryDb);
      // console.table(
      //   results.map((v) => ({
      //     score: v.score,
      //     item: v.content.substring(0, 45).replaceAll("\n", " ") + "...",
      //   }))
      // );
      break;
    default:
      generator.start();
      await generator.allSettled();

      DEFAULT_LOGGER.info("Generated!");
      if (loaded.content.at(-1)?.outPath == "/dev/stdout") {
        console.log(loaded.content.at(-1)?.response);
      } else {
        ailly.content.write(loaded.fs, loaded.content);
      }
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