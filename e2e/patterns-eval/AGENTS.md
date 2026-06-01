# Patterns skill eval

This project's constitution. Both assemblies name this file explicitly at
position zero of the prefix so it is always the first thing the model
reads.

This project is a regression harness for the `patterns:*` plugin from
[davidsouther/domain-driven-design](https://github.com/davidsouther/domain-driven-design).
Two axes are exercised: **discovery** asks the model to pick the right
skill from its `description:` frontmatter alone, and **invocation** asks
the model to produce code that structurally exhibits the named pattern
once the skill is loaded.

## Reading the invocation comparison

The invocation axis is run as an A/B comparison: a **baseline** arm (no
skill) against an **invocation** arm (skill loaded), over identical
prompts. The falsification gate in `ci.sh` requires `improved > 0` and
`regressed == 0` — the skill must help on at least one assertion and harm
none.

Not every skill produces improvement, and that is a feature, not a bug:

- **emitting-logs** is the clear positive. Without the skill the model
  interpolates values into the message body and skips `eventName`; with
  it, the record is structured under semantic-convention keys. Both the
  script checker and the judge flip from fail to pass.
- **configuring-logging** improves on the judge: the skill produces a
  fuller five-layer pipeline than the baseline's partial attempt.
- **newtype** is a deliberate **null result**. A capable model already
  reaches for brand types when asked for swap-proof ids, with or without
  the skill, so both arms pass. newtype is retained precisely to show
  what a skill that does *not* change a capable model's output looks like
  in the report: `UnchangedPass`, contributing nothing to `improved`. The
  gate does not depend on it.

The lesson the harness encodes: a skill earns its place by changing
behaviour a baseline model would otherwise get wrong. The eval reports
both the skills that clear that bar and the ones that do not.
