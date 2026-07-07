import json
import tempfile
import textwrap
import unittest
from pathlib import Path

from backend import precheck
from backend.conversation_draft import parse_conversation_draft

DISCOVERY_SUITE = textwrap.dedent(
    """\
    name: discovery
    cases:
      - name: paired-add-propagator
        assertions:
          - { type: text_contains, value: "patterns:configuring-logging" }
          - { type: text_not_contains, value: "patterns:emitting-logs" }
          - type: judge
            prompt: |
              some rubric text
    """
)

SCALAR_DRAFT = (
    '---\nmodel: "claude-sonnet-4-6"\n'
    "---\nrole: user\ncontent: |\n"
    "  Which pattern applies?\n"
    "---\nrole: assistant\ncontent: |\n"
    "  Use patterns:configuring-logging because bootstrap runs once.\n"
)

FAILING_SCALAR_DRAFT = (
    '---\nmodel: "claude-sonnet-4-6"\n'
    "---\nrole: user\ncontent: |\n"
    "  Which pattern applies?\n"
    "---\nrole: assistant\ncontent: |\n"
    "  Use patterns:emitting-logs for this call site.\n"
)

BLOCKS_DRAFT = (
    '---\nmodel: "claude-opus-4-8"\n'
    "---\nrole: user\ncontent: |\n"
    "  Add a UserId newtype.\n"
    "---\nrole: assistant\ncontent:\n"
    "  - type: text\n"
    "    text: |\n"
    "      Here is the branded type.\n"
    "  - type: tool_use\n"
    '    id: "toolu_01"\n'
    '    name: "Write"\n'
    "    input:\n"
    '      file_path: "/repo/src/user_id.ts"\n'
    "      content: |\n"
    "        export type UserId = string & { readonly __brand: unique symbol };\n"
    "        export function makeUserId(raw: string): UserId {\n"
    "          return raw as UserId;\n"
    "        }\n"
)

NEWTYPE_INVOCATION_SUITE = textwrap.dedent(
    """\
    name: invocation
    cases:
      - name: newtype
        assertions:
          - { type: script, runtime: python, script: { path: evals/scripts/check_newtype.py } }
          - type: judge
            prompt: |
              some rubric text
          - { type: tokens, metric: total, op: "<", value: 6000 }
    """
)

# A tiny real checker mirroring the shape/contract of the real
# e2e/patterns-eval/evals/scripts checkers closely enough to exercise
# precheck's subprocess plumbing without depending on the real suite.
FAKE_CHECKER = textwrap.dedent(
    """\
    import re
    import sys

    FENCE = re.compile(r"```[^\\n]*\\n(.*?)```", re.DOTALL)


    def extract_code(src: str) -> str:
        blocks = FENCE.findall(src)
        return "\\n".join(blocks) if blocks else src


    def main() -> int:
        src = extract_code(sys.stdin.read())
        if "unique symbol" not in src:
            sys.stdout.write("R1 brand required: no branded type\\n")
            return 1
        return 0


    if __name__ == "__main__":
        raise SystemExit(main())
    """
)


def _write_fake_project(tmp: Path) -> Path:
    """Lay out a minimal <tmp>/e2e/patterns-eval/{evals/{invocation.yaml,
    scripts/check_newtype.py}} tree, mirroring the real suite's shape."""
    project = tmp / "e2e" / "patterns-eval"
    (project / "evals" / "scripts").mkdir(parents=True)
    (project / "evals" / "invocation.yaml").write_text(NEWTYPE_INVOCATION_SUITE, encoding="utf-8")
    (project / "evals" / "scripts" / "check_newtype.py").write_text(FAKE_CHECKER, encoding="utf-8")
    return tmp


class TestTextAssertions(unittest.TestCase):
    def test_text_contains_case_sensitive_default(self):
        outcome, reason = precheck._check_text_contains(
            "Use PATTERNS:NEWTYPE here", {"type": "text_contains", "value": "patterns:newtype"}
        )
        self.assertEqual(outcome, "fail")
        self.assertIn("not found", reason)

    def test_text_contains_case_insensitive_opt_in(self):
        outcome, reason = precheck._check_text_contains(
            "Use PATTERNS:NEWTYPE here",
            {"type": "text_contains", "value": "patterns:newtype", "case_sensitive": False},
        )
        self.assertEqual(outcome, "pass")
        self.assertIsNone(reason)

    def test_text_not_contains_reports_byte_offset(self):
        outcome, reason = precheck._check_text_not_contains(
            "abc patterns:newtype xyz", {"type": "text_not_contains", "value": "patterns:newtype"}
        )
        self.assertEqual(outcome, "fail")
        self.assertIn("byte offset 4", reason)

    def test_text_matches_with_flags(self):
        outcome, _ = precheck._check_text_matches(
            "USE NEWTYPE", {"type": "text_matches", "pattern": "newtype", "flags": "i"}
        )
        self.assertEqual(outcome, "pass")

    def test_text_matches_malformed_regex(self):
        outcome, reason = precheck._check_text_matches(
            "anything", {"type": "text_matches", "pattern": "("}
        )
        self.assertEqual(outcome, "malformed")
        self.assertIn("regex compile error", reason)

    def test_text_equals_exact_match_only(self):
        self.assertEqual(precheck._check_text_equals("abc", {"value": "abc"})[0], "pass")
        self.assertEqual(precheck._check_text_equals("abc", {"value": "abcd"})[0], "fail")


class TestProjectRootForSuiteFile(unittest.TestCase):
    def test_two_levels_up_from_evals_file(self):
        repo_root = Path("/repo")
        resolved = precheck.project_root_for_suite_file(repo_root, "e2e/patterns-eval/evals/invocation.yaml")
        self.assertEqual(resolved, Path("/repo/e2e/patterns-eval").resolve())


class TestPrecheckCellTextAssertions(unittest.TestCase):
    def _judge(self, tmp: Path) -> dict:
        suite_file_rel = "e2e/patterns-eval/evals/discovery.yaml"
        (tmp / "e2e" / "patterns-eval" / "evals").mkdir(parents=True)
        (tmp / suite_file_rel).write_text(DISCOVERY_SUITE, encoding="utf-8")
        return {
            "judge_id": "patterns-eval/discovery/paired-add-propagator",
            "suite_file": suite_file_rel,
            "case_name": "paired-add-propagator",
        }

    def test_all_pass_when_text_matches(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            judge = self._judge(tmp)
            conversation = parse_conversation_draft(SCALAR_DRAFT)
            result = precheck.precheck_cell(tmp, judge, "cand-1", conversation)
            self.assertEqual(result["overall"], "all_pass")
            self.assertEqual(result["content_shape"], "scalar")
            types = [c["type"] for c in result["checks"]]
            self.assertEqual(types, ["text_contains", "text_not_contains", "judge"])
            self.assertEqual(result["checks"][-1]["outcome"], "skipped")

    def test_some_fail_when_text_does_not_match(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            judge = self._judge(tmp)
            conversation = parse_conversation_draft(FAILING_SCALAR_DRAFT)
            result = precheck.precheck_cell(tmp, judge, "cand-2", conversation)
            self.assertEqual(result["overall"], "some_fail")
            outcomes = {c["type"]: c["outcome"] for c in result["checks"]}
            self.assertEqual(outcomes["text_contains"], "fail")
            self.assertEqual(outcomes["text_not_contains"], "fail")


class TestPrecheckCellScriptAssertion(unittest.TestCase):
    def _judge(self) -> dict:
        return {
            "judge_id": "patterns-eval/invocation/newtype",
            "suite_file": "e2e/patterns-eval/evals/invocation.yaml",
            "case_name": "newtype",
        }

    def test_script_assertion_runs_real_subprocess_and_passes(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = _write_fake_project(Path(d))
            conversation = parse_conversation_draft(
                '---\nmodel: "x"\n---\nrole: user\ncontent: |\n  q\n---\nrole: assistant\ncontent: |\n'
                "  ```typescript\n"
                "  export type UserId = string & { readonly __brand: unique symbol };\n"
                "  ```\n"
            )
            result = precheck.precheck_cell(tmp, self._judge(), "cand-3", conversation)
            script_check = next(c for c in result["checks"] if c["type"] == "script")
            self.assertEqual(script_check["outcome"], "pass")
            self.assertEqual(result["content_shape"], "scalar")
            self.assertFalse(result["flattening_applied_for_script_checks"])

    def test_script_assertion_fails_with_checker_stdout_as_reason(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = _write_fake_project(Path(d))
            conversation = parse_conversation_draft(
                '---\nmodel: "x"\n---\nrole: user\ncontent: |\n  q\n---\nrole: assistant\ncontent: |\n'
                "  ```typescript\n"
                "  export type UserId = string;\n"
                "  ```\n"
            )
            result = precheck.precheck_cell(tmp, self._judge(), "cand-4", conversation)
            script_check = next(c for c in result["checks"] if c["type"] == "script")
            self.assertEqual(script_check["outcome"], "fail")
            self.assertIn("R1 brand required", script_check["reason"])

    def test_script_assertion_against_blocks_content_uses_flattened_tool_use_code(self):
        # The narration alone ("Here is the branded type.") contains no
        # branded type; only the Write tool_use's real file content does.
        # This pins the flattening wrinkle: without it, this would (wrongly)
        # fail R1 even though the candidate really did write a valid brand.
        with tempfile.TemporaryDirectory() as d:
            tmp = _write_fake_project(Path(d))
            conversation = parse_conversation_draft(BLOCKS_DRAFT)
            result = precheck.precheck_cell(tmp, self._judge(), "cand-5", conversation)
            script_check = next(c for c in result["checks"] if c["type"] == "script")
            self.assertEqual(script_check["outcome"], "pass")
            self.assertEqual(result["content_shape"], "blocks")
            self.assertTrue(result["flattening_applied_for_script_checks"])

    def test_script_assertion_missing_checker_file_errors_not_fails(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = _write_fake_project(Path(d))
            (tmp / "e2e" / "patterns-eval" / "evals" / "scripts" / "check_newtype.py").unlink()
            conversation = parse_conversation_draft(
                '---\nmodel: "x"\n---\nrole: user\ncontent: |\n  q\n---\nrole: assistant\ncontent: |\n  no code here\n'
            )
            result = precheck.precheck_cell(tmp, self._judge(), "cand-6", conversation)
            script_check = next(c for c in result["checks"] if c["type"] == "script")
            self.assertEqual(script_check["outcome"], "errored")

    def test_overall_no_checks_when_only_skipped_assertions_present(self):
        judge_only_suite = textwrap.dedent(
            """\
            name: invocation
            cases:
              - name: solo-judge
                assertions:
                  - type: judge
                    prompt: |
                      rubric
            """
        )
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "e2e" / "patterns-eval" / "evals").mkdir(parents=True)
            (tmp / "e2e" / "patterns-eval" / "evals" / "invocation.yaml").write_text(
                judge_only_suite, encoding="utf-8"
            )
            judge = {
                "judge_id": "patterns-eval/invocation/solo-judge",
                "suite_file": "e2e/patterns-eval/evals/invocation.yaml",
                "case_name": "solo-judge",
            }
            conversation = parse_conversation_draft(
                '---\nmodel: "x"\n---\nrole: user\ncontent: |\n  q\n---\nrole: assistant\ncontent: |\n  a\n'
            )
            result = precheck.precheck_cell(tmp, judge, "cand-7", conversation)
            self.assertEqual(result["overall"], "no_checks")


class TestPrecheckStore(unittest.TestCase):
    def test_missing_file_is_empty_not_an_error(self):
        store = precheck.PrecheckStore(Path("/does/not/exist/precheck_results.json"))
        self.assertEqual(len(store), 0)
        self.assertIsNone(store.get("some/judge", "some-candidate"))

    def test_none_path_is_empty_not_an_error(self):
        store = precheck.PrecheckStore(None)
        self.assertEqual(len(store), 0)
        self.assertIsNone(store.get("some/judge", "some-candidate"))

    def test_loads_and_indexes_by_judge_and_candidate_id(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "precheck_results.json"
            records = [
                {"judge_id": "j1", "candidate_id": "c1", "overall": "all_pass", "checks": []},
                {"judge_id": "j1", "candidate_id": "c2", "overall": "some_fail", "checks": []},
                {"judge_id": "j2", "candidate_id": "c1", "overall": "no_checks", "checks": []},
            ]
            path.write_text(json.dumps(records), encoding="utf-8")
            store = precheck.PrecheckStore(path)
            self.assertEqual(len(store), 3)
            self.assertEqual(store.get("j1", "c1")["overall"], "all_pass")
            self.assertEqual(store.get("j1", "c2")["overall"], "some_fail")
            self.assertEqual(store.get("j2", "c1")["overall"], "no_checks")
            self.assertIsNone(store.get("j1", "c3"))
            self.assertIsNone(store.get("j3", "c1"))

    def test_reload_picks_up_a_rewritten_file(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "precheck_results.json"
            path.write_text("[]", encoding="utf-8")
            store = precheck.PrecheckStore(path)
            self.assertEqual(len(store), 0)
            path.write_text(
                json.dumps(
                    [{"judge_id": "j1", "candidate_id": "c1", "overall": "all_pass", "checks": []}]
                ),
                encoding="utf-8",
            )
            store.reload()
            self.assertEqual(len(store), 1)


if __name__ == "__main__":
    unittest.main()
