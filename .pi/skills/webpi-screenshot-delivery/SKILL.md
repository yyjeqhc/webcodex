---
name: webpi-screenshot-delivery
description: Use when capturing, returning, inspecting, or persisting WebPi desktop/browser screenshots. Optimizes clarity, payload size, timeout risk, and long-session image reuse using preview/detail/resource/artifact tiers.
---
# WebPi Screenshot Delivery

Use this skill for screenshot-heavy desktop/browser work, especially when text must remain readable or image responses are timing out.

## Architectural rule

WebPi Computer/Browser owns capture and authority. Do not install or invoke a second screenshot provider when native capture is available.

Treat screenshot delivery as two separate products:

1. **Preview** — bounded synchronous image for the current model turn.
2. **Full detail** — short-lived authenticated resource or explicit project artifact fetched only when needed.

Never solve transport pressure by silently degrading the only copy of the image.

## Default capture strategy

Prefer the smallest authoritative surface:

1. target application/window;
2. region of that window when the question concerns one UI area;
3. full display only when layout across windows/displays matters.

Avoid repeated full-display captures during iterative UI work.

When `computer_observe` supports a bounded preview, accept it for the first pass. If small text, code, diagnostics, or dense tables are unreadable, request detail deliberately instead of repeatedly re-screenshotting the full display.

## Preview vs detail

Use these tiers conceptually:

- **preview**: clear enough for layout, controls, state, and medium-sized text; optimized for low-latency synchronous delivery;
- **detail**: higher resolution or tighter region for small text and pixel-level inspection;
- **full resource**: original/high-quality screenshot retained outside the synchronous JSON/image payload;
- **artifact**: persistent project copy only when evidence must survive the short-lived resource window.

A preview is not evidence that the full screenshot was lost. Check metadata such as full-resource availability, dimensions, byte sizes, hashes, and preview dimensions.

## Transport discipline

Prefer native image/resource framing. Do not paste full screenshot base64 into prose, tool arguments, logs, prompts, or structured summaries.

When a short-lived screenshot resource is available:

- use the bounded inline preview for the current turn;
- read the full resource only if preview detail is insufficient;
- save a project artifact only when the screenshot is needed for durable evidence or later tasks.

If preview generation fails, prefer resource-link-only delivery over falling back to a large inline image that risks Host/WebSocket timeout.

## Clarity rules

For UI screenshots, preserve text edges:

- prefer high JPEG quality plus dimension reduction over very low JPEG quality;
- use high-quality resampling for downscaling;
- do not upscale source images;
- preserve aspect ratio;
- prefer a tight region/detail capture when text is small rather than enlarging a blurry preview.

## Long-session behavior

Do not keep resending unchanged screenshots.

- Reuse stable resource/artifact references when available.
- Capture again only after UI state materially changes.
- When many screenshots accumulate, summarize which image is authoritative and stop carrying obsolete images forward.
- Prefer hashes/generations to identify repeated captures rather than visual guesswork.

## Failure recovery

If screenshot transfer fails:

1. distinguish capture failure from encoding failure from transport timeout;
2. retry with a tighter region or lower dimensions, not lower JPEG quality first;
3. if a full resource/artifact already exists, reuse it instead of recapturing;
4. treat truncated/invalid image JSON as a transport bug, not as permission to repeat side effects;
5. keep screenshot/control operations idempotent and separate from mouse/keyboard effects.

## Verification checklist

Before declaring screenshot transport improved, verify with real images:

- window, region, and full-display paths;
- high-DPI/dense-text screenshot;
- inline preview stays within its byte budget;
- full resource/artifact remains byte-identical to the retained source;
- old clients still accept the output schema;
- caller/resource scope isolation remains enforced;
- repeated screenshot responses do not cause Host timeout;
- diagnostics contain no WARN/ERROR from preview encoding or resource reads.

## External ecosystem guidance

Use third-party Pi image extensions mainly as design references unless they fill a missing primitive. Pi editor/TUI screenshot preview extensions do not replace WebPi Computer delivery. Image-processing MCP servers should not sit on the production screenshot hot path unless their extra process, filesystem, network, and credential authority is justified by a capability WebPi cannot provide natively.
