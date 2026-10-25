from __future__ import annotations

import json
import os
from pathlib import Path
import re
import sqlite3
import tempfile
import unittest
from unittest import mock

from scripts import continuation_snapshot as evidence

ROOT = Path(__file__).resolve().parents[2]
GOAL = 'wc_goal_' + 'g' * 16
AGENT = 'wc_dagent_' + 'a' * 16
SESSION = 'wc_sess_' + 's' * 16
ENDPOINT = 'wc_endpoint_' + 'e' * 16
EVENT = 'wc_attention_event_' + 'v' * 16
WAKE = 'wc_wake_' + 'w' * 16
ATTEMPT = 'wc_wake_attempt_' + 't' * 16
OWNER = {'owner_principal_kind': 'account', 'owner_principal_digest': 'wc_commprincipal_' + 'a' * 64}
PRIVATE = 'PRIVATE_CANARY_DO_NOT_EXPORT'


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name).resolve() / 'runtime.sqlite'
        self.db = sqlite3.connect(self.path)
        self.addCleanup(self.db.close)
        # Use the owning Store's real CREATE TABLE declarations, including CHECK
        # constraints, rather than a permissive imitation of the snapshot SQL.
        for source, tables in {
            'goal.rs': ('wc_goals', 'wc_goal_correlations'),
            'communication.rs': ('wc_agent_identities', 'wc_agent_endpoints'),
            'agent_attention.rs': ('wc_agent_attention_events',),
            'agent_wake.rs': ('wc_agent_wakes', 'wc_agent_wake_attempts'),
        }.items():
            text = (ROOT / 'crates/webcodex-store/src' / source).read_text()
            for table in tables:
                sql = re.search(r'CREATE TABLE IF NOT EXISTS ' + table + r' \([\s\S]+?^\s*\);', text, re.M)[0]
                self.db.execute(sql.replace('\\"', '"'))
        self.plan = {'completion_conditions': [PRIVATE], 'progress_summary': PRIVATE,
                     'checkpoint_at_unix_ms': None, 'steps': [
                         {'id': 'first', 'title': PRIVATE, 'status': 'pending', 'updated_at_unix_ms': 100}]}
        self.insert('wc_goals', goal_id=GOAL, **OWNER, title=PRIVATE, objective=PRIVATE,
                    controller_agent_id=AGENT, lifecycle='active', revision=3,
                    created_at_unix_ms=100, updated_at_unix_ms=100, plan_json=json.dumps(self.plan))
        self.insert('wc_agent_identities', agent_id=AGENT, **OWNER, handle=PRIVATE,
                    display_name=PRIVATE, description=PRIVATE, specialty_labels_json=json.dumps([PRIVATE]),
                    profile_revision=1, created_at_unix_ms=100, updated_at_unix_ms=100,
                    current_controller_generation=1)
        self.insert('wc_goal_correlations', goal_id=GOAL, kind='workflow_session', reference_id=SESSION,
                    created_at_unix_ms=100)
        self.endpoint()
        self.event()
        self.wake()

    def insert(self, table, **values):
        self.db.execute('INSERT INTO ' + table + '(' + ','.join(values) + ') VALUES (' +
                        ','.join('?' for _ in values) + ')', tuple(values.values()))
        self.db.commit()

    def update(self, table, **values):
        self.db.execute('UPDATE ' + table + ' SET ' + ','.join(key + '=?' for key in values), tuple(values.values()))
        self.db.commit()

    def endpoint(self, endpoint=ENDPOINT, generation=1, lifecycle='attached'):
        self.insert('wc_agent_endpoints', endpoint_id=endpoint, agent_id=AGENT,
                    attachment_principal_kind=OWNER['owner_principal_kind'],
                    attachment_principal_digest=OWNER['owner_principal_digest'], host=PRIVATE,
                    client_attachment_id=PRIVATE, wake_capable=1,
                    mcp_app_recovery_fingerprint='b' * 64, mcp_app_client_window_key='c' * 64,
                    controller_generation=generation, lifecycle=lifecycle, attached_at_unix_ms=100,
                    last_seen_at_unix_ms=110, lease_expires_at_unix_ms=1000,
                    expired_at_unix_ms=120 if lifecycle == 'expired' else None,
                    detached_at_unix_ms=120 if lifecycle == 'detached' else None)

    def event(self, event=EVENT, goal=GOAL, activity=110):
        self.insert('wc_agent_attention_events', event_id=event, kind='goal_workflow_stalled',
                    **OWNER, target_agent_id=AGENT, goal_id=goal, workflow_session_id=SESSION,
                    observed_window_key=PRIVATE, observation_principal_kind=PRIVATE,
                    observation_principal_id=PRIVATE, last_meaningful_activity_at_ms=activity,
                    last_seen_at_ms=activity + 10, goal_revision=1, created_at_unix_ms=activity + 20)

    def wake(self, wake=WAKE, event=EVENT):
        self.insert('wc_agent_wakes', wake_id=wake, target_agent_id=AGENT, trigger_kind='attention_event',
                    source_event_id=event, state='pending', revision=1,
                    created_at_unix_ms=130, updated_at_unix_ms=130)

    def attempt(self, attempt=ATTEMPT, state='consumed', endpoint=ENDPOINT, generation=1):
        values = dict(attempt_id=attempt, wake_id=WAKE, endpoint_id=endpoint,
                      controller_generation=generation, adapter_kind='mcp_app', state=state,
                      claim_fence_hash=PRIVATE, consume_token_hash=PRIVATE, claimed_at_unix_ms=140,
                      claim_lease_expires_at_unix_ms=300)
        if state in {'prepared', 'delivered', 'delivery_unknown', 'consumed'}:
            values['prepared_at_unix_ms'] = 150
        if state == 'delivered':
            values['delivered_at_unix_ms'] = 160
        if state == 'delivery_unknown':
            values['delivery_unknown_at_unix_ms'] = 160
        if state == 'consumed':
            values['consumed_at_unix_ms'] = 170
        if state == 'revoked':
            values['revoked_at_unix_ms'] = 160
        self.insert('wc_agent_wake_attempts', **values)
        if state != 'revoked':
            wake_values = dict(state=state, revision=2, updated_at_unix_ms=170,
                               claimed_attempt_id=attempt, claimed_endpoint_id=endpoint,
                               claimed_controller_generation=generation,
                               claim_lease_expires_at_unix_ms=300 if state == 'claimed' else None)
            if state == 'consumed':
                wake_values.update(consumed_at_unix_ms=170, consumed_by_endpoint_id=endpoint,
                                   consumed_controller_generation=generation)
            self.update('wc_agent_wakes', **wake_values)

    def read(self, **overrides):
        selectors = dict(goal_id=GOAL, agent_id=AGENT, session_id=SESSION, now_ms=500)
        selectors.update(overrides)
        return evidence.snapshot(self.path, **selectors)

    def assert_closed(self, expected=None):
        with self.assertRaises(evidence.EvidenceError) as caught:
            self.read()
        code = str(caught.exception)
        self.assertRegex(code, r'^[a-z_]+$')
        self.assertNotIn(PRIVATE, code)
        if expected:
            self.assertEqual(expected, code)

    def test_pending_metadata_is_body_free_and_read_only(self):
        before = self.path.read_bytes()
        result = self.read()
        self.assertEqual(before, self.path.read_bytes())
        self.assertEqual(result['db_identity'], [self.path.stat().st_dev, self.path.stat().st_ino])
        self.assertEqual(result['goal']['steps'], [{'id': 'first', 'status': 'pending', 'updated_at_unix_ms': 100}])
        self.assertEqual(result['selectors'], {'goal_id': GOAL, 'agent_id': AGENT, 'session_id': SESSION})
        text = json.dumps(result)
        for forbidden in (PRIVATE, 'claim_fence', 'consume_token', 'principal', 'title', 'summary', 'description'):
            self.assertNotIn(forbidden, text)

    def test_never_selects_sensitive_columns(self):
        real_connect = sqlite3.connect
        denied_columns = {'title', 'objective', 'terminal_reason', 'handle', 'display_name', 'description',
                          'specialty_labels_json', 'host', 'client_attachment_id', 'mcp_app_recovery_fingerprint',
                          'mcp_app_client_window_key', 'claim_fence_hash', 'consume_token_hash',
                          'observed_window_key', 'observation_principal_kind', 'observation_principal_id'}
        def connect(*args, **kwargs):
            self.assertTrue(kwargs['uri'])
            self.assertTrue(args[0].endswith('?mode=ro'))
            connection = real_connect(*args, **kwargs)
            def authorize(action, table, column, database, source):
                if action == sqlite3.SQLITE_READ and column in denied_columns:
                    return sqlite3.SQLITE_DENY
                return sqlite3.SQLITE_OK
            connection.set_authorizer(authorize)
            return connection
        with mock.patch.object(evidence.sqlite3, 'connect', side_effect=connect):
            self.read()

    def test_completed_goal_and_consumed_wake(self):
        self.attempt()
        self.plan['steps'][0]['status'] = 'completed'
        self.plan['checkpoint_at_unix_ms'] = 180
        self.update('wc_goals', lifecycle='completed', terminal_at_unix_ms=190,
                    updated_at_unix_ms=190, plan_json=json.dumps(self.plan))
        result = self.read()
        self.assertEqual(result['goal']['lifecycle'], 'completed')
        self.assertEqual(result['wakes'][0]['state'], 'consumed')
        self.assertEqual(result['attempts'][0]['consumed_at_unix_ms'], 170)

    def test_old_consumed_generation_is_valid_after_replacement(self):
        self.attempt()
        self.update('wc_agent_endpoints', lifecycle='expired', expired_at_unix_ms=200)
        self.update('wc_agent_identities', current_controller_generation=2)
        self.endpoint('wc_endpoint_' + 'n' * 16, 2)
        result = self.read()
        self.assertEqual([row['controller_generation'] for row in result['endpoints']], [1, 2])
        self.assertEqual(result['wakes'][0]['consumed_controller_generation'], 1)

    def test_pre_dispatch_recovery_retains_revoked_attempt(self):
        self.attempt(state='revoked')
        self.update('wc_agent_endpoints', lifecycle='detached', detached_at_unix_ms=200)
        self.update('wc_agent_identities', current_controller_generation=2)
        successor = 'wc_endpoint_' + 'n' * 16
        self.endpoint(successor, 2)
        self.attempt('wc_wake_attempt_' + 'u' * 16, 'delivered', successor, 2)
        result = self.read()
        self.assertEqual([row['state'] for row in result['attempts']], ['revoked', 'delivered'])

    def test_prepared_and_unknown_are_valid_metadata(self):
        for state in ('claimed', 'prepared', 'delivered', 'delivery_unknown'):
            with self.subTest(state=state):
                self.db.execute('DELETE FROM wc_agent_wake_attempts')
                self.db.commit()
                self.attempt(state=state)
                self.assertEqual(self.read()['wakes'][0]['state'], state)

    def test_owner_and_controller_mismatch(self):
        self.update('wc_agent_identities', owner_principal_digest='wc_commprincipal_' + 'b' * 64)
        self.assert_closed('owner_mismatch')
        self.update('wc_agent_identities', **OWNER)
        self.update('wc_goals', controller_agent_id='wc_dagent_' + 'x' * 16)
        self.assert_closed('controller_mismatch')

    def test_endpoint_and_attention_owner_mismatch(self):
        self.update('wc_agent_endpoints', attachment_principal_kind='shared_key')
        self.assert_closed('owner_mismatch')
        self.update('wc_agent_endpoints', attachment_principal_kind='account')
        self.update('wc_agent_attention_events', owner_principal_digest='wc_commprincipal_' + 'b' * 64)
        self.assert_closed('owner_mismatch')

    def test_session_missing_and_both_directions_ambiguous(self):
        self.db.execute('DELETE FROM wc_goal_correlations')
        self.db.commit()
        self.assert_closed('session_correlation_not_unique')
        for goal, session in ((GOAL, 'wc_sess_' + 'x' * 16), ('wc_goal_' + 'x' * 16, SESSION)):
            with self.subTest(goal=goal):
                self.db.execute('DELETE FROM wc_goal_correlations')
                self.db.commit()
                self.insert('wc_goal_correlations', goal_id=GOAL, kind='workflow_session', reference_id=SESSION, created_at_unix_ms=100)
                self.insert('wc_goal_correlations', goal_id=goal, kind='workflow_session', reference_id=session, created_at_unix_ms=100)
                self.assert_closed('session_correlation_not_unique')

    def test_attention_binding_mismatch(self):
        for field, value in (('workflow_session_id', 'wc_sess_' + 'x' * 16),
                             ('target_agent_id', 'wc_dagent_' + 'x' * 16)):
            with self.subTest(field=field):
                self.update('wc_agent_attention_events', **{field: value})
                self.assert_closed('attention_identity_mismatch')
                self.update('wc_agent_attention_events', workflow_session_id=SESSION, target_agent_id=AGENT)

    def test_other_goal_and_inbox_wakes_are_not_attributed(self):
        other_event = 'wc_attention_event_' + 'o' * 16
        other_wake = 'wc_wake_' + 'o' * 16
        self.event(other_event, 'wc_goal_' + 'o' * 16, 120)
        self.wake(other_wake, other_event)
        self.insert('wc_agent_wakes', wake_id='wc_wake_' + 'i' * 16, target_agent_id=AGENT,
                    trigger_kind='inbox_changed', first_triggering_delivery_id=PRIVATE,
                    latest_triggering_delivery_id=PRIVATE, latest_conversation_id=PRIVATE,
                    latest_message_id=PRIVATE, inbox_high_watermark=1, queued_delivery_count_snapshot=1,
                    state='pending', revision=1, created_at_unix_ms=130, updated_at_unix_ms=130)
        self.assertEqual([row['wake_id'] for row in self.read()['wakes']], [WAKE])

    def test_wake_target_mismatch(self):
        self.update('wc_agent_wakes', target_agent_id='wc_dagent_' + 'x' * 16)
        self.assert_closed('wake_identity_mismatch')

    def test_consume_identity_and_generation_mismatch(self):
        self.attempt()
        self.update('wc_agent_wakes', consumed_controller_generation=2)
        self.assert_closed('consume_identity_mismatch')
        self.update('wc_agent_wakes', consumed_controller_generation=1)
        self.update('wc_agent_wake_attempts', controller_generation=2)
        self.assert_closed('generation_mismatch')

    def test_claim_identity_and_fence_mismatch(self):
        self.attempt(state='prepared')
        self.update('wc_agent_wakes', claim_lease_expires_at_unix_ms=301)
        self.assert_closed('wake_fence_mismatch')
        self.update('wc_agent_wakes', claim_lease_expires_at_unix_ms=None)
        self.update('wc_agent_wake_attempts', prepared_at_unix_ms=139)
        self.assert_closed('attempt_fence_mismatch')

    def test_consumed_without_dispatch_metadata_fails_closed(self):
        self.attempt()
        self.update('wc_agent_wake_attempts', prepared_at_unix_ms=None)
        self.assert_closed('attempt_fence_mismatch')

    def test_no_stall_yet_is_valid_and_missing_atomic_wake_is_closed(self):
        self.db.execute('DELETE FROM wc_agent_wakes')
        self.db.commit()
        self.assert_closed('attention_wake_missing')
        self.db.execute('DELETE FROM wc_agent_attention_events')
        self.db.commit()
        result = self.read()
        self.assertEqual(result['attention'], [])
        self.assertEqual(result['wakes'], [])
        self.assertEqual(result['attempts'], [])

    def test_recovery_preserves_post_dispatch_unknown_old_generation(self):
        self.attempt(state='delivery_unknown')
        self.update('wc_agent_endpoints', lifecycle='expired', expired_at_unix_ms=200)
        self.update('wc_agent_identities', current_controller_generation=2)
        self.endpoint('wc_endpoint_' + 'n' * 16, 2)
        self.assertEqual(self.read()['attempts'][0]['controller_generation'], 1)

    def test_generation_and_current_endpoint_uniqueness(self):
        self.endpoint('wc_endpoint_' + 'n' * 16)
        self.assert_closed('endpoint_not_unique')
        self.db.execute('DELETE FROM wc_agent_endpoints WHERE endpoint_id=?', ('wc_endpoint_' + 'n' * 16,))
        self.db.commit()
        self.update('wc_agent_identities', current_controller_generation=2)
        self.assert_closed('generation_mismatch')

    def test_invalid_persisted_numeric_and_state_metadata(self):
        # These defects bypass the Store and simulate manual database damage.
        self.db.execute('PRAGMA ignore_check_constraints=ON')
        self.update('wc_agent_identities', current_controller_generation='private-' + PRIVATE)
        self.assert_closed()
        self.update('wc_agent_identities', current_controller_generation=1)
        self.update('wc_agent_wakes', state=PRIVATE)
        self.assert_closed()

    def test_claim_metadata_cannot_claim_another_attempt(self):
        self.attempt(state='claimed')
        self.update('wc_agent_wakes', claimed_attempt_id='wc_wake_attempt_' + 'x' * 16)
        self.assert_closed('wake_fence_mismatch')

    def test_no_write_allowed_by_opened_connection(self):
        real_read = evidence._read
        def read(connection, *args):
            self.assertEqual(connection.execute('PRAGMA query_only').fetchone()[0], 1)
            with self.assertRaises(sqlite3.OperationalError):
                connection.execute('UPDATE wc_goals SET revision=99')
            return real_read(connection, *args)
        with mock.patch.object(evidence, '_read', side_effect=read):
            self.read()
        self.assertEqual(self.db.execute('SELECT revision FROM wc_goals').fetchone()[0], 3)

    def test_symlinked_database_parent_is_rejected(self):
        link = self.path.parent.with_name(self.path.parent.name + '-link')
        link.symlink_to(self.path.parent, target_is_directory=True)
        self.addCleanup(link.unlink)
        self.path = link / self.path.name
        self.assert_closed('unsafe_database_file')

    def test_wal_database_reads_committed_metadata(self):
        self.db.execute('PRAGMA journal_mode=WAL')
        self.update('wc_goals', revision=4)
        self.assertEqual(self.read()['goal']['revision'], 4)

    def test_missing_and_substituted_schema_fail_closed(self):
        self.db.execute('DROP TABLE wc_agent_wake_attempts')
        self.db.commit()
        self.assert_closed('unsupported_schema')
        self.db.execute('CREATE VIEW wc_agent_wake_attempts AS SELECT 1 AS attempt_id')
        self.db.commit()
        self.assert_closed('unsupported_schema')

    def test_missing_required_column_fails_closed(self):
        self.db.execute('ALTER TABLE wc_agent_wake_attempts RENAME COLUMN prepared_at_unix_ms TO unrelated')
        self.db.commit()
        self.assert_closed('database_or_schema_unavailable')

    def test_step_and_plan_limits(self):
        self.plan['steps'] *= 65
        self.update('wc_goals', plan_json=json.dumps(self.plan))
        self.assert_closed('step_limit')
        self.plan['steps'] = []
        self.plan['progress_summary'] = 'p' * (256 * 1024)
        self.update('wc_goals', plan_json=json.dumps(self.plan))
        self.assert_closed()

    def test_json_bool_is_not_integer_metadata(self):
        for field in ('checkpoint_at_unix_ms', 'step_timestamp'):
            with self.subTest(field=field):
                self.plan['checkpoint_at_unix_ms'] = True if field == 'checkpoint_at_unix_ms' else None
                self.plan['steps'][0]['updated_at_unix_ms'] = True if field == 'step_timestamp' else 100
                self.update('wc_goals', plan_json=json.dumps(self.plan))
                self.assert_closed()
        with self.assertRaises(evidence.EvidenceError):
            self.read(now_ms=True)

    def test_each_collection_limit_fails_instead_of_truncating(self):
        for table in ('attention', 'wakes', 'attempts', 'endpoints'):
            with self.subTest(table=table):
                # Each case owns a fresh real database fixture.
                fixture = SnapshotTests()
                fixture.setUp()
                try:
                    if table == 'attention':
                        for index in range(128):
                            fixture.event('wc_attention_event_' + f'{index:016d}', activity=200 + index)
                    elif table == 'wakes':
                        # Deliberately corrupt the unique wake/event relation so
                        # the count, rather than identity validation, catches it.
                        for index in range(128):
                            fixture.wake('wc_wake_' + f'{index:016d}')
                    elif table == 'attempts':
                        for index in range(129):
                            fixture.attempt('wc_wake_attempt_' + f'{index:016d}', 'revoked')
                    else:
                        for index in range(128):
                            fixture.endpoint('wc_endpoint_' + f'{index:016d}', lifecycle='detached')
                    fixture.assert_closed('row_limit')
                finally:
                    fixture.doCleanups()

    def test_output_limit_is_closed(self):
        with mock.patch.object(evidence, 'MAX_OUTPUT_BYTES', 1):
            self.assert_closed('output_limit')

    def test_query_budget_is_closed(self):
        with mock.patch.object(evidence, 'QUERY_SECONDS', 0):
            self.assert_closed('query_budget_exceeded')

    def test_symlink_fifo_and_foreign_uid_rejected(self):
        original = self.path
        link = original.with_name('link.sqlite')
        link.symlink_to(original)
        self.path = link
        self.assert_closed('unsafe_database_file')
        self.path = original.with_name('fifo.sqlite')
        os.mkfifo(self.path)
        self.assert_closed('unsafe_database_file')
        self.path = original
        with mock.patch.object(evidence.os, 'getuid', return_value=os.getuid() + 1):
            self.assert_closed('unsafe_database_file')

    def test_database_inode_replacement_detected(self):
        original_identity = evidence._file_identity
        count = 0
        def identity(path):
            nonlocal count
            count += 1
            if count == 2:
                replacement = path.with_name('replacement.sqlite')
                replacement.write_bytes(path.read_bytes())
                os.replace(replacement, path)
            return original_identity(path)
        with mock.patch.object(evidence, '_file_identity', side_effect=identity):
            self.assert_closed('database_replaced')

    def test_missing_file_and_invalid_selector_do_not_leak(self):
        self.path = self.path.with_name(PRIVATE)
        self.assert_closed('database_unavailable_or_invalid')
        with self.assertRaises(evidence.EvidenceError) as caught:
            self.read(goal_id=PRIVATE)
        self.assertNotIn(PRIVATE, str(caught.exception))


if __name__ == '__main__':
    unittest.main()
