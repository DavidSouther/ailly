import unittest
from collections import Counter

from backend import similarity


class TestTokenize(unittest.TestCase):
    def test_lowercases_and_splits_words(self):
        vec = similarity.tokenize("Hello World, HELLO Banana!")
        self.assertEqual(vec["hello"], 2)
        self.assertEqual(vec["banana"], 1)

    def test_removes_stopwords(self):
        vec = similarity.tokenize("the quick brown fox and the lazy dog")
        self.assertNotIn("the", vec)
        self.assertNotIn("and", vec)
        self.assertIn("quick", vec)
        self.assertIn("fox", vec)

    def test_empty_text(self):
        self.assertEqual(similarity.tokenize(""), Counter())
        self.assertEqual(similarity.tokenize(None), Counter())


class TestCosineSimilarity(unittest.TestCase):
    def test_identical_vectors_are_similarity_one(self):
        v = Counter({"foo": 3, "bar": 1})
        self.assertAlmostEqual(similarity.cosine_similarity(v, v), 1.0)

    def test_disjoint_vectors_are_zero(self):
        a = Counter({"foo": 1})
        b = Counter({"bar": 1})
        self.assertEqual(similarity.cosine_similarity(a, b), 0.0)

    def test_empty_vector_is_zero(self):
        self.assertEqual(similarity.cosine_similarity(Counter(), Counter({"foo": 1})), 0.0)

    def test_partial_overlap_between_zero_and_one(self):
        a = Counter({"foo": 2, "bar": 1})
        b = Counter({"foo": 1, "baz": 5})
        sim = similarity.cosine_similarity(a, b)
        self.assertGreater(sim, 0.0)
        self.assertLess(sim, 1.0)

    def test_symmetric(self):
        a = Counter({"foo": 2, "bar": 1, "qux": 4})
        b = Counter({"foo": 1, "baz": 5})
        self.assertAlmostEqual(similarity.cosine_similarity(a, b), similarity.cosine_similarity(b, a))


class TestSuggestForVector(unittest.TestCase):
    def _vec(self, text):
        return similarity.tokenize(text)

    def test_returns_none_with_too_few_neighbors(self):
        vec = self._vec("database migration rollback plan")
        graded = [("g1", "pass", self._vec("database migration rollback plan"))]
        self.assertIsNone(similarity.suggest_for_vector(vec, graded, min_neighbors=2))

    def test_returns_none_below_similarity_threshold(self):
        vec = self._vec("totally unrelated topic about weather")
        graded = [
            ("g1", "pass", self._vec("database migration rollback plan execution")),
            ("g2", "pass", self._vec("database schema migration tooling")),
        ]
        result = similarity.suggest_for_vector(vec, graded, sim_threshold=0.5, min_neighbors=2)
        self.assertIsNone(result)

    def test_clear_majority_produces_suggestion(self):
        vec = self._vec("refactor the authentication middleware for clarity")
        graded = [
            ("g1", "pass", self._vec("refactor the authentication middleware for readability")),
            ("g2", "pass", self._vec("refactor authentication middleware cleanly")),
            ("g3", "fail", self._vec("delete unrelated database backup files")),
        ]
        result = similarity.suggest_for_vector(vec, graded, k=3, sim_threshold=0.1, majority_threshold=0.6)
        self.assertIsNotNone(result)
        self.assertEqual(result.label, "pass")
        self.assertIn(result.neighbor_id, ("g1", "g2"))
        # g3 shares no non-stopword tokens with vec, so only g1/g2 (nonzero
        # similarity) count among the top-k neighbors.
        self.assertEqual(len(result.neighbors), 2)

    def test_no_majority_when_split(self):
        vec = self._vec("shared vocabulary words appear here in both")
        graded = [
            ("g1", "pass", self._vec("shared vocabulary words appear here")),
            ("g2", "fail", self._vec("shared vocabulary words appear there")),
        ]
        result = similarity.suggest_for_vector(vec, graded, k=2, sim_threshold=0.01, majority_threshold=0.6)
        self.assertIsNone(result)


class TestComputeSuggestions(unittest.TestCase):
    def test_skips_already_graded_ids(self):
        texts = {
            "a": "refactor the authentication middleware for clarity",
            "b": "refactor authentication middleware cleanly please",
            "c": "already graded so should not get a suggestion entry",
        }
        labels = {"a": "pass", "b": "pass", "c": "pass"}
        result = similarity.compute_suggestions(texts, labels)
        self.assertEqual(result, {})

    def test_produces_suggestions_only_for_ungraded(self):
        texts = {
            "graded-1": "refactor the authentication middleware for clarity and safety",
            "graded-2": "refactor authentication middleware cleanly and safely please",
            "graded-3": "delete unrelated database backup files entirely now",
            "ungraded-1": "refactor the authentication middleware please for clarity",
        }
        labels = {"graded-1": "pass", "graded-2": "pass", "graded-3": "fail"}
        result = similarity.compute_suggestions(texts, labels, k=3, sim_threshold=0.1)
        self.assertIn("ungraded-1", result)
        self.assertEqual(result["ungraded-1"]["label"], "pass")
        self.assertNotIn("graded-1", result)


if __name__ == "__main__":
    unittest.main()
