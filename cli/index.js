#! /usr/bin/env node
import { createInterface } from "node:readline";

import { DEFAULT_LOGGER } from "@davidsouther/jiffies/lib/esm/log.js";
import { NodeFileSystemAdapter } from "@davidsouther/jiffies/lib/esm/fs_node.js";
import * as ailly from "@ailly/core";
import { makeArgs, help } from "./args.js";
import { loadFs } from "./fs.js";
import { version } from "./version.js";
import { promisify } from "node:util";

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

  const fs = new ailly.Ailly.GitignoreFs(new NodeFileSystemAdapter());
  const loaded = await loadFs(fs, args);

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
          await doEdit(fs, loaded, edit, prompt, args.values.yes ?? false);
        } else {
          console.log(prompt.response);
        }
      } else {
        await ailly.content.write(fs, loaded.content.map(c => loaded.context[c]));
      }
      break;
  }
}

/**
 * 
 * @param {import("@davidsouther/jiffies/lib/esm/fs").FileSystem} fs 
 * @param {Awaited<ReturnType<import("./fs").loadFs>>} loaded 
 * @param {import("./fs.js").Edit} edit 
 * @param {import("./fs.js").Content} prompt 
 * @param {boolean} yes 
 */
async function doEdit(fs, loaded, edit, prompt, yes) {
  const out = loaded.context[edit.file];
  const responseLines = out.prompt?.split("\n") ?? [];
  const replaceLines = prompt.response?.split("\n") ?? [];
  const editValue = makeEditConfirmMessage(edit, out.name, responseLines, replaceLines);
  console.log(editValue);
  if (!yes) {
    await check_or_exit('Continue? (y/N) ');
  }
  if (edit.after) {
    responseLines?.splice(edit.after + 1, 0, ...(replaceLines));
  } else {
    responseLines?.splice(edit.start, edit.end - edit.start, ...(replaceLines));
  }
  out.response = responseLines.join("\n");
  await fs.writeFile(out.path, out.response);
}

/**
 * 
 * @param {*} edit 
 * @param {string} name
 * @param {string[]} responseLines 
 * @param {string[]} replaceLines 
 * @returns 
 */
function makeEditConfirmMessage(edit, name, responseLines, replaceLines) {
  return (edit.after ?
    [`Insert into ${name} at ${edit.after + 1}\n`,
    responseLines.slice(Math.min(edit.after - 3, 0), Math.min(edit.after, 0)).map(s => ` ${s}`).join('\n'),
    replaceLines.map(s => `+${s}`).join("\n"),
    responseLines.slice(edit.after + 1, edit.after + 4).map(s => ` ${s}`).join('\n'),
    ]
    : [
      `Edit ${name} ${edit.start + 1}:${edit.end + 1}\n`,
      responseLines.slice(Math.min(edit.start - 3, 0), Math.min(edit.start, 0)).map(s => ` ${s}`).join('\n'),
      responseLines.slice(edit.start, edit.end).map(s => `-${s}`).join('\n'),
      replaceLines.map(s => `+${s}`).join("\n"),
      responseLines.slice(edit.end + 1, edit.end + 4).map(s => ` ${s}`).join('\n'),
    ]).join("\n");
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
  const answer = await promisify(rl.question)(prompt);
  if (!answer.toUpperCase().startsWith("Y")) {
    process.exit(0);
  }
  rl.close();
}
