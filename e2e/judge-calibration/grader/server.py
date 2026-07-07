#!/usr/bin/env python3
"""Judge-calibration grader app -- local-only, stdlib-only server.

Usage:
    python3 server.py [--port 8765] [--mined-dir PATH] [--evals-dir PATH] [--judges-path PATH] [--precheck-path PATH]

Binds to 127.0.0.1 ONLY. There is deliberately no --host flag: this data
contains verbatim excerpts of the user's private/client codebases and must
never be reachable off this machine.
"""
from __future__ import annotations

import argparse
import sys
import webbrowser
from pathlib import Path

APP_DIR = Path(__file__).resolve().parent
if str(APP_DIR) not in sys.path:
    sys.path.insert(0, str(APP_DIR))

from backend.app import AppContext, build_server  # noqa: E402
from backend.judges import JudgeRegistry  # noqa: E402
from backend.matrix import MatrixStore  # noqa: E402
from backend.precheck import PrecheckStore  # noqa: E402

DEFAULT_PORT = 8765


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help=f"port to bind (default {DEFAULT_PORT})")
    parser.add_argument(
        "--mined-dir",
        type=Path,
        default=APP_DIR.parent / "mined",
        help="path to e2e/judge-calibration/mined, containing matrix.jsonl and matrix/ (default: ../mined)",
    )
    parser.add_argument(
        "--evals-dir",
        type=Path,
        default=APP_DIR.parent / "evals",
        help="path to e2e/judge-calibration/evals -- labels.yaml lives here and is written to directly (default: ../evals)",
    )
    parser.add_argument(
        "--judges-path",
        type=Path,
        default=None,
        help="path to judges.yaml (default: <evals-dir>/judges.yaml)",
    )
    parser.add_argument(
        "--precheck-path",
        type=Path,
        default=None,
        help=(
            "path to precheck_results.json, a cached batch run of "
            "backend/precheck.py's real deterministic sibling-assertion "
            "checks (default: <mined-dir>/precheck_results.json). Purely "
            "informational and optional -- missing this file just means no "
            "'Deterministic pre-check' hint is shown, grading still works."
        ),
    )
    parser.add_argument("--no-browser", action="store_true", help="don't auto-open a browser tab")
    args = parser.parse_args(argv)

    judges_path = args.judges_path or (args.evals_dir / "judges.yaml")
    matrix_path = args.mined_dir / "matrix.jsonl"
    precheck_path = args.precheck_path or (args.mined_dir / "precheck_results.json")

    if not judges_path.exists():
        print(f"error: {judges_path} not found. Point --judges-path at evals/judges.yaml.", file=sys.stderr)
        return 1
    if not matrix_path.exists():
        print(f"error: {matrix_path} not found. Point --mined-dir at e2e/judge-calibration/mined.", file=sys.stderr)
        return 1

    judges = JudgeRegistry(judges_path)
    matrix = MatrixStore(matrix_path, mined_dir=args.mined_dir)
    precheck = PrecheckStore(precheck_path)
    ctx = AppContext(
        judges, matrix, evals_dir=args.evals_dir, static_dir=APP_DIR / "static", precheck=precheck
    )

    host = "127.0.0.1"
    httpd = build_server(ctx, host, args.port)
    url = f"http://{host}:{args.port}/"
    pickable = sum(1 for j in ctx.list_judges() if j["pickable"])
    print(
        f"Judge-calibration grader serving {len(judges)} judges "
        f"({len(matrix)} relevant matrix cells, {pickable} judges with ungraded cells) at {url}"
    )
    print(f"  judges:     {judges_path}")
    print(f"  matrix:     {matrix_path}")
    print(
        f"  precheck:   {precheck_path}"
        + (f" ({len(precheck)} cells, informational only)" if precheck_path.exists() else " (not found -- pre-check panel hidden)")
    )
    print(f"  labels.yaml (written to directly): {ctx.labels_path}")
    print("  bound to 127.0.0.1 only (not reachable off this machine)")
    print("Press Ctrl+C to stop.")

    if not args.no_browser:
        try:
            webbrowser.open(url)
        except Exception:
            pass

    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nshutting down")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
