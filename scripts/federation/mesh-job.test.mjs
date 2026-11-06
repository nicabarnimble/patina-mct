import test from 'node:test';
import assert from 'node:assert/strict';
import { assertMeshJobLedgers, assertMeshJobResult } from './mesh-job.mjs';

const a = { node: 'a' };
const b = { node: 'b' };
const inputSha = 'a'.repeat(64);
const jobId = 'nic63-job';

function encoded(value) { return Buffer.from(JSON.stringify(value)).toString('base64'); }

function completedPiece(piece_id, call_id, route_taken, decoded_result) {
  return { piece_id, call_id, outcome: 'completed', protocol_reason: null, safe_message: 'completed', result_ref: `result-${call_id}`, result_payload: {}, route_decision_id: `decision-${call_id}`, route_taken, inline_result_payload_base64: encoded(decoded_result), decoded_result };
}

test('mesh job result requires one local and one delegated completed piece', () => {
  const report = { schema: 'mct-repo-report/v1', sha256: inputSha, package_count: 1 };
  const reply = { job_id: jobId, outcome: 'completed', safe_message: 'mesh job completed', pieces: [
    completedPiece('local', `${jobId}:local`, { node_id: 'a', runtime_kind: 'wasm_component' }, { results: [41] }),
    completedPiece('delegated', `${jobId}:delegated`, { node_id: 'b', runtime_kind: 'remote_peer' }, { results: [{ ok: JSON.stringify(report) }] }),
  ] };
  assert.deepEqual(assertMeshJobResult(reply, jobId, a, b, inputSha).report, report);
  assert.throws(() => assertMeshJobResult({ ...reply, outcome: 'denied' }, jobId, a, b, inputSha));
  assert.throws(() => assertMeshJobResult({ ...reply, pieces: [reply.pieces[1], reply.pieces[0]] }, jobId, a, b, inputSha));
});

test('mesh ledger proof isolates execution and requires exact expired-grant refusal', () => {
  const local = `${jobId}:local`, delegated = `${jobId}:delegated`, uncovered = `${jobId}-uncovered`;
  a.checkpoint = { observations: [
    { call_id: jobId, kind: 'result_recorded', outcome: 'completed', detail_ref: 'local_mesh_job_result:completed=2;denied=0;failed=0' },
    { call_id: local, kind: 'runtime_execution_started', provenance: {} },
    { call_id: local, kind: 'runtime_execution_completed', provenance: {} },
    { call_id: local, kind: 'runtime_execution_completed', provenance: { executed_on: 'a' } },
    { call_id: delegated, kind: 'peer_call_sent' },
  ] };
  b.checkpoint = { observations: [
    { call_id: 'receiver-delegated', kind: 'call_constructed', peer_correlation: { caller_call_id: delegated } },
    { call_id: 'receiver-delegated', kind: 'runtime_execution_started', provenance: {} },
    { call_id: 'receiver-delegated', kind: 'runtime_execution_completed', provenance: {} },
    { call_id: 'receiver-delegated', kind: 'runtime_execution_completed', provenance: { executed_on: 'b' } },
    { call_id: 'receiver-uncovered', kind: 'call_constructed', peer_correlation: { caller_call_id: uncovered } },
    { call_id: 'receiver-uncovered', kind: 'toy_grant_expired', outcome: 'denied', detail_ref: 'toy_grant_reason:ExpiredGrant' },
  ] };
  const facts = assertMeshJobLedgers({ a, b, jobId, uncoveredCallId: uncovered });
  assert.equal(facts.delegated_execution_id, 'receiver-delegated');
  assert.equal(facts.uncovered_receiver_call_id, 'receiver-uncovered');
  b.checkpoint.observations.push({ call_id: 'receiver-uncovered', kind: 'runtime_execution_started', provenance: { executed_on: 'b' } });
  assert.throws(() => assertMeshJobLedgers({ a, b, jobId, uncoveredCallId: uncovered }));
});
