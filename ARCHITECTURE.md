# Architecture

Ailly's codebase has a roughly hexagonal shaped architecture. `@ailly/core` provides the domain logic, as well as an extension mechanism for defining engines and plugins. CLI, Web, and Extension expose that core to several surfaces.

## Core

`@ailly/core` provides the domain logic for Ailly. It loads a file system for `Content` (./core/src/content/content.ts), orchestrates that through the chosen `Engine` (./core/src/engine/engine.ts), and saves the updated Content back to disk. The two main functions to interact with Content are `loadContent` and `writeContent`, both in ./core/src/content/content.ts. There's also a utility, `makeCLIContent`, that serves as a way to make an ad-hoc one-off piece of Content.

Engines are wrappers around SDK and API calls, with a consistent interface to take a Content and stream its generation. Current Engines are OpenAI and Bedrock, though only Bedrock has been used extensively. To get an engine, call `getEngine` from ./core/src/engine/index.ts passing either a name of a known engine (see same file), or a path starting file `file://` that will be imported as a JavaScript file whose default export must align to the `Engine` interface.

Plugins are under development at this time, but generally follow the same loading process as custom engines. Call `getPlugin` from ./core/src/plugin/index.ts with a `file://` path to the javascript file whose default export implements the `Plugin` interface.

With content and an engine, a `GenerateManager` is responsible for running, retrying, etc all pieces of the pipeline. To get a `GenerateManager`, call the static method `GenerateManager.from`. `from` takes a list of which items to generate, a record of all content in the context, and additional `PipelineSettings`. The list of items must come from the keys of the context. (So, build a record of all context, and then choose the keys to generate for.) The easiest way to get a `PipelineSettings` is the `makePipelineSettings` function in ./core/src/index.ts.

After creating a `GenerateManager`, call `start` on it. While each thread is individually awaitable, the easiest approach is to just call `await manager.allSettled()`, which resolves with a PromiseSettledResult for each content in the list passed to `GenerateManager.from`. The original content in `context` will also have been updated at this point.

The File System is an abstraction provided by [`JEFRi Jiffies`](https://github.com/jefri/jiffies/blob/main/src/fs.ts), with in-memory & NodeJS backed implementations.

## Cli

The CLI does the above operations guided by user-provided flags, with some additional flourish for streaming, logging, and debugging.

## Web

The Web interface at https://ailly.dev provides a guided approach to learning prompt engineering. For developers, the `generateOne` function in ./web/src/app/ailly.ts is an accessible minimal function to make an ad-hoc API call. Note that at this point, the Content was created by the calling app, and no FileSystem is involved.

## Extension

The VSCode extension is similar to the CLI, but uses VSCode extension APIs to load and write content. Explore the [VSCode Extension API Docs](https://code.visualstudio.com/api) for more on what might be possible here.

## Integ

integ is a series of shell and batch scripts to widely exercise @ailly/core and @ailly/cli.

## Content

Sample content to show various cookbook approaches.