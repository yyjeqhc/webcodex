// Run the documented Host Code Mode snippet against controlled tool results.
// This verifies a suggested Host projection, not arbitrary Host compliance.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { Script } from "node:vm";
import { test } from "node:test";

const docs = readFileSync(
  new URL("../../docs/agent/host-code-mode-sidecar-consumption.md", import.meta.url),
  "utf8",
);
const example = docs.match(/```javascript\n([\s\S]*?)\n```/);
assert.ok(example, "Host projection example must remain executable");
const script = new Script("(async () => {\n" + example[1] + "\n})()", {
  filename: "host-code-mode-sidecar-example",
});

async function project(output, success = true, error = null) {
  const printed = [];
  const tools = {
    mcp__webcodex__get_runtime_status: async () => ({
      structuredContent: { success, output, error },
    }),
  };
  await script.runInNewContext({
    tools,
    text: (line) => printed.push(JSON.parse(line)),
  });
  return printed;
}

test("forward all nonempty attention before compact business output", async () => {
  const printed = await project({
    service: "webcodex",
    version: "test",
    session_attention: { messages: [{ message_id: "A", message: "marker-A" }] },
    operator_messages: { messages: [{ message_id: "O", message: "marker-O" }] },
    peer_messages: { messages: [{ message_id: "B", message: "marker-B" }] },
    job_attention: { items: [{ job_id: "J", outcome: "passed" }] },
    context_projection: { materials: [{ key: "project.instructions" }] },
    peer_awareness: { self_peer_id: "wc_peer_self", new_peers: [] },
  });
  assert.deepEqual(
    printed.map((x) => x.kind + ":" + (x.channel ?? "main")),
    [
      "webcodex_attention:session_attention",
      "webcodex_attention:operator_messages",
      "webcodex_attention:peer_messages",
      "webcodex_attention:job_attention",
      "webcodex_attention:context_projection",
      "webcodex_attention:peer_awareness",
      "business:main",
    ],
  );
  assert.equal(printed[0].value.messages[0].message_id, "A");
  assert.equal(printed[1].value.messages[0].message_id, "O");
  assert.equal(printed[2].value.messages[0].message_id, "B");
  assert.equal(printed[3].value.items[0].job_id, "J");
});

test("preserve ACK-only receipts and omitted-message evidence", async () => {
  const printed = await project({
    session_attention: {
      messages: [],
      omitted_count: 2,
      truncated: true,
      ack: { accepted_count: 0 },
    },
    operator_messages: { messages: [], ack: { accepted_ids: ["A"] } },
    peer_messages: { messages: [], ack: { accepted_count: 1 } },
  });
  assert.deepEqual(printed.slice(0, 3).map((x) => x.channel), [
    "session_attention", "operator_messages", "peer_messages",
  ]);
  assert.equal(printed[0].value.omitted_count, 2);
  assert.equal(printed[1].value.ack.accepted_ids[0], "A");
});

test("preserve attention when the main business call fails", async () => {
  const printed = await project(
    { job_attention: { items: [{ job_id: "J", state: "terminal" }] } },
    false,
    "business failed",
  );
  assert.equal(printed[0].channel, "job_attention");
  assert.equal(printed.at(-1).kind, "business");
  assert.equal(printed.at(-1).success, false);
  assert.equal(printed.at(-1).error, "business failed");
});

test("skip empty attention without hiding normal business output", async () => {
  const printed = await project({
    service: "webcodex",
    version: "test",
    session_attention: { messages: [], ack: { accepted_count: 0 } },
    peer_messages: { messages: [] },
    job_attention: { items: [] },
  });
  assert.equal(printed.length, 1);
  assert.equal(printed[0].kind, "business");
  assert.equal(printed[0].service, "webcodex");
});
