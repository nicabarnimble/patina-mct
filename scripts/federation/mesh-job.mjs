import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { appendFileSync, mkdirSync, readFileSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { posix } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as sleep } from 'node:timers/promises';
import { deploy } from './deploy.mjs';
import { createFleet, createSshTransport, sanitize, validateConfig } from './fleet.mjs';

const TARGET_LOCAL = Object.freeze({ namespace: 'patina:fleet-n0', interface_name: 'work@0.1.0', function_name: 'step' });
const TARGET_REPO = Object.freeze({ namespace: 'patina:repo-report', interface_name: 'report@0.1.0', function_name: 'inspect' });
const runtimeKinds = new Set(['runtime_execution_started', 'runtime_execution_completed', 'runtime_execution_failed', 'runtime_execution_timed_out']);
const nonempty = value => typeof value === 'string' && value.length > 0;
const traceId = observation => observation.trace_id ?? observation.trace?.trace_id;

function resultPayload(piece) {
  assert.ok(nonempty(piece.inline_result_payload_base64), 'completed piece has no result bytes');
  const decoded = JSON.parse(Buffer.from(piece.inline_result_payload_base64, 'base64').toString('utf8'));
  assert.deepEqual(decoded, piece.decoded_result, 'decoded piece result differs from result bytes');
  return decoded;
}

export function assertMeshJobResult(reply, jobId, a, b, inputSha256) {
  assert.equal(reply?.job_id, jobId, 'wrong mesh job ID');
  assert.equal(reply.outcome, 'completed', 'mesh job did not complete');
  assert.equal(reply.safe_message, 'mesh job completed');
  assert.ok(Array.isArray(reply.pieces) && reply.pieces.length === 2, 'mesh job must have two results');
  const [local, delegated] = reply.pieces;
  assert.equal(local?.piece_id, 'local', 'mesh job local piece order changed');
  assert.equal(delegated?.piece_id, 'delegated', 'mesh job delegated piece order changed');
  assert.equal(local.call_id, `${jobId}:local`);
  assert.equal(local.outcome, 'completed');
  assert.equal(local.route_taken?.node_id, a.node);
  assert.equal(local.route_taken?.runtime_kind, 'wasm_component');
  assert.deepEqual(resultPayload(local), { results: [41] });
  assert.equal(delegated.call_id, `${jobId}:delegated`);
  assert.equal(delegated.outcome, 'completed');
  assert.equal(delegated.route_taken?.node_id, b.node);
  assert.equal(delegated.route_taken?.runtime_kind, 'remote_peer');
  const outer = resultPayload(delegated);
  assert.ok(Array.isArray(outer.results) && outer.results.length === 1 && nonempty(outer.results[0]?.ok), 'repo-report result missing');
  const report = JSON.parse(outer.results[0].ok);
  assert.equal(report.schema, 'mct-repo-report/v1');
  assert.equal(report.sha256, inputSha256);
  assert.ok(Number.isSafeInteger(report.package_count) && report.package_count > 0);
  return { local, delegated, report };
}

function correlationFor(observations, callerCallId) {
  const matches = observations.filter(observation => observation.peer_correlation?.caller_call_id === callerCallId);
  assert.equal(matches.length, 1, `missing or duplicate peer correlation for ${callerCallId}`);
  return matches[0];
}

function runtimeFor(observations, callId) {
  return observations.filter(observation => runtimeKinds.has(observation.kind) && observation.call_id === callId);
}

export function assertMeshJobLedgers({ a, b, jobId, uncoveredCallId }) {
  const localCallId = `${jobId}:local`;
  const delegatedCallId = `${jobId}:delegated`;
  const aObservations = a.checkpoint.observations;
  const bObservations = b.checkpoint.observations;
  const jobResults = aObservations.filter(observation => observation.call_id === jobId && observation.kind === 'result_recorded');
  assert.ok(jobResults.some(observation => observation.outcome === 'completed' && observation.detail_ref?.includes('local_mesh_job_result:completed=2;denied=0;failed=0')), 'A ledger lacks the completed two-piece job fact');

  const aLocal = runtimeFor(aObservations, localCallId);
  assert.equal(aLocal.filter(observation => observation.kind === 'runtime_execution_started').length, 1, 'A local piece did not start exactly once');
  assert.equal(aLocal.filter(observation => observation.kind === 'runtime_execution_completed').length, 1, 'A local piece did not complete exactly once');
  assert.ok(aLocal.every(observation => observation.provenance?.executed_on === a.node), 'A ledger attributes its piece to another Mother');
  assert.equal(runtimeFor(bObservations, localCallId).length, 0, 'B executed A local piece');

  const delegatedCorrelation = correlationFor(bObservations, delegatedCallId);
  const delegatedExecutionId = delegatedCorrelation.call_id;
  const bDelegated = runtimeFor(bObservations, delegatedExecutionId);
  assert.equal(bDelegated.filter(observation => observation.kind === 'runtime_execution_started').length, 1, 'B delegated piece did not start exactly once');
  assert.equal(bDelegated.filter(observation => observation.kind === 'runtime_execution_completed').length, 1, 'B delegated piece did not complete exactly once');
  assert.ok(bDelegated.every(observation => observation.provenance?.executed_on === b.node), 'B ledger attributes its piece to another Mother');
  assert.equal(runtimeFor(aObservations, delegatedCallId).length, 0, 'A locally executed B delegated piece');
  const sent = aObservations.filter(observation => observation.call_id === delegatedCallId && observation.kind === 'peer_call_sent');
  assert.equal(sent.length, 1, 'A ledger lacks exact delegated-piece forwarding');

  const uncoveredCorrelation = correlationFor(bObservations, uncoveredCallId);
  const uncoveredExecutionId = uncoveredCorrelation.call_id;
  const expiries = bObservations.filter(observation => observation.call_id === uncoveredExecutionId && observation.kind === 'toy_grant_expired');
  assert.ok(expiries.length >= 1, 'B ledger lacks the expired ToyGrant refusal');
  assert.ok(expiries.every(observation => observation.outcome === 'denied' && observation.detail_ref === 'toy_grant_reason:ExpiredGrant'), 'B refusal is not exact expired-grant authority');
  assert.equal(runtimeFor(bObservations, uncoveredExecutionId).length, 0, 'B started the uncovered piece');
  assert.equal(runtimeFor(aObservations, uncoveredCallId).length, 0, 'A started the uncovered piece');

  const aRuntimeIds = new Set(aObservations.filter(observation => runtimeKinds.has(observation.kind)).map(observation => observation.call_id));
  const bRuntimeIds = new Set(bObservations.filter(observation => runtimeKinds.has(observation.kind)).map(observation => observation.call_id));
  assert.deepEqual([...aRuntimeIds], [localCallId], 'A ledger contains execution for more than A piece');
  assert.deepEqual([...bRuntimeIds], [delegatedExecutionId], 'B ledger contains execution for more than B piece');
  return { local_call_id: localCallId, delegated_call_id: delegatedCallId, delegated_execution_id: delegatedExecutionId, uncovered_call_id: uncoveredCallId, uncovered_receiver_call_id: uncoveredExecutionId, toy_expiry_facts: expiries.length };
}

export async function runMeshJob(input, options = {}) {
  const config = validateConfig(input);
  let fleet; let failure; let report;
  try {
    fleet = await createFleet(config, 'two', {
      ...options,
      async provisionHost(current, host) {
        await options.provisionHost?.(current, host);
        if (host.index !== 1) return;
        host.repoReport = await current.request(host, { op: 'provision_repo_report', package: config.package });
        assert.equal(host.repoReport?.verification_outcome, 'verified');
        assert.equal(host.repoReport?.approval_state, 'approved');
        current.emit('repo-report-provisioned', { node: host.node, artifact_id: host.repoReport.artifact_id, input_sha256: host.repoReport.input_sha256 });
      },
    });
    const [a, b] = fleet.hosts;
    assert.ok(b.repoReport, 'repo-report fixture was not provisioned on B');
    const jobId = `nic63-${randomUUID()}`;
    const expiresAt = new Date(Date.now() + 45_000).toISOString();
    const grant = await fleet.request(b, { op: 'grant_repo_report', caller_node: a.node, mutation_id: `grant-${randomUUID()}`, expires_at: expiresAt });
    assert.equal(grant.expires_at, expiresAt);
    assert.equal(grant.input_sha256, b.repoReport.input_sha256);
    fleet.emit('repo-report-granted', { receiver: b.node, caller: a.node, expires_at: expiresAt, artifact_id: grant.artifact_id });
    const payload = JSON.stringify([JSON.stringify({ sha256: b.repoReport.input_sha256 })]);
    const request = { op: 'mesh_job', job_id: jobId, pieces: [
      { piece_id: 'local', payload_json: '[41]', target: TARGET_LOCAL },
      { piece_id: 'delegated', payload_json: payload, target: TARGET_REPO },
    ] };
    const refreshDeadline = Date.now() + 30_000;
    let jobReply;
    for (;;) {
      jobReply = await fleet.request(a, request);
      const delegated = jobReply?.pieces?.find(piece => piece.piece_id === 'delegated');
      if (jobReply?.outcome === 'completed') break;
      assert.equal(delegated?.outcome, 'denied', 'mesh job failed for a non-refresh reason');
      assert.equal(delegated?.protocol_reason, 'no_route', 'mesh job refresh denial was not no_route');
      assert.ok(Date.now() < refreshDeadline && Date.now() < Date.parse(expiresAt), 'repo-report publication did not become callable before grant expiry');
      await sleep(options.pollMs ?? 500, undefined, { signal: options.signal });
    }
    const accepted = assertMeshJobResult(jobReply, jobId, a, b, b.repoReport.input_sha256);
    fleet.emit('mesh-job-completed', { job_id: jobId, pieces: jobReply.pieces.map(piece => ({ piece_id: piece.piece_id, call_id: piece.call_id, outcome: piece.outcome, route: piece.route_taken })) });
    const waitMs = Math.max(0, Date.parse(expiresAt) - Date.now() + 1_500);
    await sleep(waitMs, undefined, { signal: options.signal });
    const uncoveredCallId = `${jobId}-uncovered`;
    const refusal = await fleet.request(a, { op: 'call_target', call_id: uncoveredCallId, payload_json: payload, target: TARGET_REPO });
    assert.equal(refusal?.outcome, 'denied', 'uncovered piece was not refused');
    assert.equal(refusal.route_taken, null, 'uncovered piece reports an execution route');
    assert.equal(refusal.decoded_result, null, 'uncovered piece reports a result');
    fleet.emit('uncovered-refused', { job_id: jobId, call_id: uncoveredCallId, receiver: b.node, outcome: refusal.outcome, protocol_reason: refusal.protocol_reason });
    for (const host of fleet.hosts) { await fleet.stop(host); await fleet.verify(host); }
    const ledgerFacts = assertMeshJobLedgers({ a, b, jobId, uncoveredCallId });
    report = { status: 'passed', slice: 'mesh-job', run: fleet.token, job_id: jobId, nodes: [a.node, b.node], grant: { receiver: b.node, caller: a.node, expires_at: expiresAt, artifact_id: grant.artifact_id, input_sha256: grant.input_sha256 }, job: { outcome: jobReply.outcome, pieces: jobReply.pieces, report: accepted.report }, refusal: { call_id: uncoveredCallId, outcome: refusal.outcome, protocol_reason: refusal.protocol_reason, fact: 'expired ToyGrant refused before runtime' }, ledger_facts: ledgerFacts, binaries: fleet.binaries, ledgers: fleet.hosts.map(host => ({ node: host.node, root: host.root, ...host.checkpoint })) };
  } catch (error) { failure = error; fleet?.emit('failed', { error: error.message }); }
  finally {
    if (fleet) {
      const cleanup = await fleet.close();
      if (cleanup.length) failure = new AggregateError([...(failure ? [failure] : []), ...cleanup.map(item => Error(`${item.node}: ${item.error}`))], `${failure ? `${failure.message}; ` : ''}helper cleanup was not confirmed`);
    }
  }
  if (failure) throw failure;
  fleet.emit('complete', { slice: 'mesh-job', job_id: report.job_id });
  return sanitize(report, fleet.config);
}

function absolutePath(value) {
  return nonempty(value) && value.startsWith('/') && value !== '/' && !/[\x00-\x1f\x7f]/.test(value) && !value.endsWith('/') && value.split('/').slice(1).every(part => part && part !== '.' && part !== '..');
}

export async function main(args = process.argv.slice(2)) {
  const values = {};
  for (let i = 0; i < args.length; i += 2) {
    assert.ok(['--config', '--evidence'].includes(args[i]) && nonempty(args[i + 1]) && !Object.hasOwn(values, args[i]), 'usage: --config private.json --evidence absolute_new_dir');
    values[args[i]] = args[i + 1];
  }
  assert.equal(Object.keys(values).length, 2, 'config and evidence are required');
  assert.ok(absolutePath(values['--evidence']), 'evidence must be an absolute new directory');
  const config = validateConfig(JSON.parse(readFileSync(values['--config'], 'utf8')));
  assert.ok(statSync(config.sshIdentity).isFile(), 'SSH identity is not a file');
  const evidence = values['--evidence'];
  mkdirSync(evidence, { mode: 0o700 });
  const emit = event => {
    const line = JSON.stringify(sanitize({ time: new Date().toISOString(), ...event }, config)) + '\n';
    appendFileSync(posix.join(evidence, 'stages.jsonl'), line, { mode: 0o600 });
    process.stdout.write(line);
  };
  const abort = new AbortController();
  const stop = () => abort.abort(Error('controller interrupted'));
  process.once('SIGINT', stop); process.once('SIGTERM', stop);
  const transport = createSshTransport({ onDiagnostic: (node, text) => emit({ stage: 'diagnostic', node, text }) });
  try {
    await deploy(config, 'two', event => emit({ stage: 'deploy', ...event }));
    const result = await runMeshJob(config, { transport, onStage: emit, signal: abort.signal });
    writeFileSync(posix.join(evidence, 'result.json'), JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
  } catch (error) {
    const result = sanitize({ status: 'failed', slice: 'mesh-job', error: error.message, causes: (error.errors ?? []).map(cause => cause.message) }, config);
    writeFileSync(posix.join(evidence, 'result.json'), JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
    emit({ stage: 'failed', ...result });
    throw Error([result.error, ...result.causes].join('\n'));
  } finally { process.removeListener('SIGINT', stop); process.removeListener('SIGTERM', stop); }
}

let entrypoint;
try { entrypoint = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined; }
catch { entrypoint = undefined; }
if (import.meta.url === entrypoint) main().catch(error => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
