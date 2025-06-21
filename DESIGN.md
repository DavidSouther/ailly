# AIlly, an AI writing assistant by David Souther

My authoring approach is text-based.
My writing style builds from bullet points.
I want to write prose that is minimally annotated, yet expressive across media.
I can use generative AI tooling to augment my writing style.

This authoring tool will allow me trivial static content publishing (including interactive materials).
Content will be version controlled.
It will use the content to tune generative AI on my voice, and I will edit generative AI's suggestions to expand content.
It can be forked and run by other writers with basic programming skills.
It can be augmented and developed by anyone with JavaScript programming experience.

This project will showcase "good", and it is "cheap" in that I'll do it myself on my own time.
The codebase aims for usable for myself, with the deployed site being broadly accessible.
This project will be extensible, and I intend to release the generative AI portion as a standalone utility at a future date.
The website is static content, and will be deployed to a scalable provider.

I am the author & driver, but the content is globally available.
Readers of this site will see it as a reflection of myself.
I may seek help for UX design, but I will retain overall architecture and software design.
I am building it on my own.
If colleagues want to contribute to portions beyond ideas and encouragement, they can fork and run their own.
I will consider pull requests that add features or fix bugs.
I am paying hosting and related costs myself.

## Constraints

I have no deadline for this project.
I will contribute to this as a lower priority for my time.
For the forseeable timeframe, the code will be run only via next.js. I may add a CLI in the future, and any folder structure details will be specified in a way that is amenable to a Python or other CLI implementation.

## Context & Scope

Adam Owada's Prompt Engineering site is an inspiration, and I'm sure we'll remain in contact.
I'll probably ask Gabo for help & feedback on design issues.
Several colleagues will provide additional feedback on the software and content, as requested.
At this time I will use OpenAI's GPT family of models for generative ai features. I may try to move this to a local llama based model at some point.
This starts with no dependents, though I do anticipate releasing the tooling around content generation and voice learning.

## Solution Strategy

The content will be plaintext markdown files in a version controlled folder structure.
Authoring tooling runs as an npm cli command.
Options are available as cli arguments, plugin extension points, and as per-file settings.
Tooling for the authoring experience will be dev only, will load content files as necessary, and will use node or NextJS server side execution environments to call LLM APIs.
Engines will be available at least for AWS Bedrock and OpenAI API.
Publishing will use NextJS static site generation utilities to build & export files appropriate for deploying using GitHub Pages.

## Containers

### Authoring

The NextJS dev environment will have protected pages that are only exposed during development.
These routes will have access to the project's local file system.
It will read and write files directly, and will have some git awareness to block operations when the repo is dirty.
Authoring proper will happen using text editors of choice.

Generative AI features will use engines to allow multiple LLM backends.
The project will use a base model, and may fine tune that model using version controlled prompts and responses.
The responses are the markdown content that will get published to the site.
The prompts are stored adjacent their generated content, and can be edited same as content.
Fine-tuning will happen when requested, and be tied to a specific git commit of the project's content.
A section of content will get generated from appropriate prompt text & that file written to the local disk.
It can then be edited by the author, before making a git commit with that content.

### Publishing

Static routes for all content will be registered & generated via NextJS.
Content will be read as markdown files, processed with off-the-shelf and custom plugins, and saved in a self-contained directory with all files necessary for static HTTP publishing.
Markdown and plugins will run during build time, however, some injected content like source code and diagrams wil be sent as plain text and evaluated for highlighting and SVG generation in the readers' browser
Site style, themes, and look & feel are by David.

## Runtime

### Authoring

- David writes and edits prompt and content files.
- ~Ailly reads git statuses, to prevent OpenAI operations while the working tree is dirty.~
- ~Ailly reads prompt and content files to prepare model fine-tuning runs.~
- Ailly reads prompt files to request generated content, and writes that content to disk.

### Publishing

- NextJS has routes for content.
- NextJS reads content files & renders them as markdown.
- Markdown-AND plugin adds support for `&[ref];` as a way to inject arbitrary file and url contents as plain text.
- In the browser, javascript sends analytics data to Google Analytics.
- In the browser, javascript highlights source code blocks & generates SVG images for diagrams.

## Deployment

- NextJS writes static website files.
- GitHub Actions publishes static website files.

## Architectural Decisions

NextJS is React framework with leading (circa mid-2023) support for React Server Components.
This makes it a good choice for this style of edge computing.
The published end result is static HTML, but NextJS can expose dev systems and UIs when run locally.
While the generative AI portions could be run as a CLI, writing that in Node would add unrelated overhead to the project.
Such a CLI could also be written in Python, and perhaps a Python implementation of the resulting spec will be a useful tool, but for now I'm starting from NextJS so let's see where NextJS takes me.

OpenAI's GPT models are comfortable and familiar, and the APIs "just work" (so far).
AWS 
Learning to deploy and run a llama model would add significant overhead to getting started on the interesting parts, the authoring experience.
Perhaps when the Python CLI version comes, it'll have a module that allows switching between OpenAI API and a local llama.

## Quality Commitments

~The only person with a face to get an egg is me, so, that's my own quality bar.~
As Ailly begins to hit wider usage, unit and integration tests cases are becoming increasingly important. Need to integration test on both *nix and Windows.

## Risks and Technical Debt

I'm doing this as a hobby project, which has a long history of starting strong and losing steam.
Let's see how much I like using this tool, how much people want me to be writing, and how much feedback I get on wanting to write with it.

### Glossary

| Term        | Definition                                          |
| ----------- | --------------------------------------------------- |
| NextJS      | https://nextjs.org                                  |
| OpenAI API  | https://platform.openai.com/docs/guides/gpt         |
| Fine Tuning | https://platform.openai.com/docs/guides/fine-tuning |


# Design Decisions

The following are some design decisions that were documented at the time.

## Base Platform

NextJS is React framework with leading (circa mid-2023) support for React Server Components.
This makes it a good choice for this style of edge computing.
The published end result is static HTML, but NextJS can expose dev systems and UIs when run locally.
While the generative AI portions could be run as a CLI, writing that in Node would add unrelated overhead to the project.
Such a CLI could also be written in Python, and perhaps a Python implementation of the resulting spec will be a useful tool, but for now I'm starting from NextJS so let's see where NextJS takes me.

## Generative AI Tool

OpenAI's GPT models are comfortable and familiar, and the APIs "just work" (so far).
Learning to deploy and run a llama model would add significant overhead to getting started on the interesting parts, the authoring experience.
Perhaps when the Python CLI version comes, it'll have a module that allows switching between OpenAI API and a local llama.

## Bedrock and --engine

The Content to Message translation is fairly consistent.
APIs are HTTP calls.
Provide a `bedrock` engine for calling AWS Bedrock, and allow user to choos eny enabled model.

## CLI Pivot

The core Ailly project only really cares about a FileSytem abstraction.
For easier deployment, and to better manage very large contexts, a CLI front end works very well.

```
npm install @ailly/cli
ailly .
```

## Templates

Provide a templating mechanism and expose views.
Templates use Mustache syntax.
Templates get their views by collecting `View` objects along the content load path.

## Edit

Ailly should provide a line-based mechanism to edit files in situ.
Ailly will provide context from the current contents to the LLM with a user prompt.
The prompt will also include instructions and settings to encourage the LLM to only respond with new content for that portion of the file.
Then, replace the chosen portion with the LLM's response.

```
ailly --edit --lines 10,20 file.js --prompt 'Change it to be more like so'
```

## Extension

Ailly's edit mode works great on the command line.
Expose it as an editor extension.
Prompt for the changes, include the contents same as CLI, and use the users's current selection (or cursor) to decide where to edit.
This intentional approach to editing is very different from CoPilot's and Q's inline completions.

## Ailly.dev

Repurpose the website and domain for a Prompt Engineering playground and training site.
Users are guided through several prompt engineering exercises that show how small changes to the system prompt cause huge changes to the output.
Allow a final "playground" mode when they've completed the main course.

## Tool Use as Content

Ailly's core "one file one prompt" faces challenges in the Tool Use paradigm.
One prompt call can result in multiple LLM tool use blocks.
Putting the back and forth as an array in the meta head isn't great user experience, as it's too hard to parse where the tool use calls align in the document itself.

Ailly could use yaml's --- syntax to indicate multiple documents, but this clases with Markdown's --- for &lt;hr> section breaks.
Further, tool Use can include large responses, that aren't specifically related to the LLM's response but are necessary to understand the LLM's context window.

One option seems to be expanding one "call" into several files, with each tool call being a call and response.
This has a downside of keeping the final response separated across a number of files, but otherwise "looks like" current Ailly prompt files.

Ailly could store the primary response in a single file, and insert hrefs to tool use files.
This seems to handle the "one file one prompt" approach well, without needlessly burdening the user with inline tool responses.
However, Ailly needs to include some information in situ for the primary prompt file to coordinate where to insert the prompts.
If we insist that Ailly responses are always markdown syntax, there are some interesting mechanisms for this.

The simplest approach would be to use reference-style links, and then put the tool use details (the tool, parameters, and response) in that file.
When loading content, Ailly would need to look for these references, and re-build the messages for the content from those positions.

```
[tool-use:tool-use-1]: ./file-tool-use.md
```

> (The below example shouldn't render in a markdown tool.)

[tool-use:tool-use-1]: ./file-tool-use.md

However, this requires the reader to go to the response file to view the tool called and its parameters.

Another option would be to use an HTML comment block, and similarly remove it.
This would allow a YAML format in the primary file, but keep the response separate.

```
<!-- TOOL-USE
id: id123
tool: add
parameters: [2, 5]
response: ./file-tool-use.md
-->
```

In MDAST, this is

```
{
  type: 'definition',
  identifier: 'tool-use:tool-use-1',
  label: 'tool-use:tool-use-1',
  title: null,
  url: './file-tool-use.md',
  position: [Object]
}```

> (The below example, again, shouldn't render in a markdown tool.)

<!-- TOOL-USE
id: id123
tool: add
parameters: [2, 5]
response: ./file-tool-use.md
-->

Using `remark`, this looks like:

```
{
  type: 'html',
  value: '<!-- TOOL-USE\n' +
    'id: id123\n' +
    'tool: add\n' +
    'parameters: [2, 5]\n' +
    'response: ./file-tool-use.md\n' +
    '-->',
  position: [Object]
}```