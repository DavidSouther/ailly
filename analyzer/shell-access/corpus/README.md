# shell-access corpus

Command *shapes* the classifier has to get right, written on synthetic paths.

These cases are inspired by local Claude, Codex, and Pi sessions and copied from
none of them. A real session is a private sketchbook: read it to notice which
wrappers, redirects, script-first utilities, globs, and inline interpreters
actually occur, then rewrite the example on a made-up path. No recorded command,
working directory, session id, address, hostname, or home directory belongs here.

This is not the same thing as `shell/tests/fixtures/*.jsonl`, which are harness
loader fixtures and stay unrelated whole-session JSONL.

## Layout

One file per shape family, so a reviewer opens one concern at a time. The file
names are grouping, not schema — every file has the same two documents.

| File | Holds |
|---|---|
| `readers.yaml` | reader operands, script-first `sed`/`awk`/`rg`/`grep` |
| `redirects.yaml` | `>`, `>>`, `<`, `2>`, `2>&1`, tree-sitter-bash issue #233 |
| `writes.yaml` | `tee`, `cp`/`mv`, in-place flags, `touch` |
| `deletes.yaml` | `rm`, `rmdir` |
| `directories.yaml` | `ls`, `find`, `du`, `mkdir` — operands that name a directory |
| `wrappers.yaml` | `env`, `sudo`, `bash -lc`, `timeout`, pipelines, `&&`/`;` lists |
| `ambiguities.yaml` | globs, expansions, command substitution, heredocs |
| `scripting.yaml` | `python3 -c`, `perl -e`, `node -e`, `ruby -e` |
| `unattributed.yaml` | utilities outside the table, and text that does not parse |

## Case format

Document one is an optional **header**, carrying `cwd` for every case in the
file. Document two is a map keyed by **case id**, unique across all files.

```yaml
cwd: /work/app
---
reader-redirect-append:
  command: "cat config/base.yml >> build/out.env"
  tags: [read, write, redirect]
  expect:
    reads: ["config/base.yml"]
    writes: ["build/out.env"]
```

- `command` (required) — the text a transcript would have recorded.
- `tags` (required) — which shapes this case pins, from a closed vocabulary.
- `cwd` (optional) — overrides the file header for this one case.
- `expect` (optional) — omit it entirely when the command should produce no
  access at all, which is the correct answer for an unattributed utility or a
  reader taking stdin.

`expect` is sparse: every key is dropped rather than written as a default. There
are no empty `reads`/`writes`/`deletes`/`directories`/`ambiguous` lists and no
`scripting: false`.

| Key | Meaning |
|---|---|
| `reads`, `writes`, `deletes` | literal paths that name a **file**, in the order the classifier emits them |
| `directories` | `{path, op}` for a literal path that names a **directory** |
| `ambiguous` | `{path, op, reason}` for a fragment the parse refused to resolve, whichever it targets |
| `scripting` | `true` when every access this command produces is inside an inline interpreter |
| `error` | `parse` when the command yields a classification error instead of accesses |

`cwd` is recorded call context, the same kind of fact a harness stores. The
classifier copies it onto each access and **never joins it onto a path**, so a
relative operand stays relative.

## Checks

```sh
uv run --script shell-access/corpus/verify_corpus.py     # CI: schema, coverage, denylist
uv run --script shell-access/corpus/check_not_copied.py  # local only, see below
```

`verify_corpus.py` fails on an unknown key or tag, a missing `command`/`tags`, a
duplicate id, a default-valued `expect` key, an `expect` path that does not
appear in its own command, a missing required tag, or any machine/user/project
token in a command or working directory.

`check_not_copied.py` compares corpus commands against local sessions and is
deliberately not a CI job: a build machine has no sessions, so there it would
pass without checking anything. Maintainers run it after editing the corpus.

Both scripts need PyYAML. Neither executes a command or reads project source.
