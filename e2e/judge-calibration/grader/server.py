#!/usr/bin/env python3
"""Judge-calibration grader app -- local-only, stdlib-only server.

Usage:
    python3 server.py [--port 8765] [--mined-dir PATH] [--evals-dir PATH]

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
from backend.candidates import CandidateStore  # noqa: E402

DEFAULT_PORT = 8765


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help=f"port to bind (default {DEFAULT_PORT})")
    parser.add_argument(
        "--mined-dir",
        type=Path,
        default=APP_DIR.parent / "mined",
        help="path to e2e/judge-calibration/mined (default: ../mined relative to this file)",
    )
    parser.add_argument(
        "--evals-dir",
        type=Path,
        default=APP_DIR.parent / "evals",
        help="path to e2e/judge-calibration/evals -- export destination (default: ../evals)",
    )
    parser.add_argument("--no-browser", action="store_true", help="don't auto-open a browser tab")
    args = parser.parse_args(argv)

    candidates_path = args.mined_dir / "candidates.jsonl"
    if not candidates_path.exists():
        print(f"error: {candidates_path} not found. Point --mined-dir at e2e/judge-calibration/mined.", file=sys.stderr)
        return 1

    store = CandidateStore(candidates_path)
    ctx = AppContext(store, mined_dir=args.mined_dir, evals_dir=args.evals_dir, static_dir=APP_DIR / "static")

    host = "127.0.0.1"
    httpd = build_server(ctx, host, args.port)
    url = f"http://{host}:{args.port}/"
    print(f"Judge-calibration grader serving {len(store)} candidates at {url}")
    print(f"  mined dir:  {args.mined_dir}")
    print(f"  evals dir:  {args.evals_dir}")
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
