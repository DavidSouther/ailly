#!/usr/bin/env python3
"""Structural checker for `patterns:newtype` invocation cases.

Reads a TypeScript candidate from stdin and applies an ordered list of rules,
each tracing 1:1 to a bullet in the newtype SKILL.md "Common Mistakes" section.
On the first violated rule it prints a single-line reason to stdout and exits 1;
if every rule holds it exits 0. stderr is left untouched so the eval runner
records a genuine Fail, never an `Errored` broken checker.

The rules validate structure, not a single surface dialect: a brand may be an
`&`-intersection (`string & { __brand }`, `string & { [sym]: never }`) or a
brand-helper alias (`Branded<string, "X">`); a constructor may be a `function`
declaration or an arrow `const`. The transparent-alias mistake (`type X = string`
with no brand) is what fails.

Rules:
- R1 brand required ("Plain type alias instead of a brand").
- R2 no `as` casts at call sites ("Casting with `as` at call sites").
- R3 named constructor required ("Skipping the constructor or builder").
"""

import re
import sys

from _checker_utils import extract_code, fail, strip_comments

BRAND_HELPER = re.compile(r"\b(?:Brand|Branded|Opaque|Nominal|Flavor|Tagged)\b\s*<")


def brand_names(src: str) -> list[str]:
    """Names of branded type aliases.

    A `type X = RHS;` is branded when its RHS is an intersection (`&`) or
    references a brand helper. A bare-primitive alias (`type X = string;`) is
    not — that is the transparent-alias mistake.
    """
    names = []
    for match in re.finditer(r"\btype\s+(\w+)\s*=\s*([^;]+);", src, re.DOTALL):
        name, rhs = match.group(1), match.group(2)
        if "&" in rhs or BRAND_HELPER.search(rhs):
            names.append(name)
    return names


def constructor_body_ranges(src: str, brands: list[str]) -> list[tuple[int, int]]:
    """Char ranges of bodies of functions whose return type is a brand.

    Recognizes three constructor forms: `function name(...): Brand { ... }`, a
    braced arrow `const name = (...): Brand => { ... }`, and an expression arrow
    `const name = (...): Brand => raw as Brand;`. For braced forms the body is
    the matching brace pair; for an expression arrow the body runs to the
    terminating `;`. The single sanctioned `as` cast lives in any of these.
    """
    ranges = []
    alt = "|".join(brands)
    braced = re.compile(
        r"\bfunction\s+\w+\s*\([^)]*\)\s*:\s*(?:" + alt + r")\b[^{;]*\{"
        r"|\b(?:const|let)\s+\w+\s*=\s*\([^)]*\)\s*:\s*(?:" + alt + r")\b[^{;=]*=>\s*\{"
    )
    for match in braced.finditer(src):
        open_brace = src.index("{", match.end() - 1)
        depth = 0
        for i in range(open_brace, len(src)):
            if src[i] == "{":
                depth += 1
            elif src[i] == "}":
                depth -= 1
                if depth == 0:
                    ranges.append((open_brace, i))
                    break

    expr_arrow = re.compile(
        r"\b(?:const|let)\s+\w+\s*=\s*\([^)]*\)\s*:\s*(?:" + alt + r")\b\s*=>\s*(?!\{)"
    )
    for match in expr_arrow.finditer(src):
        end = src.find(";", match.end())
        ranges.append((match.start(), end if end != -1 else len(src)))
    return ranges


def has_named_constructor(src: str, brands: list[str]) -> bool:
    """A named function (declaration or arrow const) returning a brand."""
    alt = "|".join(brands)
    decl = re.compile(r"\bfunction\s+\w+\s*\([^)]*\)\s*:\s*(?:" + alt + r")\b")
    arrow = re.compile(r"\b(?:const|let)\s+\w+\s*=\s*\([^)]*\)\s*:\s*(?:" + alt + r")\b")
    return bool(decl.search(src) or arrow.search(src))


def main() -> int:
    src = strip_comments(extract_code(sys.stdin.read()))

    # R1 — brand required.
    brands = brand_names(src)
    if not brands:
        return fail(
            "R1 brand required: no branded type (an `&` intersection or a brand "
            "helper); a plain `type X = <primitive>` alias is transparent to the "
            "compiler and provides no safety"
        )

    # R2 — no `as` casts at call sites.
    sanctioned = constructor_body_ranges(src, brands)
    cast = re.compile(r"\bas\s+(" + "|".join(brands) + r")\b")
    for match in cast.finditer(src):
        pos = match.start()
        if not any(start < pos < end for start, end in sanctioned):
            return fail(
                f"R2 no `as` casts at call sites: `as {match.group(1)}` outside the "
                "constructor; the only sanctioned cast lives in the constructor body"
            )

    # R3 — named constructor required.
    if not has_named_constructor(src, brands):
        return fail(
            "R3 named constructor required: no named function returns a branded type; "
            "centralise construction so validation happens once"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
