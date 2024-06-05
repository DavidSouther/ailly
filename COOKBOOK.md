# Ailly Cookbook

## Writing Assistant

Create an outline of a larger document or written work.
Create a folder for the content, say `campaign`.
In this folder, create a file `.aillyrc`, with a high level description of the work.
This will be used as part of the **system** prompt for every request, so typical prompt engineering practices work well.
For instance, "You are a fantasy tabletop roleplaying game master. The book is a campaign for a high-magic, high-fantasy setting."

Within this folder, create additional folders for each _chapter_ sized section of the outline.
Within each of those folders, create another `.aillyrc` file with a description of that section.
Continuing the RPG campaign example, this could be "Chapter 1 describes the overall setting. This world is called MyLandia. It is one large continent with two political factions, a republic and an empire."

Then, create one file for each _section_.
Each section will be an individually generated response from the LLM.
Files will be generated in their alphabetical order, so files (and folders) should generally use two-digit alphanumeric prefixes to determine their sort order.
For typical writing projects, this will be a file in markdown syntax with an optional YAML frontmatter block.
Add YAML frontmatter with a property, `prompt`, which will be the **human** prompt to the LLM.

`10_republic.md`:

```
  ---
  prompt: >
    The Republic is an aristocratic society founded on a shifting structure of familial wealth.
    Families elect one member to the ruling senate.
    The senate elects a consol for 18 month terms.
    Describe more about the Republic's politics and society.
  ---
```

`20_empire.md`:

```
  ---
  prompt: >
    The Empire is a strict military hierarchy across their entire society.
    Denizens are not required to join the military, but that is the only path to citizenship.
    Leadership is meritocratic, with strictly defined qualification criteria for promotion.
    Describe more about the Empire's politics and society.
  ---
```

After creating several chapter folders and section files, your content is ready for a first round of Ailly.
From the command line, `cd` into the content folder. Then, run ailly:

```
$ npx ailly
```

You should see output similar to this:

```
Loading content from /.../70_mylandia
Loading content from /.../70_mylandia/10_setting
Found 2 at or below /.../70_mylandia/10_setting
Found 2 at or below /.../70_mylandia
Generating...
Ready to generate 1 messages
Running thread for sequence of 2 prompts
Calling openai [
  {
    role: 'system',
    content: 'You are a fantasy tabletop roleplaying game master...',
    tokens: undefined
  },
  {
    role: 'user',
    content: 'The Republic is an aristocratic society founded on...',
    tokens: undefined
  }
]
...
```

Review the generated files. Make any edits you want. Then, if you want to rerun just one file (based on, say, changing its prompt or editing earlier responses), provide that path on the command line.

```
$ npx ailly 10_setting/20_empire.md

Loading content from /.../70_mylandia
Loading content from /.../70_mylandia/10_setting
Found 2 at or below /.../70_mylandia/10_setting
Found 2 at or below /.../70_mylandia
Generating...
Ready to generate 1 messages
Running thread for sequence of 1 prompts
Calling openai [
  {
    role: 'system',
    content: 'You are a fantasy tabletop roleplaying game master...',
    tokens: undefined
  },
  {
    role: 'user',
    content: 'The Republic is an aristocratic society founded on...',
    tokens: undefined
  },
  {
    role: 'assistant',
    content: 'The Republic, known officially as the Resplendent ...',
    tokens: undefined
  },
  {
    role: 'user',
    content: 'The Empire is a strict military hierarchy across t...',
    tokens: undefined
  }
]
Response from OpenAI for 20_empire.md { id: 'chatcmpl-1234', finish_reason: 'stop' }
```

## Editing Files

Ailly's `--edit` flag enables specific functionality to achieve good edits to documents.
It employs several tricks to reduce the model's explanations, focusing only on getting replacement or insertion text. For the Claude family, this includes using the Haiku model by default, as well as limiting responses to the first markdown code fence ```.

`ailly --edit` requires specifying the lines to edit, either as `[start]:[stop]` which are 1-based includes/exclusive replacement markers, or as `[start]:` or `:[stop]` as 1-based insert after or before, respectively.

Ailly will show the proposed change and ask for confirmation before editing the file, though the prompt can be suppressed by passing `--yes` on the command line.

Here's an example, using the `createUserPoolHandler.js` file from `content/40_ramdectomy`.

```
% ailly --context folder --prompt "Rewrite the storeUserPoolMeta function to use vanilla javascript without using Ramda." createUserPoolHandler.js --edit --lines 14:19
Edit createUserPoolHandler.js 14:19

 import { createUserPoolClient } from "../../../actions/create-user-pool-client.js";
 import { join } from "ramda";

-const storeUserPoolMeta = (...args) => {
-  const tmp = getTmp(FILE_USER_POOLS);
-  const entry = join(",", args);
-  setTmp(FILE_USER_POOLS, tmp ? `${tmp}\n${entry}` : entry);
-};
+const storeUserPoolMeta = (...args) => {
+  const tmp = getTmp(FILE_USER_POOLS);
+  const entry = args.join(",");
+  setTmp(FILE_USER_POOLS, tmp ? `${tmp}\n${entry}` : entry);
+};

 const validateUserPool = (poolName) => {
   if (!poolName) {

Continue? (y/N) y

% git diff .
diff --git a/content/40_ramdectomy/createUserPoolHandler.js b/content/40_ramdectomy/createUserPoolHandler.js
index 5bc1fdc..54fe19f 100644
--- a/content/40_ramdectomy/createUserPoolHandler.js
+++ b/content/40_ramdectomy/createUserPoolHandler.js
@@ -13,7 +13,7 @@ import { join } from "ramda";

 const storeUserPoolMeta = (...args) => {
   const tmp = getTmp(FILE_USER_POOLS);
-  const entry = join(",", args);
+  const entry = args.join(",");
   setTmp(FILE_USER_POOLS, tmp ? `${tmp}\n${entry}` : entry);
 };
```

## Using a template
This command takes advantage of Ailly's templating feature.

In `path/to/javascript-prompt.yaml` there is a `code.tests` property with information common to a project's tests:
```yaml
...
code:
  tests: |
      <tests>
        For writing unit tests remember that we are using Vitest. Do not import any mocking libraries other than those provided by vitest. Vitest requires that test methods (describe, it, expect, etc.) are explicitly imported. They are not added to the global environment. If you're not sure about the vitest API at any time, please let me know rather than making assumptions. Be sure to use the latest EcmaScript, not Typescript.
        While vitest allows for globals, we must explicitly import any vitest functions.
        
        Be judicious in your mocking. For example, things like "path.join" probably don't need to be mocked
        because it's a pure function with no i/o.
      </tests>
```

That context can be added to a prompt by passing the file path to `--template-view` and using [mustache](https://mustache.github.io/) syntax in the prompt:

```bash
ailly --context folder --template-view path/to/javascript-prompts.yaml \
      --prompt "{{code.tests}} \n Implement the unit test getObject."
```

## How many files or folders?

This is a bit of an experimental / trial by doing issue.
Generally, late 2024 LLMs (Llama, Claude, ChatGPT) respond with "around" 700 words.
Similarly, while their context sizes vary, they seem to work best with "up to" 16,000 words.
Based on these limits, up to about 5 layers of folder depth and at most 20 files per folder generate the most reliably "good" results, but your milage will vary.

## Statistics on prior runs

This node.js script shows how to load the previously run Ailly content and extract its meta and debug information to perform statistics. For instance, reviewing the statistics of a recent run of the entire content/ directory using Haiku, we found that R=0.9995 correlation between input token count to first token response latency.

```
const ailly = require('@ailly/core');
const { NodeFileSystemAdapter } = require('@davidsouther/jiffies/lib/cjs/fs_node.js');
const fs = new ailly.GitignoreFs(new NodeFileSystemAdapter());
fs.cd(process.cwd());
ailly.LOGGER.level = 3; // Turn off logging
loaded = await ailly.content.loadContent(fs, [], {}, Number.MAX_SAFE_INTEGER);
content = Object.values(loaded)
content.filter(c => !c.skip && c.meta?.debug?.id).map(c => ({ id: c.meta.debug.id, ...c.meta.debug.statistics }));
```

input:first 1.000
input:invocation 0.995
input:delta 0.926
output:first 0.938
output:invocation 0.961
output:delta 0.987

https://docs.google.com/spreadsheets/d/1qqCvNvivJutGnppln2xHpc8lqJlwIjb8erSt-upVo-Y/view
