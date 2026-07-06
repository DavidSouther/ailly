#!/usr/bin/env python3
"""Grade ~20 real candidates via the live API to exercise the heuristic
pre-fill cadence without waiting for a human to click through 20 rounds by
hand. Points at whatever server is already running (default port 8765).

Usage: python3 scripts/simulate_grading.py [--port 8765] [--count 22]
"""
from __future__ import annotations

import argparse
import json
import urllib.request


def get_json(url):
    with urllib.request.urlopen(url) as resp:
        return json.loads(resp.read())


def post_json(url, payload):
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, data=data, method="POST", headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def deterministic_label(text: str) -> str:
    """A cheap deterministic pass/fail split so the simulated grades form two
    coherent clusters a cosine-similarity heuristic can actually pick up on:
    responses mentioning error/fail/revert words -> "fail", else "pass"."""
    lowered = text.lower()
    fail_markers = ("error", "fail", "revert", "broken", "wrong", "bug")
    return "fail" if any(m in lowered for m in fail_markers) else "pass"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--count", type=int, default=22)
    args = parser.parse_args()

    base = f"http://127.0.0.1:{args.port}"

    before = get_json(f"{base}/api/state")
    print(f"before: graded={before['graded']} suggestions_count={before['suggestions_count']}")

    candidates = get_json(f"{base}/api/candidates?status=ungraded")["candidates"]
    graded = 0
    recompute_seen = False
    for c in candidates:
        if graded >= args.count:
            break
        detail = get_json(f"{base}/api/candidate/{c['id']}")
        label = deterministic_label(detail.get("response", ""))
        result = post_json(f"{base}/api/grade", {"id": c["id"], "label": label})
        if result.get("suggestions_recomputed"):
            recompute_seen = True
        graded += 1

    after = get_json(f"{base}/api/state")
    print(f"after:  graded={after['graded']} suggestions_count={after['suggestions_count']} "
          f"computed_at_grade_count={after['suggestions_computed_at_grade_count']}")
    print(f"recompute triggered during this run: {recompute_seen}")

    suggestions_list = get_json(f"{base}/api/candidates?status=ungraded")["candidates"]
    with_suggestions = [c for c in suggestions_list if c.get("suggestion")]
    print(f"ungraded candidates now carrying a suggestion: {len(with_suggestions)}")
    for c in with_suggestions[:5]:
        print(f"  {c['id']}: suggested={c['suggestion']['label']} "
              f"confidence={c['suggestion']['confidence']} "
              f"neighbor={c['suggestion']['neighbor_id']} sim={c['suggestion']['top_similarity']}")


if __name__ == "__main__":
    main()
