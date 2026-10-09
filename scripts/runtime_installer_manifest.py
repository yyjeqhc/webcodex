"""The optional Runtime installer contract and its deterministic legacy view."""
from __future__ import annotations

import json
import re

RUNTIME_TARGETS = {
    f"{platform}-runtime-{fmt}": (platform, fmt)
    for platform in ("linux-x64", "linux-arm64") for fmt in ("deb", "rpm")
}


def installer_filename(version: str, target: str) -> str:
    platform, fmt = RUNTIME_TARGETS[target]
    return f"webcodex-runtime-v{version}-{platform}.{fmt}"


def source_filename(version: str, platform: str) -> str:
    return f"webcodex-runtime-source-v{version}-{platform}.json"


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate manifest field: {key}")
        value[key] = item
    return value


def parse(raw: bytes | str) -> dict:
    return json.loads(raw, object_pairs_hook=unique_object)


def encode(value: dict) -> str:
    return json.dumps(value, indent=2) + "\n"


def legacy_projection(value: dict) -> dict:
    if not isinstance(value, dict) or set(value) != {"schema_version", "version", "binaries", "artifacts", "installers"} or type(value.get("schema_version")) is not int or value["schema_version"] != 2:
        raise ValueError("unsupported manifest-v2 schema")
    installers = value["installers"]
    if not isinstance(installers, dict) or not set(RUNTIME_TARGETS) <= set(installers):
        raise ValueError("manifest-v2 Runtime installer set is incomplete")
    return {"version": value["version"], "binaries": value["binaries"], "artifacts": value["artifacts"],
            "installers": {key: item for key, item in installers.items() if key not in RUNTIME_TARGETS}}


def validate(value: dict, legacy: dict, *, version: str, repo: str, full_targets: set[str]) -> dict:
    projection = legacy_projection(value)
    if value["version"] != version or projection != legacy or set(value["installers"]) != full_targets | set(RUNTIME_TARGETS):
        raise ValueError("manifest-v2 does not bind the exact legacy Full view")
    result = {}
    sources = {}
    for target, (platform, fmt) in RUNTIME_TARGETS.items():
        item = value["installers"][target]
        filename = installer_filename(version, target)
        source = source_filename(version, platform)
        base = f"https://github.com/{repo}/releases/download/v{version}/"
        fields = {"platform", "format", "flavor", "filename", "url", "sha256", "source_manifest_url", "source_manifest_sha256"}
        if not isinstance(item, dict) or set(item) != fields or (item["platform"], item["format"], item["flavor"], item["filename"], item["url"], item["source_manifest_url"]) != (platform, fmt, "runtime", filename, base + filename, base + source):
            raise ValueError(f"invalid Runtime installer identity: {target}")
        for key in ("sha256", "source_manifest_sha256"):
            if not isinstance(item[key], str) or not re.fullmatch(r"[0-9a-f]{64}", item[key]):
                raise ValueError(f"invalid Runtime digest: {target}")
        if sources.setdefault(platform, item["source_manifest_sha256"]) != item["source_manifest_sha256"]:
            raise ValueError("Runtime formats disagree on source provenance")
        result[target] = {**item, "source_manifest_filename": source}
    return result
