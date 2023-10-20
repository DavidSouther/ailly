#! /usr/bin/env node
import { createInterface } from "readline";

import * as ailly from "@ailly/core";
import { fs, settings, content, args } from "./args.js";

async function continue_() {
  if (args.values.summary) {
    console.log(
      `Found ${content.length} items, estimated cost TODO: CALCULATE`
    );
    if (!args.values.yes) {
      const rl = createInterface({
        input: process.stdin,
        output: process.stdout,
      });
      const prompt =
        (await new Promise((resolve) =>
          rl.question("Continue with generating these prompts? (y/N) ", resolve)
        )) ?? "N";
      if (prompt.toUpperCase() !== "Y") {
        process.exit(0);
      }
    }
  }
}

async function generate() {
  console.log("Generating...");

  // Generate
  let generator = ailly.Ailly.generate(content, settings);
  generator.start();
  await generator.allSettled();

  console.log("Generated!");
  ailly.content.write(fs, content);
}

async function tune() {
  return ailly.Ailly.tune(content, settings);
}

await continue_();
if (settings.tune) {
  await tune();
} else {
  await generate();
}
