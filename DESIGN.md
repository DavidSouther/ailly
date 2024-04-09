# Design Decisions

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
