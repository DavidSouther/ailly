#! /usr/bin/env node
import { createInterface } from "node:readline";

import { NodeFileSystemAdapter } from "@davidsouther/jiffies/lib/cjs/fs_node.js";
import { makeArgs, help } from "./args.js";
import { loadFs, LOGGER } from "./fs.js";
import { version } from "./version.js";
import { GitignoreFs } from "@ailly/core/lib/content/gitignore_fs.js";
import { GenerateManager } from "@ailly/core/lib/actions/generate_manager.js";
import {
  AillyEdit,
  Content,
  isAillyEditReplace,
  writeContent,
} from "@ailly/core/lib/content/content.js";
import { FileSystem } from "@davidsouther/jiffies/lib/cjs/fs";

export async function main() {
  const args = makeArgs(process.argv);

  if (args.values["help"]) {
    help();
    process.exit(0);
  }

  if (args.values["version"]) {
    version();
    process.exit(0);
  }

  const fs = new GitignoreFs(new NodeFileSystemAdapter());
  const loaded = await loadFs(fs, args);

  await check_should_run(args, loaded);

  let generator = await GenerateManager.from(
    loaded.content,
    loaded.context,
    loaded.settings
  );

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
      LOGGER.info(`Starting ${loaded.content.length} requests`);
      generator.start();
      if (!args.values.stream) {
        await finish(generator);
      }
      if (last == "/dev/stdout") {
        const prompt = loaded.context[last];
        if (args.values.stream) {
          // Lazy spin until the request starts
          while (prompt.responseStream == undefined) {
            await Promise.resolve();
          }
          for await (const word of prompt.responseStream as unknown as AsyncIterable<string>) {
            process.stdout.write(word);
          }
          await finish(generator);
        }
        console.debug(`Finished prompt, final meta`, { meta: prompt.meta });
        if (prompt.meta?.debug?.finish == "failed") {
          console.error(prompt.meta.debug.error?.message);
          return;
        }
        const edit = prompt.context.edit;
        if (edit) {
          await doEdit(fs, loaded, edit, prompt, args.values.yes ?? false);
        } else {
          console.log(prompt.response);
        }
      } else {
        await writeContent(
          fs,
          loaded.content.map((c) => loaded.context[c])
        );
      }
      break;
  }
}

async function finish(generator: GenerateManager) {
  await generator.allSettled();

  const doneSummary = generator.summary();
  LOGGER.info(`All ${doneSummary.totalPrompts} requests finished`);
  if (doneSummary.errors) {
    LOGGER.warn(`Finished with errors`, { errors: doneSummary.errors });
  }
}

async function doEdit(
  fs: FileSystem,
  loaded: Awaited<ReturnType<typeof loadFs>>,
  edit: AillyEdit,
  prompt: Content,
  yes: boolean
) {
  const out = loaded.context[edit.file];
  const responseLines = (out.meta?.text ?? out.prompt)?.split("\n") ?? [];
  const replaceLines = prompt.response?.split("\n") ?? [];
  const editValue = makeEditConfirmMessage(
    edit,
    out.name,
    responseLines,
    replaceLines
  );
  console.log(editValue);
  if (!yes) {
    await check_or_exit("Continue? (y/N) ");
  }
  if (isAillyEditReplace(edit)) {
    responseLines?.splice(edit.start, edit.end - edit.start, ...replaceLines);
  } else {
    responseLines?.splice(edit.after + 1, 0, ...replaceLines);
  }
  out.response = responseLines.join("\n");
  await fs.writeFile(out.path, out.response);
}

function makeEditConfirmMessage(
  edit: AillyEdit,
  name: string,
  responseLines: string[],
  replaceLines: string[]
) {
  return (
    isAillyEditReplace(edit)
      ? [
          `Edit ${name} ${edit.start + 1}:${edit.end + 1}\n`,
          responseLines
            .slice(Math.min(edit.start - 3, 0), Math.min(edit.start, 0))
            .map((s) => ` ${s}`)
            .join("\n"),
          responseLines
            .slice(edit.start, edit.end)
            .map((s) => `-${s}`)
            .join("\n"),
          replaceLines.map((s) => `+${s}`).join("\n"),
          responseLines
            .slice(edit.end + 1, edit.end + 4)
            .map((s) => ` ${s}`)
            .join("\n"),
        ]
      : [
          `Insert into ${name} at ${edit.after + 1}\n`,
          responseLines
            .slice(Math.min(edit.after - 3, 0), Math.min(edit.after, 0))
            .map((s) => ` ${s}`)
            .join("\n"),
          replaceLines.map((s) => `+${s}`).join("\n"),
          responseLines
            .slice(edit.after + 1, edit.after + 4)
            .map((s) => ` ${s}`)
            .join("\n"),
        ]
  ).join("\n");
}

async function check_should_run(
  args: ReturnType<typeof makeArgs>,
  { content }: { content: string[] }
) {
  if (args.values.summary) {
    console.log(
      `Found ${content.length} items, estimated cost TODO: CALCULATE`
    );
    if (!args.values.yes) {
      await check_or_exit("Continue with generating these prompts? (y/N) ");
    }
  }
}

async function check_or_exit(prompt: string) {
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
  });
  const answer = await new Promise<string>((resolve) =>
    rl.question(prompt, resolve)
  );
  if (!answer.toUpperCase().startsWith("Y")) {
    process.exit(0);
  }
  rl.close();
}
