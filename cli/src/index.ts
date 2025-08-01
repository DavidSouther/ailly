import { createInterface } from "node:readline";

import { GenerateManager } from "@ailly/core/lib/actions/generate_manager.js";
import {
  type AillyEdit,
  type Content,
  isAillyEditReplace,
  writeContent,
} from "@ailly/core/lib/content/content.js";
import { GitignoreFs } from "@ailly/core/lib/content/gitignore_fs.js";
import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert.js";
import type { FileSystem } from "@davidsouther/jiffies/lib/cjs/fs.js";
import { NodeFileSystemAdapter } from "@davidsouther/jiffies/lib/cjs/fs_node.js";

import { PROMPT_THREAD_ONE_DONE } from "@ailly/core/lib/actions/prompt_thread";
import { type Args, help, makeArgs } from "./args.js";
import { LOGGER, loadFs } from "./fs.js";
import { version } from "./version.js";

export async function main() {
  const args = makeArgs(process.argv);

  if (args.values.help) {
    help();
    process.exit(0);
  }

  if (args.values.version) {
    version();
    process.exit(0);
  }

  const fs = new GitignoreFs(new NodeFileSystemAdapter());
  const loaded = await loadFs(fs, args);

  await check_should_run(args, loaded);

  const generator = await GenerateManager.from(
    loaded.content,
    loaded.context,
    loaded.settings,
  );

  const isStdOut = loaded.content.at(-1) === "/dev/stdout";
  switch (true) {
    case args.values["update-db"]:
      await generator.updateDatabase();
      break;
    case args.values.clean:
      writeContent(
        fs,
        loaded.content.map((name) => loaded.context[name]),
        { clean: true },
      );
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
    default: {
      LOGGER.info(`Starting ${loaded.content.length} requests`);

      generator.events.on(
        PROMPT_THREAD_ONE_DONE,
        ({ content }: { content: Content }) => {
          if (content.meta?.debug?.finish !== "failed") {
            writeContent(fs, [content]);
          }
        },
      );

      generator.start();

      if (isStdOut) {
        loaded.content.splice(-1, 1);
        const prompt = loaded.context["/dev/stdout"];
        const edit = prompt.context.edit;
        if (!edit) {
          const stream = await assertExists(prompt.responseStream).promise;
          for await (const word of stream) {
            if (word) {
              // ChatGPT sends a final `undefined`
              process.stdout.write(word);
            }
          }
          process.stdout.write("\n");
        }
        await finish(generator);
        LOGGER.debug("Finished prompt, final meta", { meta: prompt.meta });
        if (prompt.meta?.debug?.finish === "failed") {
          LOGGER.debug("Prompt run error", { debug: prompt.meta.debug });
          const error = generator.formatError(prompt) ?? "Unknown failure";
          console.error(error);
        } else if (edit) {
          await doEdit(fs, loaded, edit, prompt, args.values.yes ?? false);
        }
      }

      await finish(generator);
      const errors = generator
        .errors()
        .filter((c) => c.content.name !== "/dev/stdout");
      if (errors.length > 0 && !isStdOut) {
        console.error(
          [
            "There were errors when generating responses:",
            ...errors.map(
              (err) => `  ${err.content.name}: ${err.errorMessage}`,
            ),
          ].join("\n"),
        );
      }
    }
  }
}

async function finish(generator: GenerateManager) {
  await generator.allSettled();

  const doneSummary = generator.summary();
  LOGGER.info(`All ${doneSummary.totalPrompts} requests finished`);
  if (doneSummary.errors) {
    LOGGER.warn("Finished with errors", { errors: doneSummary.errors });
  }
}

async function doEdit(
  fs: FileSystem,
  loaded: Awaited<ReturnType<typeof loadFs>>,
  edit: AillyEdit,
  prompt: Content,
  yes: boolean,
) {
  const out = loaded.context[edit.file];
  const responseLines = (out.meta?.text ?? out.prompt)?.split("\n") ?? [];
  const replaceLines = prompt.response?.split("\n") ?? [];
  const editValue = makeEditConfirmMessage(
    edit,
    out.name,
    responseLines,
    replaceLines,
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
  replaceLines: string[],
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
  args: Args,
  { content }: { content: string[] },
) {
  if (args.values.summary) {
    console.log(
      `Found ${content.length} items, estimated cost TODO: CALCULATE`,
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
    rl.question(prompt, resolve),
  );
  if (!answer.toUpperCase().startsWith("Y")) {
    process.exit(0);
  }
  rl.close();
}
