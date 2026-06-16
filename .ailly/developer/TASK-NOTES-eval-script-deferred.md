# TASK-NOTES: eval-script deferred decisions

Carried over from the `2026-05-30-A-eval-script` design doc, "Summary & Deferred Items" section. Each item is a deliberate non-implementation: the `script` / `program` subprocess executor works as designed without it, and the trigger condition for revisiting is recorded below.

## Per-assertion timeout override (`timeout_ms` schema field)

Every subprocess assertion shares the compile-time `SCRIPT_TIMEOUT_DEFAULT` (30s). There is no per-assertion override, so a legitimately slow checker cannot ask for more time, and a fast one cannot tighten the bound.

**Revisit when:** a script legitimately needs more than 30 seconds. When added, `timeout_ms: 0` and negative values must be rejected at parse time as `Malformed` — a zero or negative timeout must never silently mean "no timeout." The default stays a compile-time constant until then, so there is no zero-value footgun.

## Suite-level `pass_env` defaults and a denylist

Per-assertion `pass_env` ships in this slice: each checker names exactly the variables it receives from the otherwise-cleared child env. There is no suite-level default list and no denylist of variables that can never be passed through.

**Revisit when:** a suite has many checkers sharing one key (a suite-level default would reduce repetition), or an operator wants to hard-block a variable regardless of what an assertion names (a denylist). Both are additive on top of the cleared base allowlist.

## Hermetic `Program` resolution

`Program` names resolve against the verbatim parent `PATH` today (bare names via PATH, relative against `cwd = project_root`, absolute as-is). Resolution is therefore non-hermetic across environments.

**Revisit when:** a cross-environment consumer needs reproducible resolution. Likely shapes: pinning a minimal `PATH`, or rejecting bare names in favour of project-relative / absolute paths. Interpreter-name pinning (`AILLY_PYTHON` / `AILLY_NODE`) folds into the runtime-version-pinning item below.

## Runtime-version pinning via project config

The Python and Node interpreters resolve from `AILLY_PYTHON` / `AILLY_NODE` (defaulting to `python3` / `node`) read from the trusted parent env. There is no project-config mechanism to pin a specific non-system runtime.

**Revisit when:** the first project needs a non-system `python3` or `node`.

## Streaming stdout into the report

Captured stdout is buffered (bounded by `SCRIPT_OUTPUT_CAP_BYTES`) and classified after the child exits; it is not streamed incrementally.

**Revisit when:** a long-running script use case wants to see output before the subprocess completes.

## Opt-in shared script library outside project root

`Script::Path` is confined to the project root today — a path that canonicalizes outside the root is `Malformed`, and the runner is never called.

**Revisit when:** a real cross-project consumer needs a shared checker library. The shape is an explicit opt-in (e.g. a configured library root that the containment check also accepts) rather than relaxing the default confinement.

## Approach-2 refactor (ScriptContext bundle on `EvaluationContext`)

The executor takes `script_runner` and `project_root` as two separate `EvaluationContext` fields. Approach 2 bundles env-var overrides and timeout into one `ScriptContext` struct for cleaner extensibility.

**Revisit when:** a third script-side knob lands on `EvaluationContext` (Three-Strikes). Until then two fields are simpler than a bundle.

## Full stderr surfacing in the report

stderr is warn-logged once, and on the broken-checker path (`Errored`) its prefix already populates the report reason. The full stderr stream is not surfaced in a structured report field.

**Revisit when:** an eval-author asks to see the complete stderr stream. The likely shape is a structured sidecar field on the per-match record rather than folding the whole stream into the reason string.
