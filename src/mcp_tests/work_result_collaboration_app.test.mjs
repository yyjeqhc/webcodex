import test from 'node:test';
import assert from 'node:assert/strict';
import { app, flush, toolResult } from './app_test_support.mjs';
import { baseState } from './work_result_app_fixture.mjs';
const scope = 'a'.repeat(64);
const row = (id, at, message = id) => ({ message_id: 'wc_msg_' + id, created_at_ms: at, message,
  source: 'operator', direction: 'inbound', kind: 'question', priority: 'normal', requires_ack: true,
  first_projected_at_ms: null, first_ack_observed_at_ms: null });
const page = (rows, more = false) => ({ available: true, can_send: true, history_scope: scope,
  messages: rows, truncated: more, next_before: more ? rows[0].message_id : null });
async function setup() {
  const view = app('mcp_work_result_app.html');
  view.toolResult({ work_result: { ...baseState, collaboration: page([row('current', 200)], true) } });
  await view.initialize(); return view;
}
test('disjoint head refresh preserves the missing-page boundary', async () => {
  const view = await setup();
  view.toolResult({work_result: {...baseState, state_version: 'wr2_' + 'c'.repeat(64), collaboration: page([row('jump', 500)], true)}});
  view.nodes.historyOlder.onclick(); await flush();
  const call = view.calls('get_work_result_state').at(-1);
  assert.equal(call.params.arguments.collaboration.before_message_id, 'wc_msg_jump');
  await view.reply(call, toolResult({work_result_collaboration: page([row('gap', 400)], true)}));
  view.nodes.historyOlder.onclick(); await flush();
  assert.equal(view.calls('get_work_result_state').at(-1).params.arguments.collaboration.before_message_id, 'wc_msg_gap');
});
test('older history is explicit, scoped, and not replaced by a later activity snapshot', async () => {
  const view = await setup();
  view.nodes.historyOlder.onclick(); await flush();
  const call = view.calls('get_work_result_state').at(-1);
  assert.deepEqual(JSON.parse(JSON.stringify(call.params.arguments.collaboration)), { limit: 50, before_message_id: 'wc_msg_current' });
  assert.equal(call.params.arguments.session_id, undefined, 'Window history must not depend on a linked Session lifetime');
  await view.reply(call, toolResult({ work_result_collaboration: page([row('old', 100)]) }));
  assert.equal(view.nodes.messages.children.length, 2);
  view.toolResult({ work_result: { ...baseState, state_version: 'wr2_' + 'b'.repeat(64), collaboration: page([row('new', 300)], true) } });
  assert.equal(view.nodes.messages.children[0].getAttribute('data-message-id'), 'wc_msg_old');
  assert.equal(view.nodes.messages.children.length, 2);
  view.nodes.historyLatest.onclick(); await flush();
  await view.reply(view.calls('get_work_result_state').at(-1), toolResult({ work_result_collaboration: page([row('new', 300)]) }));
  assert.equal(view.nodes.messages.children.length, 1);
  assert.equal(view.nodes.messages.children[0].getAttribute('data-message-id'), 'wc_msg_new');
});
test('ACK changes and appended messages preserve the original article and body nodes', async () => {
  const view = await setup(), article = view.nodes.messages.children[0], body = article.children[2];
  view.toolResult({ work_result: { ...baseState, state_version: 'wr2_' + 'b'.repeat(64), collaboration: page([
    {...row('current', 200), first_projected_at_ms: 210, first_ack_observed_at_ms: 220}, row('new', 300),
  ], true) } });
  assert.equal(view.nodes.messages.children[0], article);
  assert.equal(article.children[2], body);
  assert(article.children.at(-1).children.some(item => item.textContent === 'Acknowledged'));
});
test('late older history cannot repopulate a revoked conversation', async () => {
  const view = await setup(); view.nodes.historyOlder.onclick(); await flush();
  const call = view.calls('get_work_result_state').at(-1);
  view.toolResult({ work_result: { ...baseState, state_version: 'wr2_' + 'b'.repeat(64),
    collaboration: {available: false, can_send: false, messages: []} } });
  await view.reply(call, toolResult({ work_result_collaboration: page([row('private', 100)]) }));
  assert.equal(view.nodes.messages.children[0].textContent, 'Collaboration unavailable');
  assert.equal(view.nodes.messageInput.disabled, true);
});
test('intent and uncertain delivery are fenced; IME Enter never submits', async () => {
  const view = await setup();
  view.nodes.messageInput.value = 'Please verify'; view.nodes.messageKind.value = 'question';
  view.nodes.messageInput.onkeydown({key: 'Enter', ctrlKey: true, isComposing: true, preventDefault() {}});
  assert.equal(view.calls('send_work_result_message').length, 0);
  view.nodes.composer.onsubmit({preventDefault() {}}); await flush();
  const call = view.calls('send_work_result_message')[0]; assert.equal(call.params.arguments.kind, 'question');
  await view.reject(call); assert.equal(view.nodes.messageKind.disabled, true);
  view.nodes.composer.onsubmit({preventDefault() {}}); await flush();
  assert.deepEqual(view.calls('send_work_result_message')[1].params.arguments, call.params.arguments);
});
test('history DOM remains bounded while all older pages remain navigable', async () => {
  const view = await setup();
  for (let p = 0; p < 6; p++) {
    view.nodes.historyOlder.onclick(); await flush();
    const rows = Array.from({length: 50}, (_, i) => row(`page${p}_${i}`, 100 - p * 10 + i / 100));
    // Wire timestamps are integers; ties exercise ID sorting as well.
    rows.forEach(item => item.created_at_ms = Math.floor(item.created_at_ms));
    await view.reply(view.calls('get_work_result_state').at(-1), toolResult({work_result_collaboration: page(rows, true)}));
    assert(view.nodes.messages.children.length <= 200);
  }
  assert.equal(view.nodes.historyOlder.hidden, false);
  assert.equal(view.nodes.historyLatest.hidden, false);
});
