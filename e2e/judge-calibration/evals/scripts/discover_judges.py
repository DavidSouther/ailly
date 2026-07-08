#!/usr/bin/env python3
"""Discover every real `judge` assertion in the repo's e2e eval suites.

Step 1 of the judge-calibration relevance-matrix pipeline (see
`.ailly/developer/.../feature-e-judge-calibration/` design notes and the
sibling `mine_calibration_candidates.py` docstring for the rest of the
pipeline). Where step 2 mines *candidates* out of the operator's own session
history, this script mines *judges* out of the actual `ailly_two` eval
suites: every `{type: judge, prompt: ...}` assertion under `e2e/*/evals/*.yaml`
(excluding this suite's own `e2e/judge-calibration/`, which has none).

## What "topic keywords" means and where they come from

A judge's `prompt:` is prose a human wrote to grade a specific skill's or
pattern's use — but prose is hard to match against automatically. Each
judge's *sibling* assertions in the same eval case are a much more exact
signal: a `text_contains`/`text_not_contains` naming an exact token like
`patterns:configuring-logging` tells us unambiguously what that case is
about. Keywords are derived in this priority order, most-exact first, and
every keyword records which tier produced it:

1. **sibling** — a `patterns:`/`developer:`/`domain:`/`research:`/`general:`
   token found in a `text_contains` (this case's `include` list) or
   `text_not_contains` (its `exclude` list — the skill the answer must NOT
   show; still recorded, since it's still evidence of the case's topic, just
   with the opposite polarity) value in the SAME case as the judge.
2. **prompt** — the same token shape, found directly in the judge's own
   prompt text, when no sibling supplied one.
3. **assembly** — cross-referencing this eval suite's sibling
   `assemblies/<same-stem>.yaml` file. If the case's name matches one value
   of the assembly's `matrix:` axis that is used to select a skill's
   `SKILL.md` into the prompt (a `prefix:`/`conversation:` block templated
   as `.../skills/{{ <axis> }}/SKILL.md` or a literal `.../skills/<value>/
   SKILL.md`), that IS the skill this case exercises — full-stop, no
   guessing. If some OTHER file in the same suite's `evals/` directory
   already writes an explicit `<plugin>:<name>` token (patterns-eval's
   `discovery.yaml` does, for example), that suite's dominant plugin prefix
   is applied to synthesize `<plugin>:<case-name>`. If the assembly instead
   loads an external, unprefixed repo skill (`kind: external, path: ../../
   skills/<name>/SKILL.md`, as clean-comments-review does) and the suite
   never spells out a plugin prefix anywhere, the BARE short name is
   recorded instead (still useful: `skill_signals.tags_match` treats a bare
   name as matching a candidate's fully-qualified `plugin:name` tag by short
   form).
4. **none** — no signal survives any of the above (expected for
   domain-specific judges like delegate-52's cross-provider corruption
   check or insurance-claim's business-rule regression check, which are
   about a scenario, not a skill). These are written out with an empty
   keyword list and `needs_human_review: true` rather than forced to fit.

## Usage

    uv run --with pyyaml python3 discover_judges.py [--e2e-dir PATH] [--output PATH]

Requires PyYAML (not a base-Python module); `uv run --with pyyaml` fetches it
into an ephemeral environment without touching the system interpreter — see
this repo's other Python eval scripts for the same convention. Defaults:
`--e2e-dir` is `e2e/` at the repo root inferred from this file's location;
`--output` is `e2e/judge-calibration/evals/judges.yaml` (checked in — this is
derived-but-stable reference data, unlike the gitignored `mined/` tree).

Re-running overwrites the output deterministically from the current state of
the eval suites; there is nothing to merge.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any, Optional

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
from skill_signals import PLUGIN_PREFIXES, SKILL_TOKEN_RE  # noqa: E402


def repo_root_from_here() -> Path:
    # scripts/ -> evals/ -> judge-calibration/ -> e2e/ -> repo root
    return Path(__file__).resolve().parents[4]


def find_eval_yaml_files(e2e_dir: Path) -> list[Path]:
    files: list[Path] = []
    for suite_dir in sorted(p for p in e2e_dir.iterdir() if p.is_dir()):
        if suite_dir.name == "judge-calibration":
            continue  # this suite's own evals/ has no judge assertions (yet)
        evals_dir = suite_dir / "evals"
        if not evals_dir.is_dir():
            continue
        files.extend(sorted(evals_dir.glob("*.yaml")))
    return files


def load_yaml_documents(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def case_identifier(case: dict[str, Any], index: int) -> str:
    if case.get("name"):
        return str(case["name"])
    when = case.get("when")
    if isinstance(when, dict) and when:
        return "when-" + "-".join(f"{k}-{v}" for k, v in sorted(when.items()))
    return f"case{index}"


def tokens_in(value: Any) -> list[str]:
    if not isinstance(value, str):
        return []
    return SKILL_TOKEN_RE.findall(value)


# --------------------------------------------------------------------------
# Tier 3: assembly cross-reference + suite-wide dominant-plugin convention
# --------------------------------------------------------------------------


def suite_dominant_plugin(evals_dir: Path, yaml_cache: dict[Path, Any]) -> Optional[str]:
    """The single plugin prefix (if unambiguous) used anywhere in this suite's
    `evals/*.yaml`, e.g. `patterns` for patterns-eval (its discovery.yaml
    writes literal `patterns:configuring-logging` tokens). Returns None if no
    token appears anywhere, or if more than one distinct prefix appears
    (ambiguous — refuse to guess)."""
    prefixes: set[str] = set()
    for f in sorted(evals_dir.glob("*.yaml")):
        doc = yaml_cache.setdefault(f, load_yaml_documents(f))
        for case in (doc or {}).get("cases", []) or []:
            for assertion in case.get("assertions", []) or []:
                for field in ("value", "prompt"):
                    for tok in tokens_in(assertion.get(field)):
                        prefixes.add(tok.split(":", 1)[0])
    return next(iter(prefixes)) if len(prefixes) == 1 else None


def load_sibling_assembly(suite_dir: Path, stem: str) -> Optional[dict[str, Any]]:
    assembly_path = suite_dir / "assemblies" / f"{stem}.yaml"
    if not assembly_path.is_file():
        return None
    try:
        return load_yaml_documents(assembly_path)
    except yaml.YAMLError:
        return None


def assembly_skill_hint(assembly: dict[str, Any], case_name: str) -> Optional[str]:
    """If `case_name` is one value of a `matrix:` axis, and that axis feeds a
    `.../skills/{{ axis }}/SKILL.md`-shaped path (internal, plugin-relative)
    or a `.../skills/<value>/SKILL.md` external path (bare repo skill), return
    the bare skill short-name (== case_name) confirming this case loaded that
    skill's body into its prompt. Returns None if no axis matches or no block
    templates a skills/ path at all — i.e. this assembly's matrix is about
    something else entirely (e.g. delegate-52's `domain:` axis, which never
    touches a `skills/` path)."""
    matrix = assembly.get("matrix") or {}
    axis_name = None
    for name, values in matrix.items():
        if isinstance(values, list) and case_name in values:
            axis_name = name
            break
    if axis_name is None:
        return None

    blocks = list(assembly.get("prefix") or []) + list(assembly.get("conversation") or [])
    template_token = f"{{{{ {axis_name} }}}}"  # Jinja-ish "{{ skill }}" as literally written in the yaml
    for block in blocks:
        path = block.get("path") if isinstance(block, dict) else None
        if not isinstance(path, str) or "skills/" not in path or "SKILL.md" not in path:
            continue
        if template_token in path:
            # e.g. "context/skills/{{ skill }}/SKILL.md" — templated per matrix binding
            return case_name
        if f"skills/{case_name}/SKILL.md" in path:
            # e.g. "../../skills/clean-comments-review/SKILL.md" — this exact case's value, literal
            return case_name
    return None


def derive_keywords(
    suite_dir: Path,
    evals_dir: Path,
    stem: str,
    case: dict[str, Any],
    case_name: str,
    prompt: str,
    yaml_cache: dict[Path, Any],
) -> list[dict[str, str]]:
    keywords: list[dict[str, str]] = []

    # Tier 1: siblings in the same case.
    for assertion in case.get("assertions", []) or []:
        atype = assertion.get("type")
        if atype == "text_contains":
            for tok in tokens_in(assertion.get("value")):
                keywords.append({"value": tok, "polarity": "include", "source": "sibling"})
        elif atype == "text_not_contains":
            for tok in tokens_in(assertion.get("value")):
                keywords.append({"value": tok, "polarity": "exclude", "source": "sibling"})
    if keywords:
        return keywords

    # Tier 2: the judge prompt's own text.
    for tok in tokens_in(prompt):
        keywords.append({"value": tok, "polarity": "include", "source": "prompt"})
    if keywords:
        return keywords

    # Tier 3: assembly cross-reference + suite convention.
    assembly = load_sibling_assembly(suite_dir, stem)
    if assembly is not None:
        bare = assembly_skill_hint(assembly, case_name)
        if bare is not None:
            dominant = suite_dominant_plugin(evals_dir, yaml_cache)
            if dominant is not None:
                keywords.append(
                    {
                        "value": f"{dominant}:{bare}",
                        "polarity": "include",
                        "source": "assembly+suite-convention",
                    }
                )
            else:
                keywords.append(
                    {"value": bare, "polarity": "include", "source": "assembly-external-skill-path"}
                )
    return keywords


def discover(e2e_dir: Path, repo_root: Path) -> list[dict[str, Any]]:
    judges: list[dict[str, Any]] = []
    yaml_cache: dict[Path, Any] = {}

    for yaml_path in find_eval_yaml_files(e2e_dir):
        suite_dir = yaml_path.parent.parent  # evals/ -> suite dir
        stem = yaml_path.stem
        doc = yaml_cache.setdefault(yaml_path, load_yaml_documents(yaml_path))
        if not doc or "cases" not in doc:
            continue

        for idx, case in enumerate(doc["cases"]):
            case_name = case_identifier(case, idx)
            for assertion in case.get("assertions", []) or []:
                if assertion.get("type") != "judge":
                    continue
                prompt = assertion.get("prompt", "")
                judge_id = f"{suite_dir.name}/{stem}/{case_name}"
                keywords = derive_keywords(
                    suite_dir, yaml_path.parent, stem, case, case_name, prompt, yaml_cache
                )
                judges.append(
                    {
                        "judge_id": judge_id,
                        "suite": suite_dir.name,
                        "suite_file": str(yaml_path.relative_to(repo_root)),
                        "case_name": case.get("name"),
                        "case_when": case.get("when"),
                        "prompt": prompt,
                        "keywords": keywords,
                        "needs_human_review": not keywords,
                    }
                )

    _fill_gaps_from_sibling_suite_cases(judges)
    return judges


def _fill_gaps_from_sibling_suite_cases(judges: list[dict[str, Any]]) -> None:
    """Tier 3b, applied after every judge in the run has an initial pass: a
    "baseline" arm's eval file often re-runs the SAME rubric against the SAME
    case name as an "invocation" arm's file in the same suite, specifically
    to compare with/without the skill loaded (see e.g. patterns-eval's
    `baseline.yaml` vs `invocation.yaml`, whose `newtype`/`configuring-
    logging`/`emitting-logs` cases share prompts verbatim). The baseline
    arm's assembly never references a `skills/` path by design — that's the
    point of a baseline — so tier 3 (assembly cross-reference) correctly
    finds nothing there. But for calibration purposes the topic is identical:
    a human validating this judge still wants real conversations that
    exercised that skill. Reuse a sibling case's resolved keywords by exact
    `(suite, case_name)` match, so this only ever fires when another file in
    the very same suite already established the topic through tiers 1-3."""
    by_case: dict[tuple[str, str], list[dict[str, str]]] = {}
    for j in judges:
        if j["keywords"] and j["case_name"]:
            by_case.setdefault((j["suite"], j["case_name"]), j["keywords"])

    for j in judges:
        if j["keywords"] or not j["case_name"]:
            continue
        found = by_case.get((j["suite"], j["case_name"]))
        if found:
            j["keywords"] = [
                {**kw, "source": f"sibling-suite-case-name({kw['source']})"} for kw in found
            ]
            j["needs_human_review"] = False


def write_judges_yaml(judges: list[dict[str, Any]], output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    header = (
        "# Judge registry for judge-calibration (step 1 of the relevance-matrix\n"
        "# pipeline). Generated by discover_judges.py from every `type: judge`\n"
        "# assertion under e2e/*/evals/*.yaml — re-run that script to refresh\n"
        "# after an eval suite changes; do not hand-edit `keywords` (edit the\n"
        "# source eval suite's assertions instead, so this stays derived).\n"
        "#\n"
        "# Schema (one entry per judge assertion found):\n"
        "#   judge_id: <suite>/<eval-stem>/<case-name>            # stable, unique\n"
        "#   suite: <e2e/ subdirectory name>\n"
        "#   suite_file: <path to the .yaml the judge assertion lives in>\n"
        "#   case_name: <Case.name, or null for an unnamed/`when`-only case>\n"
        "#   case_when: <Case.when map, or null>\n"
        "#   prompt: <the judge assertion's full rubric text>\n"
        "#   keywords: [{value, polarity: include|exclude, source}]\n"
        "#     source is one of: sibling (exact, from a text_contains/\n"
        "#     text_not_contains in the same case), prompt (exact, regex over\n"
        "#     the judge's own prompt text), assembly+suite-convention or\n"
        "#     assembly-external-skill-path (inferred from the case's assembly\n"
        "#     + this suite's own token conventions — see discover_judges.py's\n"
        "#     module docstring), or absent entirely when keywords is [].\n"
        "#   needs_human_review: true when keywords is empty — no automated\n"
        "#     candidate matching is possible for this judge; a human should\n"
        "#     browse the mined candidates for it manually.\n"
    )
    with output.open("w", encoding="utf-8") as fh:
        fh.write(header)
        fh.write("\n")
        yaml.safe_dump(
            {"judges": judges}, fh, sort_keys=False, allow_unicode=True, width=100
        )


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--e2e-dir", type=Path, default=None)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args(argv)

    repo_root = repo_root_from_here()
    e2e_dir = args.e2e_dir or (repo_root / "e2e")
    output = args.output or (
        Path(__file__).resolve().parents[1] / "judges.yaml"
    )  # scripts/ -> evals/judges.yaml

    judges = discover(e2e_dir, repo_root)
    write_judges_yaml(judges, output)

    with_keywords = sum(1 for j in judges if j["keywords"])
    needs_review = [j["judge_id"] for j in judges if j["needs_human_review"]]

    print(f"[discover_judges] {len(judges)} judge assertions found under {e2e_dir}")
    print(f"[discover_judges] {with_keywords}/{len(judges)} resolved >=1 topic keyword")
    if needs_review:
        print(f"[discover_judges] {len(needs_review)} need human review (empty keywords):")
        for jid in needs_review:
            print(f"  - {jid}")
    print(f"[discover_judges] wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
