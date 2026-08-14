#!/usr/bin/env python3
"""Small, explicit operator surface for WebCodex release control-plane steps."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

if __package__:
    from . import collect_release_bundle as collector
    from . import release_readiness as readiness
else:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import collect_release_bundle as collector
    import release_readiness as readiness


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
    readiness_start = subparsers.add_parser(
        "readiness-start",
        help="dispatch exact-main pre-tag readiness and record durable correlation state",
    )
    readiness_start.add_argument("--source-sha", required=True)
    readiness_start.add_argument("--state-file", type=Path, required=True)
    readiness_start.add_argument("--repo", default=collector.DEFAULT_REPO)
    readiness_start.add_argument("--timeout", type=float, default=30.0)
    readiness_start.add_argument("--resolve-secs", type=int, default=60)

    readiness_status = subparsers.add_parser(
        "readiness-status",
        help="recover/observe the exact readiness run recorded in a durable state file",
    )
    readiness_status.add_argument("--state-file", type=Path, required=True)
    readiness_status.add_argument("--timeout", type=float, default=30.0)
    readiness_status.add_argument("--wait-secs", type=int, default=0)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.command == "collect":
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

    if args.command == "readiness-start":
        try:
            summary, exit_code = readiness.start_readiness(
                repo=args.repo,
                source_sha=args.source_sha,
                state_file=args.state_file,
                timeout=args.timeout,
                resolve_secs=args.resolve_secs,
            )
        except (collector.CollectionError, readiness.ReadinessError) as exc:
            print(f"release operator readiness-start failed: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(summary, indent=2, sort_keys=True))
        return exit_code

    if args.command == "readiness-status":
        try:
            summary, exit_code = readiness.status_readiness(
                state_file=args.state_file,
                timeout=args.timeout,
                wait_secs=args.wait_secs,
            )
        except (collector.CollectionError, readiness.ReadinessError) as exc:
            print(f"release operator readiness-status failed: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(summary, indent=2, sort_keys=True))
        return exit_code

    raise AssertionError(f"unhandled release operator command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
