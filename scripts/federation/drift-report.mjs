import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { appendFileSync, mkdirSync, readFileSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { posix } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as sleep } from 'node:timers/promises';
import { deploy } from './deploy.mjs';
import { createFleet, createSshTransport, sanitize, validateConfig } from './fleet.mjs';
import { correlationFor, resultPayload, runtimeFor } from './mesh-job.mjs';

const TARGET_REPO = Object.freeze({ namespace: 'patina:repo-report', interface_name: 'report@0.1.0', function_name: 'inspect' });
const runtimeKinds = new Set(['runtime_execution_started', 'runtime_execution_completed', 'runtime_execution_failed', 'runtime_execution_timed_out']);
const nonempty = value => typeof value === 'string' && value.length > 0;

function validateVersions(packageName, versions) {
  assert.ok(Array.isArray(versions) && versions.length > 0, `package ${packageName} has no versions`);
  let previous;
  for (const version of versions) {
    assert.ok(nonempty(version), `package ${packageName} has an invalid version`);
    if (previous !== undefined) assert.ok(previous < version, `package ${packageName} versions are not sorted and unique`);
    previous = version;
  }
}

function validateInventory(side) {
  assert.ok(side && nonempty(side.node_id), 'inventory has no Mother node ID');
  const report = side.report;
  assert.equal(report?.schema, 'mct-repo-report/v1', 'inventory schema mismatch');
  assert.match(report.sha256 ?? '', /^[a-f0-9]{64}$/, 'inventory digest is invalid');
  assert.ok(Number.isSafeInteger(report.package_count) && report.package_count > 0, 'inventory package count is invalid');
  assert.ok(report.packages && Object.getPrototypeOf(report.packages) === Object.prototype, 'inventory packages are invalid');
  for (const [packageName, versions] of Object.entries(report.packages)) {
    assert.ok(nonempty(packageName), 'inventory package name is invalid');
    validateVersions(packageName, versions);
  }
  return report;
}

export function compareDependencyInventories(left, right) {
  const leftReport = validateInventory(left);
  const rightReport = validateInventory(right);
  assert.notEqual(left.node_id, right.node_id, 'dependency drift requires distinct Mothers');
  const packageNames = [...new Set([...Object.keys(leftReport.packages), ...Object.keys(rightReport.packages)])].sort();
  const differences = [];
  for (const packageName of packageNames) {
    const leftVersions = leftReport.packages[packageName] ?? [];
    const rightVersions = rightReport.packages[packageName] ?? [];
    if (leftVersions.length === rightVersions.length && leftVersions.every((version, index) => version === rightVersions[index])) continue;
    differences.push({
      package: packageName,
      kind: leftVersions.length === 0 ? 'only_right' : rightVersions.length === 0 ? 'only_left' : 'version_mismatch',
      left_versions: [...leftVersions],
      right_versions: [...rightVersions],
    });
  }
  return {
    schema: 'mct-dependency-drift/v1',
    left_mother: { node_id: left.node_id, sha256: leftReport.sha256, package_count: leftReport.package_count },
    right_mother: { node_id: right.node_id, sha256: rightReport.sha256, package_count: rightReport.package_count },
    difference_count: differences.length,
    differences,
  };
}

export function assertDriftJobResult(reply, jobId, a, b, inputDigests) {
  assert.equal(reply?.job_id, jobId, 'wrong drift job ID');
  assert.equal(reply.outcome, 'completed', 'drift job did not complete');
  assert.equal(reply.safe_message, 'mesh job completed');
  assert.ok(Array.isArray(reply.pieces) && reply.pieces.length === 2, 'drift job must have two results');
  const [left, right] = reply.pieces;
  assert.equal(left?.piece_id, 'mother-a');
  assert.equal(right?.piece_id, 'mother-b');
  assert.equal(left.call_id, `${jobId}:mother-a`);
  assert.equal(right.call_id, `${jobId}:mother-b`);
  assert.equal(left.outcome, 'completed');
  assert.equal(right.outcome, 'completed');
  assert.equal(left.route_taken?.node_id, a.node, 'A inventory did not execute on A');
  assert.equal(left.route_taken?.runtime_kind, 'wasm_component');
  assert.equal(right.route_taken?.node_id, b.node, 'B inventory was not delegated to B');
  assert.equal(right.route_taken?.runtime_kind, 'remote_peer');
  const reports = [left, right].map((piece, index) => {
    const outer = resultPayload(piece);
    assert.ok(Array.isArray(outer.results) && outer.results.length === 1 && nonempty(outer.results[0]?.ok), 'repo-report result missing');
    const report = JSON.parse(outer.results[0].ok);
    assert.equal(report.schema, 'mct-repo-report/v1');
    assert.equal(report.sha256, inputDigests[index]);
    assert.ok(report.packages && Object.keys(report.packages).length > 0, 'repo-report package inventory missing');
    return report;
  });
  return { left, right, reports };
}

export function assertDriftLedgers({ a, b, jobId, uncoveredCallId }) {
  const leftCallId = `${jobId}:mother-a`;
  const rightCallId = `${jobId}:mother-b`;
  const aObservations = a.checkpoint.observations;
  const bObservations = b.checkpoint.observations;
  const jobResults = aObservations.filter(observation => observation.call_id === jobId && observation.kind === 'result_recorded');
  assert.ok(jobResults.some(observation => observation.outcome === 'completed' && observation.detail_ref?.includes('local_mesh_job_result:completed=2;denied=0;failed=0')), 'A ledger lacks the completed drift job fact');

  const aLeft = runtimeFor(aObservations, leftCallId);
  assert.equal(aLeft.filter(observation => observation.kind === 'runtime_execution_started').length, 1, 'A inventory did not start exactly once on A');
  assert.equal(aLeft.filter(observation => observation.kind === 'runtime_execution_completed' && observation.provenance?.executed_on === a.node).length, 1, 'A inventory lacks one exact executor attestation');
  assert.equal(aLeft.filter(observation => ['runtime_execution_failed', 'runtime_execution_timed_out'].includes(observation.kind)).length, 0, 'A inventory recorded runtime failure');
  assert.equal(runtimeFor(bObservations, leftCallId).length, 0, 'B executed A inventory');

  const rightCorrelation = correlationFor(bObservations, rightCallId);
  const rightExecutionId = rightCorrelation.call_id;
  const bRight = runtimeFor(bObservations, rightExecutionId);
  assert.equal(bRight.filter(observation => observation.kind === 'runtime_execution_started').length, 1, 'B inventory did not start exactly once on B');
  assert.equal(bRight.filter(observation => observation.kind === 'runtime_execution_completed' && observation.provenance?.executed_on === b.node).length, 1, 'B inventory lacks one exact executor attestation');
  assert.equal(bRight.filter(observation => ['runtime_execution_failed', 'runtime_execution_timed_out'].includes(observation.kind)).length, 0, 'B inventory recorded runtime failure');
  assert.equal(runtimeFor(aObservations, rightCallId).length, 0, 'A locally executed B inventory');
  assert.equal(aObservations.filter(observation => observation.call_id === rightCallId && observation.kind === 'peer_call_sent').length, 1, 'A ledger lacks exact B forwarding');

  const uncoveredCorrelation = correlationFor(bObservations, uncoveredCallId);
  const uncoveredExecutionId = uncoveredCorrelation.call_id;
  const expiries = bObservations.filter(observation => observation.call_id === uncoveredExecutionId && observation.kind === 'toy_grant_expired');
  assert.ok(expiries.length >= 1, 'B ledger lacks the expired directory-read refusal');
  assert.ok(expiries.every(observation => observation.outcome === 'denied' && observation.detail_ref === 'toy_grant_reason:ExpiredGrant'), 'B refusal is not exact expired-grant authority');
  assert.equal(runtimeFor(bObservations, uncoveredExecutionId).length, 0, 'B started the expired-grant probe');
  assert.equal(runtimeFor(aObservations, uncoveredCallId).length, 0, 'A started the expired-grant probe');

  const aRuntimeIds = new Set(aObservations.filter(observation => runtimeKinds.has(observation.kind)).map(observation => observation.call_id));
  const bRuntimeIds = new Set(bObservations.filter(observation => runtimeKinds.has(observation.kind)).map(observation => observation.call_id));
  assert.deepEqual([...aRuntimeIds], [leftCallId], 'A ledger contains execution outside A inventory');
  assert.deepEqual([...bRuntimeIds], [rightExecutionId], 'B ledger contains execution outside B inventory');
  return {
    mother_a_call_id: leftCallId,
    mother_b_call_id: rightCallId,
    mother_b_execution_id: rightExecutionId,
    uncovered_call_id: uncoveredCallId,
    uncovered_receiver_call_id: uncoveredExecutionId,
    toy_expiry_facts: expiries.length,
  };
}

export async function runDriftReport(input, options = {}) {
  const config = validateConfig(input);
  let fleet; let failure; let report;
  try {
    fleet = await createFleet(config, 'two', {
      ...options,
      async provisionHost(current, host) {
        await options.provisionHost?.(current, host);
        host.repoReport = await current.request(host, { op: 'provision_repo_report', package: config.package, snapshot: host.index === 0 ? 'a' : 'b' });
        assert.equal(host.repoReport?.verification_outcome, 'verified');
        assert.equal(host.repoReport?.approval_state, 'approved');
        current.emit('repo-report-provisioned', { node: host.node, artifact_id: host.repoReport.artifact_id, input_sha256: host.repoReport.input_sha256, snapshot: host.repoReport.snapshot });
      },
    });
    const [a, b] = fleet.hosts;
    assert.ok(a.repoReport && b.repoReport, 'repo-report was not provisioned on both Mothers');
    const jobId = `nic105-${randomUUID()}`;
    const expiresAt = new Date(Date.now() + 60_000).toISOString();
    const grantA = await fleet.request(a, { op: 'grant_repo_report', mutation_id: `grant-a-${randomUUID()}`, expires_at: expiresAt });
    const grantB = await fleet.request(b, { op: 'grant_repo_report', caller_node: a.node, mutation_id: `grant-b-${randomUUID()}`, expires_at: expiresAt });
    for (const [host, grant] of [[a, grantA], [b, grantB]]) {
      assert.equal(grant.expires_at, expiresAt);
      assert.equal(grant.input_sha256, host.repoReport.input_sha256);
      fleet.emit('repo-report-granted', { receiver: host.node, caller: host === a ? a.node : a.node, expires_at: expiresAt, artifact_id: grant.artifact_id });
    }
    const payloadA = JSON.stringify([JSON.stringify({ sha256: a.repoReport.input_sha256 })]);
    const payloadB = JSON.stringify([JSON.stringify({ sha256: b.repoReport.input_sha256 })]);
    const request = { op: 'mesh_job', job_id: jobId, pieces: [
      { piece_id: 'mother-a', required_node_id: a.node, payload_json: payloadA, target: TARGET_REPO },
      { piece_id: 'mother-b', required_node_id: b.node, payload_json: payloadB, target: TARGET_REPO },
    ] };
    const refreshDeadline = Date.now() + 45_000;
    let jobReply;
    for (;;) {
      jobReply = await fleet.request(a, request);
      if (jobReply?.outcome === 'completed') break;
      const incomplete = jobReply?.pieces?.filter(piece => piece.outcome !== 'completed') ?? [];
      assert.ok(incomplete.length > 0 && incomplete.every(piece => piece.outcome === 'denied' && piece.protocol_reason === 'no_route'), 'drift job failed for a non-refresh reason');
      assert.ok(Date.now() < refreshDeadline && Date.now() < Date.parse(expiresAt), 'repo-report publication did not become callable before grant expiry');
      await sleep(options.pollMs ?? 500, undefined, { signal: options.signal });
    }
    const accepted = assertDriftJobResult(jobReply, jobId, a, b, [a.repoReport.input_sha256, b.repoReport.input_sha256]);
    const drift = compareDependencyInventories(
      { node_id: a.node, report: accepted.reports[0] },
      { node_id: b.node, report: accepted.reports[1] },
    );
    assert.ok(drift.difference_count > 0, 'physical snapshots produced no dependency drift');
    fleet.emit('drift-job-completed', { job_id: jobId, difference_count: drift.difference_count, pieces: jobReply.pieces.map(piece => ({ piece_id: piece.piece_id, call_id: piece.call_id, outcome: piece.outcome, route: piece.route_taken })) });

    const waitMs = Math.max(0, Date.parse(expiresAt) - Date.now() + 1_500);
    await sleep(waitMs, undefined, { signal: options.signal });
    const uncoveredCallId = `${jobId}-expired-b`;
    const refusal = await fleet.request(a, { op: 'call_target', call_id: uncoveredCallId, payload_json: payloadB, target: TARGET_REPO });
    assert.equal(refusal?.outcome, 'denied', 'expired B piece was not refused');
    assert.equal(refusal.route_taken, null, 'expired B piece reports an execution route');
    assert.equal(refusal.decoded_result, null, 'expired B piece reports a result');
    fleet.emit('expired-b-refused', { job_id: jobId, call_id: uncoveredCallId, receiver: b.node, outcome: refusal.outcome, protocol_reason: refusal.protocol_reason });

    for (const host of fleet.hosts) { await fleet.stop(host); await fleet.verify(host); }
    const ledgerFacts = assertDriftLedgers({ a, b, jobId, uncoveredCallId });
    report = {
      status: 'passed',
      slice: 'dependency-drift',
      run: fleet.token,
      job_id: jobId,
      nodes: [a.node, b.node],
      grants: [
        { receiver: a.node, caller: a.node, expires_at: expiresAt, artifact_id: grantA.artifact_id, input_sha256: grantA.input_sha256 },
        { receiver: b.node, caller: a.node, expires_at: expiresAt, artifact_id: grantB.artifact_id, input_sha256: grantB.input_sha256 },
      ],
      job: { outcome: jobReply.outcome, pieces: jobReply.pieces, inventories: accepted.reports },
      drift,
      refusal: { call_id: uncoveredCallId, outcome: refusal.outcome, protocol_reason: refusal.protocol_reason, fact: 'B expired directory-read grant refused before runtime' },
      ledger_facts: ledgerFacts,
      binaries: fleet.binaries,
      ledgers: fleet.hosts.map(host => ({ node: host.node, root: host.root, ...host.checkpoint })),
    };
  } catch (error) { failure = error; fleet?.emit('failed', { error: error.message }); }
  finally {
    if (fleet) {
      const cleanup = await fleet.close();
      if (cleanup.length) failure = new AggregateError([...(failure ? [failure] : []), ...cleanup.map(item => Error(`${item.node}: ${item.error}`))], `${failure ? `${failure.message}; ` : ''}helper cleanup was not confirmed`);
    }
  }
  if (failure) throw failure;
  fleet.emit('complete', { slice: 'dependency-drift', job_id: report.job_id });
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
    const result = await runDriftReport(config, { transport, onStage: emit, signal: abort.signal });
    writeFileSync(posix.join(evidence, 'result.json'), JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
  } catch (error) {
    const result = sanitize({ status: 'failed', slice: 'dependency-drift', error: error.message, causes: (error.errors ?? []).map(cause => cause.message) }, config);
    writeFileSync(posix.join(evidence, 'result.json'), JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
    emit({ stage: 'failed', ...result });
    throw Error([result.error, ...result.causes].join('\n'));
  } finally { process.removeListener('SIGINT', stop); process.removeListener('SIGTERM', stop); }
}

let entrypoint;
try { entrypoint = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined; }
catch { entrypoint = undefined; }
if (import.meta.url === entrypoint) main().catch(error => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
