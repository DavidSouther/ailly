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
  if (loaded.settings.tune) {
    await tune(loaded);
  } else {
    await generate(loaded);
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

async function generate({ fs, content, settings }) {
  console.log("Generating...");

  // Generate
  let generator = ailly.Ailly.generate(content, settings);
  generator.start();
  await generator.allSettled();

  console.log("Generated!");
  ailly.content.write(fs, content);
}

async function tune({ content, settings }) {
  return ailly.Ailly.tune(content, settings);
}
