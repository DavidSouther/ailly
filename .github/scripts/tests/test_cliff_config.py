"""Configuration contract for root-plugin release notes."""

from pathlib import Path
import unittest


CONFIG = Path(__file__).resolve().parents[3] / "cliff.toml"


class CliffConfigTests(unittest.TestCase):
    def test_ignores_independent_import_directories(self) -> None:
        self.assertIn(
            'exclude_paths = ["analyzer/**", "harness/**"]',
            CONFIG.read_text(encoding="utf-8"),
        )


if __name__ == "__main__":
    unittest.main()
