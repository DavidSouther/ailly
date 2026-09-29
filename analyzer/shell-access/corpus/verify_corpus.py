#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml"]
# ///
"""Check the shell-access corpus: schema, shape coverage, and deidentification.

Run from anywhere; it reads only the YAML files beside it.

    uv run --script shell-access/corpus/verify_corpus.py

Exit 0 means every case is well-formed, every shape the classifier has to get
right is represented, ids are unique across files, and no machine, user, or
project token from a real transcript leaked into a command or a working
directory. Nothing here executes a command or looks at the filesystem outside
this directory.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

import yaml

CORPUS = Path(__file__).resolve().parent

#: Every tag any case may carry. A closed vocabulary so a typo is an error
#: rather than a silently uncovered shape.
KNOWN_TAGS = {
    "ambiguous",
    "command-substitution",
    "compound",
    "copy",
    "cwd-override",
    "delete",
    "directory",
    "expansion",
    "fd-merge",
    "glob",
    "heredoc",
    "in-place",
    "issue-233",
    "parse-error",
    "pipeline",
    "read",
    "redirect",
    "script-first",
    "scripting",
    "stdin",
    "tee",
    "unattributed",
    "value-flag",
    "wrapper",
    "write",
}

#: Tags the corpus must keep at least one case for, and the regression each
#: one pins. These are the shapes a classifier gets wrong by default.
REQUIRED_TAGS = {
    "read": "a reader utility's operands",
    "redirect": "input and output redirections",
    "fd-merge": "`2>&1` must not invent a path named 2",
    "script-first": "`sed -n '1,220p' file` must not read the script",
    "tee": "tee writes its operands",
    "copy": "cp/mv read all but the last operand and write the last",
    "in-place": "an in-place flag turns a read operand into a write",
    "delete": "rm/rmdir delete rather than write",
    "directory": "ls, find, du, tree, mkdir, and rmdir name a directory",
    "wrapper": "env, sudo, and bash -lc carry an inner command",
    "pipeline": "each pipeline stage is classified on its own",
    "glob": "an unexpanded glob is ambiguous, not a path",
    "expansion": "a parameter expansion is ambiguous, not a path",
    "command-substitution": "a command substitution is ambiguous, not a path",
    "heredoc": "an unquoted heredoc body is expanded before the utility sees it",
    "scripting": "python3 -c / perl -e are scripting, their redirects are not",
    "issue-233": "words after a redirect destination are not more destinations",
    "unattributed": "a utility outside the table invents no file row",
    "parse-error": "unparsable text is reported as unparsed",
    "cwd-override": "a case may record a different directory than its file",
}

ALLOWED_EXPECT_KEYS = {
    "reads",
    "writes",
    "deletes",
    "directories",
    "ambiguous",
    "scripting",
    "error",
}
PATH_LIST_KEYS = ("reads", "writes", "deletes")
OPERATIONS = {"read", "write", "delete"}
AMBIGUITY_REASONS = {"glob", "expansion", "command_substitution", "expanded_heredoc"}
ERRORS = {"parse", "unsupported_language"}

#: Tokens that would mean a real session leaked into the corpus. The corpus is
#: written on synthetic paths, so any of these is a copy rather than an example.
DENIED_SUBSTRINGS = (
    "/Users/",
    "/home/",
    "~/",
    "$HOME",
    ".ssh",
    ".claude",
    ".codex",
    ".pi/",
    ".ailly",
    "david.souther",
    "davidsouther",
    "ailly",
)

DENIED_PATTERNS = (
    # An address, and anything shaped like user@host.
    re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+"),
    # A UUID, which is how every harness names a session.
    re.compile(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}"),
)


def load_documents(path: Path) -> tuple[dict, dict, list[str]]:
    """Split one corpus file into its optional header and its case map."""
    documents = [
        document for document in yaml.safe_load_all(path.read_text(encoding="utf-8"))
    ]
    if len(documents) == 1:
        header, cases = {}, documents[0]
    elif len(documents) == 2:
        header, cases = documents[0], documents[1]
    else:
        return {}, {}, [f"{path.name}: expected one or two YAML documents"]

    problems = []
    if header is None:
        header = {}
    if not isinstance(header, dict):
        problems.append(f"{path.name}: the header document must be a mapping")
        header = {}
    if not isinstance(cases, dict):
        problems.append(f"{path.name}: the case document must be a mapping of id to case")
        cases = {}
    unknown_header = set(header) - {"cwd"}
    if unknown_header:
        problems.append(f"{path.name}: header keys not understood: {sorted(unknown_header)}")
    return header, cases, problems


def denied_tokens(text: str) -> list[str]:
    found = [token for token in DENIED_SUBSTRINGS if token in text]
    found += [
        match.group(0) for pattern in DENIED_PATTERNS for match in pattern.finditer(text)
    ]
    return found


def check_expect(case_id: str, command: str, expect: object) -> list[str]:
    if not isinstance(expect, dict):
        return [f"{case_id}: expect must be a mapping"]

    problems = []
    unknown = set(expect) - ALLOWED_EXPECT_KEYS
    if unknown:
        problems.append(f"{case_id}: expect keys not understood: {sorted(unknown)}")

    for key in PATH_LIST_KEYS:
        if key not in expect:
            continue
        paths = expect[key]
        if not isinstance(paths, list) or not all(isinstance(p, str) for p in paths):
            problems.append(f"{case_id}: expect.{key} must be a list of paths")
            continue
        if not paths:
            problems.append(f"{case_id}: drop expect.{key} rather than writing an empty list")
        problems += [
            f"{case_id}: expect.{key} path {path!r} does not appear in the command"
            for path in paths
            if path not in command
        ]

    if "ambiguous" in expect:
        fragments = expect["ambiguous"]
        if not isinstance(fragments, list):
            problems.append(f"{case_id}: expect.ambiguous must be a list")
        elif not fragments:
            problems.append(f"{case_id}: drop expect.ambiguous rather than writing an empty list")
        else:
            problems += check_ambiguous(case_id, command, fragments)

    if "directories" in expect:
        directories = expect["directories"]
        if not isinstance(directories, list):
            problems.append(f"{case_id}: expect.directories must be a list")
        elif not directories:
            problems.append(f"{case_id}: drop expect.directories rather than writing an empty list")
        else:
            problems += check_targets(case_id, command, "directories", directories)

    if "scripting" in expect and expect["scripting"] is not True:
        problems.append(f"{case_id}: expect.scripting is only ever true; drop it otherwise")

    if "error" in expect and expect["error"] not in ERRORS:
        problems.append(f"{case_id}: expect.error must be one of {sorted(ERRORS)}")

    if not expect:
        problems.append(f"{case_id}: drop expect entirely rather than writing an empty mapping")
    return problems


def check_ambiguous(case_id: str, command: str, fragments: list) -> list[str]:
    problems = []
    for fragment in fragments:
        if not isinstance(fragment, dict) or set(fragment) != {"path", "op", "reason"}:
            problems.append(
                f"{case_id}: each expect.ambiguous entry needs exactly path, op, and reason"
            )
            continue
        if fragment["path"] not in command:
            problems.append(
                f"{case_id}: ambiguous fragment {fragment['path']!r} does not appear in the command"
            )
        if fragment["op"] not in OPERATIONS:
            problems.append(f"{case_id}: ambiguous op must be one of {sorted(OPERATIONS)}")
        if fragment["reason"] not in AMBIGUITY_REASONS:
            problems.append(
                f"{case_id}: ambiguous reason must be one of {sorted(AMBIGUITY_REASONS)}"
            )
    return problems


def check_targets(
    case_id: str, command: str, target: str, accesses: list
) -> list[str]:
    problems = []
    for access in accesses:
        if not isinstance(access, dict) or set(access) != {"path", "op"}:
            problems.append(
                f"{case_id}: each expect.{target} entry needs exactly path and op"
            )
            continue
        if access["path"] not in command:
            problems.append(
                f"{case_id}: {target} path {access['path']!r} does not appear in the command"
            )
        if access["op"] not in OPERATIONS:
            problems.append(
                f"{case_id}: {target} op must be one of {sorted(OPERATIONS)}"
            )
    return problems


def check_case(case_id: str, case: object, header: dict) -> tuple[set[str], list[str]]:
    if not isinstance(case, dict):
        return set(), [f"{case_id}: a case must be a mapping"]

    problems = []
    unknown = set(case) - {"command", "tags", "cwd", "expect"}
    if unknown:
        problems.append(f"{case_id}: keys not understood: {sorted(unknown)}")

    command = case.get("command")
    if not isinstance(command, str) or not command.strip():
        return set(), problems + [f"{case_id}: command is required"]

    tags = case.get("tags")
    if not isinstance(tags, list) or not tags or not all(isinstance(t, str) for t in tags):
        return set(), problems + [f"{case_id}: tags is a required, non-empty list"]
    unknown_tags = set(tags) - KNOWN_TAGS
    if unknown_tags:
        problems.append(f"{case_id}: tags not in the vocabulary: {sorted(unknown_tags)}")

    for label, text in (("command", command), ("cwd", str(case.get("cwd", "")))):
        leaked = denied_tokens(text)
        if leaked:
            problems.append(f"{case_id}: {label} contains a real-session token: {leaked}")

    if "cwd-override" in tags and case.get("cwd", header.get("cwd")) == header.get("cwd"):
        problems.append(f"{case_id}: tagged cwd-override but records the file's own directory")

    if "expect" in case:
        problems += check_expect(case_id, command, case["expect"])

    return set(tags), problems


def main() -> int:
    files = sorted(CORPUS.glob("*.yaml"))
    if not files:
        print(f"no corpus files under {CORPUS}", file=sys.stderr)
        return 1

    problems: list[str] = []
    seen_ids: dict[str, str] = {}
    covered: set[str] = set()

    for path in files:
        header, cases, file_problems = load_documents(path)
        problems += file_problems
        leaked = denied_tokens(str(header.get("cwd", "")))
        if leaked:
            problems.append(f"{path.name}: header cwd contains a real-session token: {leaked}")
        for case_id, case in cases.items():
            if case_id in seen_ids:
                problems.append(
                    f"{path.name}: id {case_id!r} already used in {seen_ids[case_id]}"
                )
                continue
            seen_ids[case_id] = path.name
            tags, case_problems = check_case(case_id, case, header)
            covered |= tags
            problems += case_problems

    for tag, pins in sorted(REQUIRED_TAGS.items()):
        if tag not in covered:
            problems.append(f"no case tagged {tag!r}, which pins: {pins}")

    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        print(f"\n{len(problems)} problem(s) in {len(files)} file(s)", file=sys.stderr)
        return 1
    print(f"{len(seen_ids)} cases in {len(files)} files: schema, coverage, and denylist ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
