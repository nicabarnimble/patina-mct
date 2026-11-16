import test from 'node:test';
import assert from 'node:assert/strict';
import { assertDriftJobResult, assertDriftLedgers, compareDependencyInventories } from './drift-report.mjs';

function report(sha, packages) {
  return {
    schema: 'mct-repo-report/v1',
    sha256: sha,
    package_count: Object.values(packages).reduce((count, versions) => count + versions.length, 0),
    registry_count: 0,
    git_count: 0,
    local_count: 0,
    packages,
    duplicates: {},
    selected: {},
  };
}

test('dependency comparison reports only-left, only-right and version drift in package order', () => {
  const left = { node_id: 'mother-a', report: report('a'.repeat(64), { alpha: ['1'], shared: ['1', '2'], stable: ['3'] }) };
  const right = { node_id: 'mother-b', report: report('b'.repeat(64), { beta: ['4'], shared: ['2', '3'], stable: ['3'] }) };
  const drift = compareDependencyInventories(left, right);
  assert.equal(drift.schema, 'mct-dependency-drift/v1');
  assert.equal(drift.left_mother.node_id, 'mother-a');
  assert.equal(drift.right_mother.node_id, 'mother-b');
  assert.equal(drift.difference_count, 3);
  assert.deepEqual(drift.differences, [
    { package: 'alpha', kind: 'only_left', left_versions: ['1'], right_versions: [] },
    { package: 'beta', kind: 'only_right', left_versions: [], right_versions: ['4'] },
    { package: 'shared', kind: 'version_mismatch', left_versions: ['1', '2'], right_versions: ['2', '3'] },
  ]);
});

function encoded(value) { return Buffer.from(JSON.stringify(value)).toString('base64'); }

function completedPiece(piece_id, call_id, route_taken, inventory) {
  const decoded_result = { results: [{ ok: JSON.stringify(inventory) }] };
  return {
    piece_id,
    call_id,
    outcome: 'completed',
    protocol_reason: null,
    safe_message: 'completed',
    result_ref: `result-${call_id}`,
    result_payload: {},
    route_decision_id: `decision-${call_id}`,
    route_taken,
    inline_result_payload_base64: encoded(decoded_result),
    decoded_result,
  };
}

test('drift job result requires the same Child on exact A and B routes', () => {
  const jobId = 'nic105-job';
  const a = { node: 'mother-a' };
  const b = { node: 'mother-b' };
  const left = report('a'.repeat(64), { alpha: ['1'] });
  const right = report('b'.repeat(64), { alpha: ['2'] });
  const reply = {
    job_id: jobId,
    outcome: 'completed',
    safe_message: 'mesh job completed',
    pieces: [
      completedPiece('mother-a', `${jobId}:mother-a`, { node_id: a.node, runtime_kind: 'wasm_component' }, left),
      completedPiece('mother-b', `${jobId}:mother-b`, { node_id: b.node, runtime_kind: 'remote_peer' }, right),
    ],
  };
  assert.deepEqual(assertDriftJobResult(reply, jobId, a, b, [left.sha256, right.sha256]).reports, [left, right]);
  const wrongRoute = structuredClone(reply);
  wrongRoute.pieces[1].route_taken.node_id = a.node;
  assert.throws(() => assertDriftJobResult(wrongRoute, jobId, a, b, [left.sha256, right.sha256]));
});

test('drift ledger proof isolates both inventories and requires B expired-grant denial', () => {
  const jobId = 'nic105-job';
  const left = `${jobId}:mother-a`, right = `${jobId}:mother-b`, uncovered = `${jobId}-expired-b`;
  const a = { node: 'mother-a', checkpoint: { observations: [
    { call_id: jobId, kind: 'result_recorded', outcome: 'completed', detail_ref: 'local_mesh_job_result:completed=2;denied=0;failed=0' },
    { call_id: left, kind: 'runtime_execution_started', provenance: {} },
    { call_id: left, kind: 'runtime_execution_completed', provenance: { executed_on: 'mother-a' } },
    { call_id: right, kind: 'peer_call_sent' },
  ] } };
  const b = { node: 'mother-b', checkpoint: { observations: [
    { call_id: 'receiver-right', kind: 'call_constructed', peer_correlation: { caller_call_id: right } },
    { call_id: 'receiver-right', kind: 'runtime_execution_started', provenance: {} },
    { call_id: 'receiver-right', kind: 'runtime_execution_completed', provenance: { executed_on: 'mother-b' } },
    { call_id: 'receiver-expired', kind: 'call_constructed', peer_correlation: { caller_call_id: uncovered } },
    { call_id: 'receiver-expired', kind: 'toy_grant_expired', outcome: 'denied', detail_ref: 'toy_grant_reason:ExpiredGrant' },
  ] } };
  const facts = assertDriftLedgers({ a, b, jobId, uncoveredCallId: uncovered });
  assert.equal(facts.mother_b_execution_id, 'receiver-right');
  assert.equal(facts.uncovered_receiver_call_id, 'receiver-expired');
  b.checkpoint.observations.push({ call_id: 'receiver-expired', kind: 'runtime_execution_started', provenance: {} });
  assert.throws(() => assertDriftLedgers({ a, b, jobId, uncoveredCallId: uncovered }));
});

test('dependency comparison rejects malformed or unsorted inventories', () => {
  const valid = { node_id: 'mother-a', report: report('a'.repeat(64), { alpha: ['1'] }) };
  assert.throws(() => compareDependencyInventories(valid, { node_id: 'mother-b', report: report('b'.repeat(64), { alpha: ['2', '1'] }) }));
  assert.throws(() => compareDependencyInventories(valid, { node_id: 'mother-b', report: { ...report('b'.repeat(64), {}), schema: 'other' } }));
  assert.throws(() => compareDependencyInventories(valid, { node_id: 'mother-b', report: report('b'.repeat(64), { alpha: ['1', '1'] }) }));
});
