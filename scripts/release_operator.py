#!/usr/bin/env python3
"""Small, explicit operator surface for WebCodex release control-plane steps."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

if __package__:
    from . import collect_release_bundle as collector
else:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import collect_release_bundle as collector


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="WebCodex release operator. Mutating publication steps remain separate and explicit."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    collect = subparsers.add_parser(
        "collect",
        help="download and verify one assembled release-build bundle by locked workflow run id",
    )
    collect.add_argument("--run-id", type=int, required=True)
    collect.add_argument("--source-sha", required=True)
    collect.add_argument("--tag", required=True)
    collect.add_argument("--output-dir", type=Path, required=True)
    collect.add_argument("--repo", default=collector.DEFAULT_REPO)
    collect.add_argument("--timeout", type=float, default=120.0)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.command != "collect":
        raise AssertionError(f"unhandled release operator command: {args.command}")
    try:
        summary = collector.collect_bundle(
            repo=args.repo,
            run_id=args.run_id,
            expected_source_sha=args.source_sha,
            expected_tag=args.tag,
            output_dir=args.output_dir,
            timeout=args.timeout,
        )
    except collector.CollectionError as exc:
        print(f"release operator collect failed: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
