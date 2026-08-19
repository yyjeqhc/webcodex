#!/usr/bin/env python3
"""Small, explicit operator surface for WebCodex release control-plane steps."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

if __package__:
    from . import collect_release_bundle as collector
    from . import release_publication as publication
    from . import release_readiness as readiness
else:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import collect_release_bundle as collector
    import release_publication as publication
    import release_readiness as readiness


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="WebCodex release operator. Mutating publication steps remain separate and explicit."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    preflight = subparsers.add_parser(
        "preflight",
        help="verify exact release source, version availability, and publication identities",
    )
    preflight.add_argument("--version", required=True)
    preflight.add_argument("--source-sha", required=True)
    preflight.add_argument("--root", type=Path, default=Path.cwd())
    preflight.add_argument("--repo", default=collector.DEFAULT_REPO)
    preflight.add_argument("--timeout", type=float, default=30.0)

    reclaim_tag = subparsers.add_parser(
        "reclaim-tag",
        help="delete an explicitly confirmed failed pre-publication version tag after bounded safety checks",
    )
    reclaim_tag.add_argument("--version", required=True)
    reclaim_tag.add_argument("--root", type=Path, default=Path.cwd())
    reclaim_tag.add_argument("--repo", default=collector.DEFAULT_REPO)
    reclaim_tag.add_argument("--confirm", required=True, help="must exactly equal v<VERSION>")
    reclaim_tag.add_argument("--timeout", type=float, default=30.0)
    reclaim_tag.add_argument(
        "--allow-public-release-check",
        action="store_true",
        help="allow unauthenticated public Release lookup only after separately confirming no draft release exists",
    )

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
    build_start = subparsers.add_parser(
        "build-start",
        help="dispatch one exact-tag release-build run and record durable correlation state",
    )
    build_start.add_argument("--source-sha", required=True)
    build_start.add_argument("--tag", required=True)
    build_start.add_argument("--state-file", type=Path, required=True)
    build_start.add_argument("--repo", default=collector.DEFAULT_REPO)
    build_start.add_argument("--timeout", type=float, default=30.0)
    build_start.add_argument("--resolve-secs", type=int, default=60)

    build_status = subparsers.add_parser(
        "build-status",
        help="recover/observe the exact release-build run recorded in a durable state file",
    )
    build_status.add_argument("--state-file", type=Path, required=True)
    build_status.add_argument("--timeout", type=float, default=30.0)
    build_status.add_argument("--wait-secs", type=int, default=0)

    stage_npm = subparsers.add_parser(
        "stage-npm",
        help="stage and smoke the npm package using exact retained linux-x64 CI binaries",
    )
    stage_npm.add_argument("--bundle-dir", type=Path, required=True)
    stage_npm.add_argument("--source-root", type=Path, default=Path.cwd())
    stage_npm.add_argument("--output-dir", type=Path, required=True)
    stage_npm.add_argument("--repo", default=collector.DEFAULT_REPO)

    verify_draft = subparsers.add_parser(
        "verify-draft",
        help="verify draft GitHub Release asset digests against the retained bundle without re-downloading",
    )
    verify_draft.add_argument("--bundle-dir", type=Path, required=True)
    verify_draft.add_argument("--repo", default=collector.DEFAULT_REPO)
    verify_draft.add_argument("--timeout", type=float, default=30.0)

    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.command == "preflight":
        try:
            summary = publication.preflight_release(
                repo=args.repo,
                version=args.version,
                source_sha=args.source_sha,
                root=args.root,
                timeout=args.timeout,
            )
        except (collector.CollectionError, publication.PublicationError) as exc:
            print(f"release operator preflight failed: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 0

    if args.command == "reclaim-tag":
        try:
            summary = publication.reclaim_prepublication_tag(
                repo=args.repo,
                version=args.version,
                root=args.root,
                confirm=args.confirm,
                timeout=args.timeout,
                allow_public_release_check=args.allow_public_release_check,
            )
        except (collector.CollectionError, publication.PublicationError) as exc:
            print(f"release operator reclaim-tag failed: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 0

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

    if args.command == "build-start":
        try:
            summary, exit_code = publication.start_build(
                repo=args.repo,
                source_sha=args.source_sha,
                tag=args.tag,
                state_file=args.state_file,
                timeout=args.timeout,
                resolve_secs=args.resolve_secs,
            )
        except (collector.CollectionError, publication.PublicationError) as exc:
            print(f"release operator build-start failed: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(summary, indent=2, sort_keys=True))
        return exit_code

    if args.command == "build-status":
        try:
            summary, exit_code = publication.status_build(
                state_file=args.state_file,
                timeout=args.timeout,
                wait_secs=args.wait_secs,
            )
        except (collector.CollectionError, publication.PublicationError) as exc:
            print(f"release operator build-status failed: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(summary, indent=2, sort_keys=True))
        return exit_code

    if args.command == "stage-npm":
        try:
            summary = publication.stage_npm(
                repo=args.repo,
                bundle_dir=args.bundle_dir,
                source_root=args.source_root,
                output_dir=args.output_dir,
            )
        except (collector.CollectionError, publication.PublicationError) as exc:
            print(f"release operator stage-npm failed: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 0

    if args.command == "verify-draft":
        try:
            summary = publication.verify_draft_assets(
                repo=args.repo,
                bundle_dir=args.bundle_dir,
                timeout=args.timeout,
            )
        except (collector.CollectionError, publication.PublicationError) as exc:
            print(f"release operator verify-draft failed: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 0

    raise AssertionError(f"unhandled release operator command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
