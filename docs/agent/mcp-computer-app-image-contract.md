# MCP Computer App image-result contract

This note records the durable findings from the August 2026 Computer App gray-card investigation. It is intentionally short; the temporary diagnostic tools used during the investigation were removed after their results were captured.

## Observed failure

`computer_snapshot` could return a successful native MCP image while the bound Computer App card rendered as an empty gray surface. A successful `resources/read` response alone did not prove that the host had accepted the template or delivered the tool result to the App.

## Decisive controls

The investigation progressively held the App/resource/result shape constant while varying one image property at a time:

- tiny text results rendered, proving the basic MCP App resource, initialize, and tool-result path;
- a 68-byte native PNG rendered, proving native image ContentBlocks could cross the host/App bridge;
- synthetic native images rendered from 1 KiB through 512 KiB;
- fixed-size synthetic images decoded successfully through 3840x2160 intrinsic dimensions;
- synthetic 4K JPEGs decoded successfully at 64, 256, and 512 KiB;
- a real Runner JPEG (3840x1912, 419,991 bytes in the confirming run) also decoded normally once the descriptor contract was corrected.

These controls ruled out JPEG as a type, 4K dimensions, ordinary native-image framing, and payload sizes through 512 KiB as the cause of the observed gray card.

## Confirmed contract bug and permanent invariant

The generic ToolRuntime `computer_snapshot` output contains `content_base64`. MCP native-image framing deliberately removes that field from `structuredContent.output`, adds `content_delivery = "mcp_image"`, and carries the binary bytes in an MCP image ContentBlock.

The gray-card build advertised the generic **pre-framing** output schema in MCP `tools/list`, even though MCP returned the **post-framing** structured result. This was a real transport-contract bug and was corrected by making the MCP-facing descriptor omit `content_base64` and declare `content_delivery = "mcp_image"` while leaving the generic ToolRuntime/API schema unchanged.

That correction materially improved the observed behavior, but it is not proven to be the sole gray-card cause. Later same-session production smoke tests showed intermittent gray cards even though the corresponding `computer_snapshot` calls completed successfully on the server. Reusing the exact same `surface_id` could render normally on the next call, and the App does not consume `surface_id` at all. A normal card visibly paints the small static HTML first and expands only after the image arrives; a failed card can appear at full card size as a blank gray surface from its first frame. This moves the remaining hypothesis earlier than image decode toward host App-card/iframe instantiation, first paint, binding, or resource lifecycle.

**Invariant:** a transport adapter that rewrites structured output must advertise the post-adapter schema on that transport. Do not reuse a pre-adapter ToolRuntime schema when the fields actually delivered to the MCP client differ. Keep the generic ToolRuntime/API schema unchanged when only the MCP representation changes.

The regression in `src/mcp_tests.rs` intentionally asserts both sides of this boundary: the runtime schema retains `content_base64`, while the MCP-facing `computer_snapshot` schema exposes `content_delivery = "mcp_image"` instead. Resource identity bumps alone did not eliminate the intermittent gray card. The current diagnostic canonical resource is `v11` with `ttlMs = 0`, while prior versions remain hidden zero-TTL read aliases. Live ActionAudit on 2026-08-16 showed that `ttlMs = 0` is only cache guidance: after the last recorded v11 `resources/read`, fifteen subsequent successful snapshots—including calls against different windows and `surface_id` values—rendered normally without another resource read. Template/resource reuse is therefore compatible with successful rendering and is not by itself the gray-card cause. Computer App `resources/read` outcomes are recorded in ActionAudit as bounded metadata only (`resource_uri`/version, protocol era, UI-capability presence, HTTP status, and MCP error code); HTML, screenshot bytes, tool arguments, window titles, and result content are never persisted by this diagnostic projection.

## Separate macOS lock-screen observation

During the same investigation, a locked Mac produced `image_too_large: cannot establish a bounded macOS capture scale`. After unlock, the same Edge window captured normally at the expected Retina 2x scale (1920x956 logical to 3840x1912 backing pixels). Treat this as a separate capture-scale/display-state issue, not as evidence of an MCP App gray-card failure. The current fail-closed behavior is safer than guessing a scale; error classification/message cleanup can be handled independently.
