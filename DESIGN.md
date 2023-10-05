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
