# Content Folders

1. Ordering
2. Prompts vs Content
3. Jiffdown

## Ordering

Content files in the file system will get combined into a work as a whole.
A work can be a book or an article (possibly a series or a journal in the future).
Content within a work is ordered by using unix-style two-character numeric prefixes in the file or folder name.
When presenting a work, content must be displayed in that numeric sort order.
When creating the entire text, the content within a folder is presented before the content in the next sorted file or folder.
Filenames that begin with an \_, or otherwise do not start with a numeric prefix, are not included in the content.

For example, with this file system:

```
01_start.md -> The quick brown
20b/40_part.md -> fox jumped
20b/56_part_d.md -> over the lazy
_tweedle_dum.md -> Tweedle Dee
54_a/12_section.md -> dog.
```

The final content would be

```
The quick brown
fox jumped
over the lazy
dog.
```

Each file will be parsed as markdown individually.

### gray matter

Files named `_s.md` in a folder are "system" files, which are used to create a system prompt for AI training.
`_s.md` files and prompt files are parsed for gray matter in the head.

| Field    | Usage                                                                                                         |
| -------- | ------------------------------------------------------------------------------------------------------------- |
| skip     | Don't show this folder or file in the content pane, and don't generate for it or include it in training data. |
| isolate  | Don't include the prior content in the folder in the context window.                                          |
| training | Don't include this file or folder in fine-tuning training data.                                               |

### Sections

Within a work, sections are created in two ways.
Nesting folders creates a new section of work.
Within a markdown file, subsections are created for that section at each level of `#` heading.

Each top-level folder creates a `chapter` section.
Within a chapter, one level of folder nesting allows creating sections within the chapter.
Additional layers create further sub-section nesting.
Markdown files, when parsed, will have their headings reset at the appropriate nesting level.
This may result in some levels being below the `h6` nesting threshold for large works.

> ! TODO Consider `Series` to combine books and `Part` to combine chapters.

> ! TODO Implement the nesting using the [document outline algorithm](https://html.spec.whatwg.org/multipage/sections.html#outline).
> See https://css-tricks.com/document-outline-dilemma/.

## Prompts vs Content

When generating data and performing fine-tuning, Gen AI models need content tagged as prompt and response.
In some Llama interfaces, this is performed with in-text token markers, eg `<START_Q>{question}<END_Q><START_A>{answer}<END_A>`.
In OpenAI, this is done using a list of `message`s, with the prompt roles being `system` and `user`, and the fine-tuned response as role `assistant`.
Fine Tuning uses a list of complete conversations starting with one or more `system` messages, followed by alternating rounds of `user` and `assistant` messages, ending with an `assistant` message.
Generative completions use a list of incomplete conversation messages with one or more `system` messages, and then alternating `user` and `assistant` messages, ending with a `user` message.
OpenAI then returns with the anticipated `assistant` message.

> `message` is `{"role": "system"|"user"|"assistant", "content": "<string>"}`

To separate content into prompt and response, and to be immediately obvious to authors, the letters `s` (system), `p` (prompt, `user`), and `r` (response, `assistant`) can be used immediately after the first `_` for ignored files, and the numbers, for content.
The rest of the filenames must be the same, to line up sessions.

> The example from https://platform.openai.com/docs/guides/gpt/chat-completions-api:

```
_s.md -> You are a helpful assistant.
01p_who_won.md -> Who won the world series in 2020?
01r_who_won.md -> The Los Angeles Dodgers won the world series in 2020.
02p_where_played.md -> Where was it played?
```

> The example from https://platform.openai.com/docs/guides/fine-tuning/preparing-your-dataset

```
_s.md -> Marv is a factual chatbot that is also sarcastic.
01_capital/01p_capital.md -> What's the capital of France?
01_capital/01r_capital.md -> Paris, as if everyone doesn't know that already.
02_author/01p_author.md -> Who wrote 'Romeo and Juliet'?
02_author/01r_author.md -> Oh, just some guy named William Shakespeare. Ever heard of him?
03_distance/01p_distance.md -> How far is the Moon from Earth?
03_distance/01r_distance.md -> Around 384,400 kilometers. Give or take a few, like that really matters.
```

When asked to generated the next file, the tool will start by walking to the root of the project making a list of `system` message from `_s` files.
It will then include all `p` and `r` files up to this point, following the ordering rules.
The returned generated content will get written to a file with the name of the final `p` file, replacing the `p` with an `r`.

When sending fine-tuning sessions, the P/R pairs will be included in multiple instances.
With three sections, the fine-tuning call will send ([P1, R1], [P1, R1, P2, R2], [P1, R1, P2, R2, P3, R3]), to train each of the responses in context.

> ! There is a 4k token limit on fine-tuning examples for GPT-3.5-Turbo. Provide a warning when approaching this limit.

> _During the training process this conversation will be split, with the final entry being the completion that the model will produce, and the remainder of the messages acting as the prompt. Consider this when building your training examples - if your model will act on multi-turn conversations, then please provide representative examples so it doesn’t perform poorly when the conversation starts to expand._ https://github.com/openai/openai-cookbook/blob/main/examples/How_to_finetune_chat_models.ipynb

## [Jiffdown](https://github.com/jefri/jiffdown)

> Inline, block, and reference extensions to Markdown.

Run `jifdown` during build to result in static HTML (or as appropriate).
Jiffdown references are imported as plain text, and then escaped appropriately for the output target.
For HTML publishing, code and diagrams should be annotated for [`highlight`](https://highlightjs.org/) and [mermaid](https://mermaid.js.org/).
