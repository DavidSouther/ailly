"""Dependency-free term-frequency cosine-similarity heuristic.

Pure stdlib (``collections.Counter``, ``re``, ``math``) -- no numpy/sklearn,
per the task's data scale (663 short-to-medium text records fits easily in
memory and in O(n * k) time using this approach).

The heuristic: tokenize each candidate's response text into a lowercase
word-frequency vector, then for an ungraded candidate compute cosine
similarity against every already-graded candidate's vector. If a clear
majority of the top-K most similar graded neighbors agree on one label, and
the best similarity clears a minimum threshold, surface that as a
suggestion -- along with the neighbor id/label/score that justifies it, so a
human reviewer can judge whether to trust it.
"""
from __future__ import annotations

import math
import re
from collections import Counter
from dataclasses import dataclass, field

# A small stopword list to blunt the effect of boilerplate phrasing shared by
# nearly every candidate (e.g. "Using developer:ailly to coordinate this
# session.") so similarity reflects actual topical/content overlap rather
# than shared scaffolding language.
STOPWORDS = frozenset(
    """
    a an the this that these those i you your yours we our ours he she it
    they them their to of in on for is are was were be been being with as
    at by from which who whom what when where why how let lets let's me my
    mine now have has had having do does did done not no yes so than then
    there here will would could should can may might must shall about into
    over under again further once here there all any both each few more
    most other some such only own same too very s t just don should've now
    d ll m o re ve y ain aren couldn didn doesn hadn hasn haven isn ma
    mightn mustn needn shan shouldn wasn weren won wouldn and or but if
    because until while up down out off above below between during before
    after
    """.split()
)

_TOKEN_RE = re.compile(r"[a-z][a-z0-9']*")


def tokenize(text: str) -> Counter:
    """Lowercase word-frequency vector for ``text``, stopwords removed."""
    if not text:
        return Counter()
    tokens = [
        t for t in _TOKEN_RE.findall(text.lower())
        if len(t) >= 2 and t not in STOPWORDS
    ]
    return Counter(tokens)


def cosine_similarity(a: Counter, b: Counter) -> float:
    """Cosine similarity between two term-frequency vectors, in [0, 1]."""
    if not a or not b:
        return 0.0
    # Iterate the smaller vector for a cheap dot product.
    if len(a) > len(b):
        a, b = b, a
    dot = sum(count * b.get(term, 0) for term, count in a.items())
    if dot == 0:
        return 0.0
    norm_a = math.sqrt(sum(v * v for v in a.values()))
    norm_b = math.sqrt(sum(v * v for v in b.values()))
    if norm_a == 0 or norm_b == 0:
        return 0.0
    return dot / (norm_a * norm_b)


@dataclass
class Suggestion:
    label: str
    confidence: float
    top_similarity: float
    neighbor_id: str
    neighbor_label: str
    neighbors: list[dict] = field(default_factory=list)
    method: str = "cosine-tf-topk"
    k: int = 5

    def to_dict(self) -> dict:
        return {
            "label": self.label,
            "confidence": round(self.confidence, 4),
            "top_similarity": round(self.top_similarity, 4),
            "neighbor_id": self.neighbor_id,
            "neighbor_label": self.neighbor_label,
            "neighbors": self.neighbors,
            "method": self.method,
            "k": self.k,
        }


def suggest_for_vector(
    vector: Counter,
    graded: list[tuple[str, str, Counter]],
    k: int = 5,
    sim_threshold: float = 0.12,
    majority_threshold: float = 0.6,
    min_neighbors: int = 2,
) -> Suggestion | None:
    """Suggest a label for ``vector`` from a list of (id, label, vector) graded examples.

    Returns None when there isn't enough signal (too few graded neighbors,
    best similarity below threshold, or no clear majority among the top-K).
    """
    if len(graded) < min_neighbors:
        return None

    scored = [
        (cosine_similarity(vector, gvec), gid, glabel)
        for gid, glabel, gvec in graded
    ]
    scored.sort(key=lambda t: t[0], reverse=True)
    top = scored[:k]
    top = [t for t in top if t[0] > 0]
    if len(top) < min_neighbors:
        return None
    if top[0][0] < sim_threshold:
        return None

    counts = Counter(label for _, _, label in top)
    best_label, best_count = counts.most_common(1)[0]
    majority_fraction = best_count / len(top)
    if majority_fraction < majority_threshold:
        return None

    # Justify with the highest-similarity neighbor that actually holds the
    # winning label (not just the single closest neighbor overall).
    winning_neighbors = [t for t in top if t[2] == best_label]
    best_sim, best_id, best_neighbor_label = winning_neighbors[0]

    return Suggestion(
        label=best_label,
        confidence=majority_fraction,
        top_similarity=best_sim,
        neighbor_id=best_id,
        neighbor_label=best_neighbor_label,
        neighbors=[{"id": gid, "label": glabel, "similarity": round(sim, 4)} for sim, gid, glabel in top],
        k=k,
    )


def compute_suggestions(
    texts: dict[str, str],
    labels: dict[str, str],
    k: int = 5,
    sim_threshold: float = 0.12,
    majority_threshold: float = 0.6,
    min_neighbors: int = 2,
) -> dict[str, dict]:
    """Compute suggestions for every ungraded id in ``texts``.

    ``labels`` maps id -> "pass"/"fail"/"TODO" (or absent = ungraded).
    Returns {id: suggestion_dict} only for ids the heuristic is confident
    about.
    """
    vectors = {cid: tokenize(text) for cid, text in texts.items()}

    graded: list[tuple[str, str, Counter]] = [
        (cid, labels[cid], vectors[cid])
        for cid in texts
        if labels.get(cid) in ("pass", "fail")
    ]

    results: dict[str, dict] = {}
    for cid, text in texts.items():
        if labels.get(cid) in ("pass", "fail"):
            continue
        # Never suggest a candidate against itself as a "neighbor".
        neighbors = [g for g in graded if g[0] != cid]
        suggestion = suggest_for_vector(
            vectors[cid],
            neighbors,
            k=k,
            sim_threshold=sim_threshold,
            majority_threshold=majority_threshold,
            min_neighbors=min_neighbors,
        )
        if suggestion is not None:
            results[cid] = suggestion.to_dict()
    return results
