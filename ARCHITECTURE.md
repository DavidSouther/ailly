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
