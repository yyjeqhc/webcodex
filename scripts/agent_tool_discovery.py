"""Bounded offline discovery scoring for the existing agent loop reporter.

Captured retrieval lists are caller-supplied evidence. Scoring does not invoke a
model, execute tools, or prove live Host registration or task completion.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import stat
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CORPUS = ROOT / "scripts/agent_tool_discovery_cases.json"
MAX_BYTES = 2 * 1024 * 1024
MAX_CASES = 256
MAX_CATALOG_FILES = 128
MAX_CATALOG_FILE_BYTES = 512 * 1024
MAX_CATALOG_TOTAL_BYTES = 8 * 1024 * 1024
KINDS = {"exact_name", "paraphrase", "ambiguous", "no_match", "scope_negative"}
EVIDENCE_LIMIT = (
    "Offline scoring of supplied retrieval captures; not live model or Host proof, "
    "invocation success, execution authorization, or end-to-end task completion."
)


def _object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def _reject_constant(value):
    raise ValueError(f"non-JSON constant: {value}")


def _fields(value, expected, context):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError(f"{context}: expected fields {', '.join(sorted(expected))}")


def _strings(value, maximum, context):
    if (not isinstance(value, list) or len(value) > maximum
            or any(not isinstance(item, str) or not item or len(item) > 128 for item in value)
            or len(set(value)) != len(value)):
        raise ValueError(f"{context}: expected bounded unique strings")
    return set(value)


def _read_regular_bytes(path, maximum):
    """Read one stable bounded regular file, rejecting final-path symlinks."""
    path = Path(path)
    fd = -1
    try:
        before = path.lstat()
        if not stat.S_ISREG(before.st_mode):
            raise ValueError("discovery input must be a regular file, not a symlink or special file")
        if before.st_size > maximum:
            raise ValueError("discovery input exceeds bounded size")
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_BINARY", 0)
        fd = os.open(path, flags)
        opened = os.fstat(fd)
        def identity(info):
            return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)
        if not stat.S_ISREG(opened.st_mode) or identity(opened) != identity(before):
            raise ValueError("discovery input changed identity before read")
        with os.fdopen(fd, "rb", closefd=True) as stream:
            fd = -1
            raw = stream.read(maximum + 1)
            after = os.fstat(stream.fileno())
        current = path.lstat()
        if (not stat.S_ISREG(current.st_mode) or identity(after) != identity(opened)
                or identity(current) != identity(opened)):
            raise ValueError("discovery input changed during read")
        if len(raw) > maximum:
            raise ValueError("discovery input exceeds bounded size")
        return raw
    except OSError as exc:
        raise ValueError(f"unable to read discovery input: {exc}") from exc
    finally:
        if fd >= 0:
            os.close(fd)


def _catalog():
    # Names come from canonical declarations, not a duplicate label allowlist.
    definitions = ROOT / "crates/webcodex-tool-contracts/src/tool_definition"
    paths = [definitions.parent / "tool_definition.rs", *sorted(definitions.glob("*.rs"))]
    authority_path = ROOT / "crates/webcodex-core/src/authority.rs"
    if len(paths) + 1 > MAX_CATALOG_FILES:
        raise ValueError("canonical source catalog exceeds file count bound")
    sources = {}
    total = 0
    for path in [*paths, authority_path]:
        raw = _read_regular_bytes(path, MAX_CATALOG_FILE_BYTES)
        total += len(raw)
        if total > MAX_CATALOG_TOTAL_BYTES:
            raise ValueError("canonical source catalog exceeds total byte bound")
        sources[path] = raw.decode("utf-8")
    names = set()
    for path in paths:
        names.update(re.findall(r'\b(?:def|control_def|runner_project_read_def|resource_tool|docx_read_definition|document_read_definition)\(\s*"([a-z_]+)"', sources[path]))
    scopes = set(re.findall(r'pub const SCOPE_[A-Z_]+: &str = "([a-z_:]+)";', sources[authority_path]))
    categories = set(re.findall(r'pub const TOOL_CATEGORY_[A-Z_]+: &str = "([a-z_]+)";', sources[paths[0]]))
    return names, scopes, categories


def _validate_corpus(corpus):
    _fields(corpus, {"schema_version", "corpus_version", "top_k", "metadata", "cases"}, "corpus")
    if type(corpus["schema_version"]) is not int or corpus["schema_version"] != 1:
        raise ValueError("unsupported corpus schema_version")
    if not isinstance(corpus["corpus_version"], str) or not re.fullmatch(r'\d+\.\d+\.\d+', corpus["corpus_version"]):
        raise ValueError("invalid corpus_version")
    if type(corpus["top_k"]) is not int or corpus["top_k"] != 5:
        raise ValueError("corpus top_k must be 5")
    _fields(corpus["metadata"], {"purpose", "evidence_limit", "split_policy", "scope_semantics", "label_semantics"}, "metadata")
    if any(not isinstance(text, str) or not text.strip() or len(text) > 2000 for text in corpus["metadata"].values()):
        raise ValueError("invalid corpus metadata")
    cases = corpus["cases"]
    if not isinstance(cases, list) or not 40 <= len(cases) <= MAX_CASES or len(cases) % 4:
        raise ValueError("corpus requires 40..256 cases divisible by four")
    names, scopes, categories = _catalog()
    for index, case in enumerate(cases, 1):
        _fields(case, {"id", "split", "language", "kind", "prompt", "request", "allowed_scopes", "expected_tools", "forbidden_tools"}, "case")
        if case["id"] != f"discovery-{index:03}":
            raise ValueError("case IDs must be unique sequential discovery-NNN IDs")
        if case["split"] != ("heldout" if index % 4 == 0 else "development"):
            raise ValueError("every fourth case must be predesignated heldout")
        if (not isinstance(case["language"], str) or not isinstance(case["kind"], str)
                or case["language"] not in ("en", "zh") or case["kind"] not in KINDS):
            raise ValueError("invalid case language/kind")
        if not isinstance(case["prompt"], str) or not case["prompt"].strip() or len(case["prompt"]) > 1000:
            raise ValueError("invalid case prompt")
        allowed = _strings(case["allowed_scopes"], 64, "allowed_scopes")
        expected = _strings(case["expected_tools"], 100, "expected_tools")
        forbidden = _strings(case["forbidden_tools"], 100, "forbidden_tools")
        if not allowed <= scopes or not (expected | forbidden) <= names or expected & forbidden:
            raise ValueError("unknown scope/tool or overlapping labels")
        request = case["request"]
        exact = case["kind"] == "exact_name"
        if not isinstance(request, dict) or not set(request) <= {"tool_name", "query", "limit", "category", "intent"}:
            raise ValueError("invalid manifest request fields")
        if type(request.get("limit")) is not int or request["limit"] != corpus["top_k"]:
            raise ValueError("manifest request limit must match top_k")
        if exact:
            if (set(request) != {"tool_name", "limit"} or not isinstance(request["tool_name"], str)
                    or expected != {request["tool_name"]}):
                raise ValueError("exact-name request must have its one canonical label")
        else:
            query = request.get("query")
            if ("tool_name" in request or not isinstance(query, str) or not query.strip()
                    or len(query) > 200 or any(ord(char) < 32 for char in query)):
                raise ValueError("invalid manifest query")
        if "category" in request and (not isinstance(request["category"], str) or request["category"] not in categories):
            raise ValueError("unknown manifest category")
        if "intent" in request and (not isinstance(request["intent"], str) or request["intent"] not in {
                "maintenance", "resources", "coding", "audit", "exploration", "file_transfer", "release", "discovery"}):
            raise ValueError("unknown manifest intent")
        if case["kind"] == "no_match" and expected:
            raise ValueError("no-match case must have no relevant tools")
        if case["kind"] not in ("no_match", "scope_negative") and not expected:
            raise ValueError("positive case needs relevant tools")
        if case["kind"] == "ambiguous" and len(expected) < 2:
            raise ValueError("ambiguous case needs multiple relevant tools")
        if case["kind"] == "scope_negative" and not forbidden:
            raise ValueError("scope-negative case needs forbidden tools")
    for split in ("development", "heldout"):
        selected = [case for case in cases if case["split"] == split]
        if {case["kind"] for case in selected} != KINDS or {case["language"] for case in selected} != {"en", "zh"}:
            raise ValueError("both splits need bilingual coverage and all case kinds")


def _load_json(path):
    raw = _read_regular_bytes(path, MAX_BYTES)
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=_object, parse_constant=_reject_constant)
    except RecursionError as exc:
        raise ValueError("discovery input nesting exceeds parser bounds") from exc


def load_corpus(path=DEFAULT_CORPUS):
    corpus = _load_json(path)
    _validate_corpus(corpus)
    return corpus


def load_results(path):
    """Read strict bounded JSON; evaluate_results validates capture identity/data."""
    captured = _load_json(path)
    _fields(captured, {"schema_version", "corpus_fingerprint", "base_revision", "results"}, "capture")
    return captured


def corpus_fingerprint(corpus):
    _validate_corpus(corpus)
    raw = json.dumps(corpus, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode("utf-8")
    if len(raw) > MAX_BYTES:
        raise ValueError("corpus exceeds bounded size")
    return hashlib.sha256(raw).hexdigest()


def evaluate_results(corpus, captured_json):
    """Score ordered canonical names; incomplete splits keep metrics unavailable.

    Capture fields: schema_version=1, corpus_fingerprint (SHA-256),
    base_revision (full Git SHA), results=[{case_id, returned_tools}]. The
    revision is asserted capture provenance, not independently verified here.
    """
    fingerprint = corpus_fingerprint(corpus)
    _fields(captured_json, {"schema_version", "corpus_fingerprint", "base_revision", "results"}, "capture")
    if type(captured_json["schema_version"]) is not int or captured_json["schema_version"] != 1:
        raise ValueError("unsupported capture schema_version")
    if captured_json["corpus_fingerprint"] != fingerprint:
        raise ValueError("capture corpus_fingerprint mismatch")
    revision = captured_json["base_revision"]
    if not isinstance(revision, str) or not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError("capture base_revision must be a full lowercase Git SHA")
    results = captured_json["results"]
    if not isinstance(results, list) or len(results) > len(corpus["cases"]):
        raise ValueError("capture results exceed corpus size")
    names, _, _ = _catalog()
    cases = {case["id"]: case for case in corpus["cases"]}
    captured = {}
    for result in results:
        _fields(result, {"case_id", "returned_tools"}, "result")
        case_id = result["case_id"]
        if not isinstance(case_id, str) or case_id not in cases or case_id in captured:
            raise ValueError("unknown or duplicate capture case_id")
        returned = _strings(result["returned_tools"], cases[case_id]["request"]["limit"], "returned_tools")
        if not returned <= names:
            raise ValueError("capture returned unknown canonical tool")
        captured[case_id] = result["returned_tools"]
    summaries = {}
    for split in ("development", "heldout"):
        selected = [case for case in corpus["cases"] if case["split"] == split]
        observed = [case for case in selected if case["id"] in captured]
        missing = [case["id"] for case in selected if case["id"] not in captured]
        positive = 0
        hits = 0
        recall_sum = 0.0
        precision_sum = 0.0
        leakage_cases = 0
        forbidden_returns = 0
        negative = 0
        correct_rejections = 0
        for case in observed:
            top = set(captured[case["id"]][:corpus["top_k"]])
            expected = set(case["expected_tools"])
            forbidden = set(case["forbidden_tools"])
            relevant = len(top & expected)
            leaked = len(top & forbidden)
            leakage_cases += bool(leaked)
            forbidden_returns += leaked
            if expected:
                positive += 1
                hits += bool(relevant)
                recall_sum += relevant / len(expected)
                precision_sum += relevant / corpus["top_k"]
            else:
                negative += 1
                correct_rejections += not top
        status = "complete" if not missing else ("incomplete" if observed else "unavailable")
        metrics = None
        if status == "complete":
            metrics = {
                "hit_rate_at_5": hits / positive if positive else None,
                "recall_at_5": recall_sum / positive if positive else None,
                "precision_at_5": precision_sum / positive if positive else None,
                "forbidden_leakage_rate_at_5": leakage_cases / len(observed),
                "correct_rejection_rate": correct_rejections / negative if negative else None,
            }
        summaries[split] = {
            "status": status, "case_count": len(selected), "captured_count": len(observed),
            "missing_count": len(missing), "missing_case_ids": missing, "metrics": metrics,
            "observed_counts": {
                "positive_cases": positive, "positive_hits_at_5": hits,
                "negative_cases": negative, "correct_rejections": correct_rejections,
                "forbidden_leakage_cases_at_5": leakage_cases,
                "forbidden_returns_at_5": forbidden_returns,
            },
        }
    return {
        "schema_version": 1, "corpus_version": corpus["corpus_version"],
        "corpus_fingerprint": fingerprint, "base_revision": revision,
        "top_k": corpus["top_k"], "evidence_limit": EVIDENCE_LIMIT,
        "status": "complete" if len(captured) == len(cases) else ("incomplete" if captured else "unavailable"),
        "splits": summaries,
    }
