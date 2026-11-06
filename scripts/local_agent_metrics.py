"""Client event accounting for e2e_local_agent_execution; no token estimates."""
from collections import Counter
import json


def json_bytes(value):
    # Discovery budget unit: compact tools/list result, not its RPC envelope.
    return len(json.dumps(value, separators=(",", ":")).encode())


def discovery_metrics(inventory):
    return {"tools": [tool["name"] for tool in inventory["tools"]],
            "json_bytes": json_bytes(inventory),
            "tool_bytes": {tool["name"]: json_bytes(tool) for tool in inventory["tools"]},
            "input_schema_bytes": {tool["name"]: json_bytes(tool["inputSchema"]) for tool in inventory["tools"]}}


def text_chars(content):
    if isinstance(content, str):
        return len(content)
    if isinstance(content, list):
        return sum(len(block.get("text", "")) for block in content
                   if isinstance(block, dict) and block.get("type") == "text")
    return None


def client_metrics(client, frames):
    calls, feedback, usage, rounds, results, discovery = [], [], [], {}, [], []
    tools_by_id, available_tools = {}, None
    seen_calls, seen_results = set(), set()
    def blocks(message):
        content = message.get("content", [])
        return [block for block in content if isinstance(block, dict)] if isinstance(content, list) else []
    for index, frame in enumerate(frames):
        if client == "codex":
            item = frame.get("item", {})
            if frame.get("type") == "item.completed":
                kind = item.get("type")
                if kind == "mcp_tool_call":
                    name = item.get("tool", "unknown")
                    calls.append(name)
                    result = item.get("result") or {}
                    error = item.get("error") or {}
                    results.append({"call_id": item.get("id"), "tool": name,
                                    "returned_text_chars": text_chars(result.get("content")) if result else text_chars(error.get("message")),
                                    "result_json_chars": len(json.dumps(result or error, ensure_ascii=False, separators=(",", ":")))})
                elif kind in {"command_execution", "file_change"}:
                    calls.append(kind)
                    results.append({"call_id": item.get("id"), "tool": kind,
                                    "returned_text_chars": text_chars(item.get("aggregated_output")),
                                    "note": "aggregated command output; file-change events omit the tool response"})
                elif kind == "agent_message":
                    feedback.append(item.get("text", ""))
            if frame.get("type") == "turn.completed":
                usage.append(frame.get("usage", {}))
        else:
            if frame.get("type") == "system" and frame.get("subtype") == "init":
                available_tools = frame.get("tools")
            if frame.get("type") == "assistant":
                message = frame.get("message", {})
                if isinstance(message.get("usage"), dict):
                    # A single message can be emitted in several content blocks.
                    # Last receipt wins; never double-count a repeated message id.
                    rounds[message.get("id", f"event-{index}")] = {
                        "event_index": index, "usage": message["usage"]}
                for block in blocks(message):
                    if block.get("type") == "tool_use":
                        call_id = block.get("id", f"event-{index}-{len(tools_by_id)}")
                        if call_id in seen_calls: continue
                        seen_calls.add(call_id)
                        name = block.get("name", "")
                        tools_by_id[call_id] = name.removeprefix("mcp__webcodex__")
                        if name == "ToolSearch": discovery.append(name)
                        else: calls.append(name.removeprefix("mcp__webcodex__"))
            if frame.get("type") == "user":
                for block in blocks(frame.get("message", {})):
                    if block.get("type") == "tool_result" and block.get("tool_use_id") not in seen_results:
                        call_id = block.get("tool_use_id")
                        seen_results.add(call_id)
                        results.append({"call_id": call_id,
                                        "tool": tools_by_id.get(call_id, "unknown"),
                                        "returned_text_chars": text_chars(block.get("content")),
                                        "is_error": block.get("is_error", False)})
            if frame.get("type") == "result":
                feedback.append(frame.get("result", ""))
                usage.append(frame.get("usage", {}))
    results.extend({"call_id": call_id, "tool": name, "returned_text_chars": None,
                    "note": "client events omitted the tool response"}
                   for call_id, name in tools_by_id.items() if call_id not in seen_results)
    return {"tool_calls": dict(Counter(calls)), "total_calls": len(calls), "usage": usage,
            "model_round_usage": list(rounds.values()) if client == "claude" else None,
            "model_round_usage_note": "assistant.message.usage; deduplicated by message id; separate from result totals" if client == "claude" else
                "codex exec --json exposes turn.completed totals, not individual model-request usage",
            "tool_results": results, "available_tools": available_tools,
            "native_discovery": dict(Counter(discovery)),
            "native_poll_calls": None if client == "codex" else sum(name == "TaskOutput" for name in calls),
            "call_metric": "completed_action_events; native stdin polls are not separately emitted" if client == "codex" else "tool_use_blocks",
            "feedback": "\n".join(feedback)[-8000:]}
