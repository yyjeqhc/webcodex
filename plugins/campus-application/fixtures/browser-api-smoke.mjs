// Reproducible local smoke through the real Server -> Registry -> Runner Browser route.
// Uses only typed Browser API operations; no Playwright, evaluation, selectors, or private profile.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const [serverUrl, clientId, project, tokenFile] = process.argv.slice(2);
if (!serverUrl || !clientId || !project || !tokenFile) {
  throw new Error("Usage: node fixtures/browser-api-smoke.mjs SERVER_URL CLIENT_ID PROJECT TOKEN_FILE");
}
const endpoint = new URL(serverUrl);
assert(["127.0.0.1", "localhost", "[::1]"].includes(endpoint.hostname), "Use a local validation Server");
const token = (await readFile(tokenFile, "utf8")).trim();
assert(token.length > 0, "A Browser-scoped validation token is required");
const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));
const html = await readFile(new URL("./public/complex-controls.html", import.meta.url));
await mkdir(join(repoRoot, "target"), { recursive: true });
const temporary = await mkdtemp(join(repoRoot, "target/browser-campus-smoke-"));
const resume = join(temporary, "resume-fixture.txt");
await writeFile(resume, "Fictional resume upload fixture. No personal data.\n");
const uploadPath = relative(repoRoot, resume).split(sep).join("/");
const server = createServer((_request, response) => {
  response.writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
  response.end(html);
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
let browserId;
let calls = 0;
let completed = false;
const started = performance.now();

async function call(tool, params, expectFailure = false) {
  calls++;
  const response = await fetch(new URL("/api/tools/call", endpoint), {
    method: "POST",
    headers: { authorization: "Bearer " + token, "content-type": "application/json" },
    body: JSON.stringify({ tool, params }),
    signal: AbortSignal.timeout(140_000),
  });
  const result = await response.json();
  const failed = !response.ok || result.success !== true || result.output?.error != null;
  if (expectFailure) {
    assert(failed, "Expected stale element authority to be rejected");
    assert.equal(result.output?.execution_state, "not_started");
    return result.output;
  }
  if (failed) {
    const code = result.error_code ?? result.output?.error?.kind ?? result.output?.error_code ?? response.status;
    throw new Error(tool + "/" + params.action + " failed: " + code);
  }
  assert.equal(result.output?.execution_state, "completed", tool + "/" + params.action + " certainty");
  return result.output;
}
function field(snapshot, name, action) {
  const matches = snapshot.nodes.filter(node => node.name === name && (!action || node.actions?.includes(action)));
  assert.equal(matches.length, 1, "One semantic field required: " + name);
  if (action) assert(matches[0].element_id, "Fresh admitted identity required: " + name);
  return matches[0];
}
try {
  const targets = await call("observe_browser", { action: "targets", client_id: clientId });
  const target = targets.targets.find(item => item.client_id === clientId);
  assert(target?.connected && target.capabilities.browser_complex_controls);
  assert(target.capabilities.browser_semantic_query && target.capabilities.browser_batch);
  const launched = await call("control_browser", { action: "launch", client_id: clientId });
  browserId = launched.browser_id;
  assert(browserId);
  const page = await call("control_browser", { action: "new_page", client_id: clientId, browser_id: browserId });
  const scope = { client_id: clientId, browser_id: browserId, page_id: page.page_id };
  assert(scope.page_id);
  await call("control_browser", {
    action: "navigate", ...scope,
    url: "http://127.0.0.1:" + server.address().port + "/complex-controls.html",
  });
  const query = () => call("observe_browser", {
    action: "snapshot", ...scope, query: { fields_only: true }, max_nodes: 64,
  });
  const first = await query();
  await call("control_browser", {
    action: "select_choice", ...scope,
    element_id: field(first, "API choice", "select_choice").element_id,
    choice_path: ["Beta"],
  });
  await call("control_browser", {
    action: "set_date", ...scope,
    element_id: field(first, "API date", "set_date").element_id,
    value: "2024-02-29",
  });
  const second = await query();
  assert.equal(field(second, "API choice").value, "Beta");
  assert.equal(field(second, "API date").value, "2024-02-29");
  const operations = [
    { action: "set_value", element_id: field(second, "API text", "set_value").element_id, value: "Fictional applicant" },
    { action: "upload_file", element_id: field(second, "API resume", "upload_file").element_id, project, path: uploadPath },
    { action: "select_choice", element_id: field(second, "API location", "select_choice").element_id, choice_path: ["Province", "City", "District"] },
    { action: "set_date", element_id: field(second, "API date", "set_date").element_id, value: "2026-06-30" },
  ];
  const receipt = await call("control_browser", { action: "batch", ...scope, operations });
  assert.equal(receipt.requested_count, operations.length);
  assert.equal(receipt.completed_count, operations.length);
  assert.equal(receipt.remaining_count, 0);
  assert.equal(receipt.stopped_at_index, undefined);
  assert.equal(receipt.stability?.stable, true);
  const final = await query();
  assert.equal(field(final, "API text").value, "Fictional applicant");
  assert(field(final, "API resume").value?.endsWith("resume-fixture.txt"));
  assert.equal(field(final, "API location").value, "Province / City / District");
  assert.equal(field(final, "API date").value, "2026-06-30");
  assert.deepEqual(JSON.parse(field(final, "API monitor").value), { submits: 0, resets: 0 });
  await call("control_browser", {
    action: "select_choice", ...scope,
    element_id: field(second, "API choice", "select_choice").element_id,
    choice_path: ["Alpha"],
  }, true);
  completed = true;
  console.log(JSON.stringify({
    test: "browser_api_complex_controls", success: true,
    single_actions: ["select_choice", "set_date"], mixed_batch_operations: operations.length,
    verified_fields: 5, semantic_snapshots: 3, stale_rejected_before_effect: true,
    submits: 0, resets: 0, screenshots: 0, pointer_operations: 0,
    api_calls_before_cleanup: calls, elapsed_ms: Math.round(performance.now() - started),
  }));
} finally {
  try {
    if (browserId) await call("control_browser", { action: "close_browser", client_id: clientId, browser_id: browserId });
  } finally {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    await rm(temporary, { recursive: true, force: true });
    if (!completed) console.error("Smoke stopped; no effect is automatically replayed.");
  }
}
