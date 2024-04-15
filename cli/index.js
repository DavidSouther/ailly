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

  let generator = await ailly.Ailly.GenerateManager.from(loaded.content, loaded.context, loaded.settings);

  const last = loaded.content.at(-1);
  switch (true) {
    case args.values["update-db"]:
      await generator.updateDatabase();
      break;
    case Number(args.values["query-db"]?.length) > 0:
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

      DEFAULT_LOGGER.info("Generator all settled!");
      if (last == "/dev/stdout") {
        const prompt = loaded.context[last];
        if (prompt.meta?.debug?.finish == 'failed') {
          console.error(prompt.meta.debug.error.message);
          return;
        }
        const edit = prompt.context.edit;
        if (edit) {
          await doEdit(loaded, edit, prompt, args);
        } else {
          console.log(prompt.response);
        }
      } else {
        await ailly.content.write(loaded.fs, loaded.content.map(c => loaded.context[c]));
      }
      break;
  }
}

async function doEdit(loaded, edit, prompt, args) {
  const out = loaded.context[edit.file];
  const responseLines = out.prompt?.split("\n") ?? [];
  const replaceLines = prompt.response?.split("\n") ?? [];
  const editValue = [
    `Edit ${out.name} ${edit.start + 1}:${edit.end + 1}\n`,
    responseLines.slice(edit.start - 3, edit.start).map(s => ` ${s}`).join('\n'),
    responseLines.slice(edit.start, edit.end + 1).map(s => `-${s}`).join('\n'),
    replaceLines.map(s => `+${s}`).join("\n"),
    responseLines.slice(edit.end + 1, edit.end + 4).map(s => ` ${s}`).join('\n'),
  ].join("\n");
  console.log(editValue);
  if (!args.values.yes) {
    await check_or_exit('Continue? (y/N) ');
  }
  responseLines?.splice(edit.start, edit.end - edit.start, ...(replaceLines));
  out.response = responseLines.join("\n");
  await loaded.fs.writeFile(out.path, out.response);
}

async function check_should_run(args, { content }) {
  if (args.values.summary) {
    console.log(
      `Found ${content.length} items, estimated cost TODO: CALCULATE`
    );
    if (!args.values.yes) {
      await check_or_exit("Continue with generating these prompts? (y/N) ")
    }
  }
}

async function check_or_exit(prompt) {
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
  });
  const answer = await rl.question(prompt);
  if (!answer.toUpperCase().startsWith("Y")) {
    process.exit(0);
  }
  rl.close();
}