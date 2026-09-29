import unittest

from backend.eval_case_yaml import parse_case_assertions

DISCOVERY_YAML = """name: discovery
cases:
  - name: newtype-mixed-ids
    assertions:
      - { type: text_contains, value: "patterns:newtype" }
      - { type: text_not_contains, value: "patterns:entities-value-objects-services" }

  - name: paired-add-propagator
    assertions:
      - { type: text_contains, value: "patterns:configuring-logging" }
      - { type: text_not_contains, value: "patterns:emitting-logs" }
      - type: judge
        prompt: |
          The answer selects patterns:configuring-logging because the W3C
          propagator is installed once at process bootstrap.
"""

INVOCATION_YAML = """name: invocation
cases:
  - name: newtype
    assertions:
      - { type: script, runtime: python, script: { path: evals/scripts/check_newtype.py } }
      - type: judge
        prompt: |
          The code introduces a UserId type that wraps a string.
      - { type: tokens, metric: total, op: "<", value: 6000 }

  - name: configuring-logging
    assertions:
      - { type: script, runtime: python, script: { path: evals/scripts/check_configuring_logging.py } }
      - type: judge
        prompt: |
          The bootstrap installs a single subscriber registry.
      # A comment line sitting between two block-style assertion items --
      # must not be mistaken for the end of the assertions: block sequence.
      - { type: tokens, metric: total, op: "<", value: 14000 }
"""


class TestParseCaseAssertions(unittest.TestCase):
    def test_flow_style_text_assertions(self):
        assertions = parse_case_assertions(DISCOVERY_YAML, "newtype-mixed-ids")
        self.assertEqual(
            assertions,
            [
                {"type": "text_contains", "value": "patterns:newtype"},
                {"type": "text_not_contains", "value": "patterns:entities-value-objects-services"},
            ],
        )

    def test_block_style_judge_assertion_with_multiline_prompt(self):
        assertions = parse_case_assertions(DISCOVERY_YAML, "paired-add-propagator")
        judge = next(a for a in assertions if a["type"] == "judge")
        self.assertIn("W3C", judge["prompt"])
        self.assertIn("propagator is installed once at process bootstrap.", judge["prompt"])

    def test_script_assertion_nested_flow_mapping(self):
        assertions = parse_case_assertions(INVOCATION_YAML, "newtype")
        script = next(a for a in assertions if a["type"] == "script")
        self.assertEqual(script["runtime"], "python")
        self.assertEqual(script["script"], {"path": "evals/scripts/check_newtype.py"})

    def test_tokens_assertion_numeric_value(self):
        assertions = parse_case_assertions(INVOCATION_YAML, "newtype")
        tokens = next(a for a in assertions if a["type"] == "tokens")
        self.assertEqual(tokens, {"type": "tokens", "metric": "total", "op": "<", "value": 6000})

    def test_comment_between_assertion_items_does_not_truncate_the_list(self):
        assertions = parse_case_assertions(INVOCATION_YAML, "configuring-logging")
        self.assertEqual([a["type"] for a in assertions], ["script", "judge", "tokens"])

    def test_unknown_case_name_raises(self):
        with self.assertRaises(ValueError):
            parse_case_assertions(DISCOVERY_YAML, "no-such-case")

    def test_case_without_assertions_block_raises(self):
        with self.assertRaises(ValueError):
            parse_case_assertions("name: x\ncases:\n  - name: bare\n    when: {}\n", "bare")


if __name__ == "__main__":
    unittest.main()
