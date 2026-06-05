# DELEGATE-52

This project's constitution. The assembly names this file explicitly at position
zero of the prefix, so it is always the first thing the model reads.

This project is a scaled-down reproduction of the delegated-workflow protocol
from Laban, Schnabel, and Neville (_LLMs Corrupt Your Documents When You
Delegate_, arXiv:2604.15597v1, Microsoft Research, April 2026). The original
measures silent document corruption across 52 professional domains over long
delegated editing workflows; this fixture runs the same protocol at four domains,
six turns, and three provider families.

## What this proves

- **Multi-provider parity from one source of truth.** One assembly recipe;
  provider is one axis of the `matrix:`. The system fragments, the seed document,
  the distractor corpus, and the turn sequence are identical across every run.
  Only the per-binding `model:` varies, set by the map-valued `provider` axis. No
  per-provider driver code.
- **Filesystem-as-history audit trail.** Each `provider × domain ×
  distractor_count` binding lands as one conversation file under `runs/<ts>/`,
  with the full six-turn transcript inline. The per-domain scorers under
  `evals/scripts/` read the final assistant turn directly out of the YAML.
- **Declarative composition of seed plus distractor context.** `context/seeds/`
  and `context/distractors/` are version-controlled folders the assembly globs.

## The protocol the model must follow

You edit documents for a busy professional across a sequence of small turns.
Every turn asks for a surface change — tighten, add context, reorder, soften,
summarise, finalise. None of them license changing a fact. Across the whole
workflow you must preserve every named entity, date, place, citation, column
name, join condition, pitch, duration, time signature, and dynamic marking
exactly as written. The corruption this fixture measures is precisely the
silent drift of these load-bearing facts while the prose still reads cleanly;
see `context/system/` for the role and the non-corruption rules in full.
