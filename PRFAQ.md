# PR/FAQ: Ailly

**Ailly Energizes Prompt Testing**
*Open-source toolkit treats LLM context windows as software artifacts*

Today we are releasing Ailly, an open-source toolkit for teams building LLM agents. LLM agents fail silently when their context windows drift. A two-line edit to a system prompt can change tool-selection behavior across dozens of conversation paths. Current practice surfaces this only through manual eyeballing in a playground, or through user complaints a week after deployment.

Ailly organizes the components of a context window, the system prompts, tool schemas, AGENTS.md files, knowledge corpora, and user templates, as source controlled artifacts. It assembles them via declarative recipes, runs them against any chat-completion API, and verifies behavioral changes through a regression suite. Every run produces a self-contained, replayable artifact on disk. Every change to a component is testable against a versioned suite of assertions before it ships.

"We were shipping agent updates and finding regressions in production logs a week later," said [NAME, TITLE]. "Ailly turned that loop into a five-minute pull request action.”

A project keeps its context chunks in context/system/, context/tools/, and context/knowledge/. An assemblies/*.yaml recipe declares which chunks compose the window and in what order. ailly assemble <name> produces a deterministic context file. ailly runinvokes the model and saves response, trace, and token accounting. ailly eval runs assertions from a regression suite. ailly diff compares two runs by behavior, not by character.

"I edited one constraint, ran the eval, and saw exactly which three cases changed," said [NAME, TITLE]. "I shipped with confidence for the first time in eight months."

Ailly is available today on GitHub. The first project takes about ten minutes to set up.

## EXTERNAL FAQ

### Who is Ailly for?
Teams building LLM agents who have started to feel prompt drift. If you maintain a system prompt over 200 lines, route through several tools, and have ever asked "did that edit break anything," Ailly is for you. If you are still iterating on a single chatbot prompt with no tools, Ailly is overkill.

### How is this different from LangSmith, Promptfoo, or LangChain Eval?
Those tools treat the prompt as a string and the output as a row in a database. Ailly treats the assembled context window as the artifact, with components living in a filesystem under convention-based shapes. Runs are replayable directories. Composition is via Unix pipes. Most LLM evaluation tools test outputs; Ailly tests how the window was built.

### Does Ailly call the model for me?
Yes, through `ailly run`. It supports any provider with a chat-completions API using OpenAI API conventions. Model selection lives in the recipe, so a single flag sweeps across providers. Native providers include Anthropic, OpenAI, Amazon Bedrock, and Google Vertex. Additionally, Ailly uses huggingface/candle to provide local inference, especially valuable to verify low complexity tasks that take advantage of local or offline inference capabilities.

### Do I need to rewrite my existing prompts?
No. Ailly reads Markdown and JSON. Split your existing system prompt into focused fragments, drop them in context/system/, write a recipe. Most teams migrate in a half day.

### Does it work with AGENT.md, AGENTS.md, and CLAUDE.md?
Yes. AGENT.md is a first-class recipe field, pinned early and cached. Existing files work as-is.

### What does eval actually check?
Assertions you define. Examples: must call tool X, must not call tool Y, response matches regex, token count under N, latency under M. You write these in YAML against the structured run output. Ailly provides a number of common text primitives to apply on fields in the conversation response, as well as free-form “script” and “prompt” assertions.

### What does a Ailly project cost to run?
The toolkit is free and open-source. Model calls cost what they cost. Recipes declare cache breakpoints; teams who use them see 40 to 70 percent prompt-cost reduction versus uncached calls.

## INTERNAL FAQ

### Why "Ailly"?
This is the second iteration of Ailly, the AI Writer’s Ally. The first version was written in 2023 using TypeScript and one-file-per-call format. While this was very useful for early LLM experiments, the format and structure is insufficient for current piecemeal context window session management.

### Why these primitives?
Ailly has been searching hard for its niche. While other tools provide a range of LLM runners, coding agents, AI observability, ..., Ailly focuses on managing and understanding context windows. These are the primitives we have found that seems to provide appropriate power and flexibility for that context window niche.

### Why Unix pipes & files and not an SDK?
Pipes and files are an interoperability bet. Stdin/stdout means any language can drive Ailly. Disk artifacts mean any tool can work on them. Keeping files in source control provides substantially increased flexibility to a database system, and run datasets can be imported into data analytics platforms on a user-driven cadence.

### Why YAML recipes and not code?
Recipes are configuration, not logic. Code recipes invite branching, helpers, and shared state, all of which break reproducibility. YAML is dumb on purpose. If a project needs dynamic assembly, it generates the YAML and runs Ailly on the generated file.

### What about caching?
Recipes may declare cache breakpoints. Ailly respects provider cache topologies: stable content first, dynamic content after. Cache hits and misses are logged in the trace.

### What happens when a model is deprecated?
The recipe pins a model. meta.yaml records it for every run. When a model is retired, the suite fails on the next eval against the new model, with a diff against the last passing run. The regression set is visible immediately.

### What is explicitly out of scope?
A UI. The tool is a CLI; UIs may sit on top later. Agent runtime. Ailly does not host long-running agents. It produces and tests the windows that other runtimes consume. Prompt optimization. Ailly does not auto-tune prompts. It tells you whether your edit had the expected effect. The edit itself is yours.

### What is the biggest adoption risk?
Teams accustomed to playground iteration may resist filesystem discipline. Mitigation: the first project takes ten minutes, and the first regression catch usually settles the question.

### When is Ailly the wrong tool?
Single-prompt chatbots with no tools and no retrieval. Research prototypes where the prompt changes hourly and there are no users yet. Teams without version control discipline; Ailly inherits git's assumptions.

### What would make this fail?
Two scenarios. First, if the convention is wrong, if context/system/ and context/tools/ are not the right primary axes, projects will fight the structure. We will know within three months of public release. Second, if recipe sprawl produces more YAML than markdown, the discipline has inverted and the tool has become its own problem. Both failure modes are visible in user projects and recoverable.
