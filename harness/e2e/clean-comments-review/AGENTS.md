# Comment review eval

This project's constitution. Every assembly names this file explicitly at
position zero of the prefix, so it is the first thing the model reads.

This project is a regression harness for a comment-review skill. Two axes are
exercised: **discovery** asks the model to pick the right skill from its
`description:` frontmatter alone, and **invocation** asks the model to review
the comments in a piece of code once the skill is loaded.

## Reading the invocation comparison

The invocation axis runs as an A/B comparison: a **baseline** arm (no skill)
against an **invocation** arm (skill loaded), over identical prompts. The
falsification gate in `ci.sh` requires `improved > 0` and `regressed == 0`: the
skill must help on at least one assertion and harm none.

Improvement must come from the skill body shaping the review. It must never come
from this shared prefix teaching the technique, nor from a lenient checker. This
constitution and the candidate-project file frame the task and the output format
only; how to review a comment well is the skill's job, and the skill is the only
thing the two arms do not share.
