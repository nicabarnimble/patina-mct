import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync, cpSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import {
  CHECK_IDS, FROZEN_SHA256, PROBES, STEPS, evaluate, frozenDigest, main, parseCargoResult,
  parseFrozenChecks, runPlan, runEvidenceProbe, validateKnownFailures,
} from './done.mjs';

const repo = fileURLToPath(new URL('../..', import.meta.url));
const review = readFileSync(join(repo, 'docs/design/mct-shape-review-opus.md'), 'utf8');
const tracked = JSON.parse(readFileSync(join(repo, 'docs/done/known-failures.json'), 'utf8'));

const pass = { status: 'pass', reason: null, basis: 'observed' };
const fail = reason => ({ status: 'fail', reason, basis: 'observed' });

function allPassing() { return Object.fromEntries(Object.keys(PROBES).map(id => [id, pass])); }
function todayResults() {
  const results = allPassing();
  for (const entry of tracked.failures) if (!entry.reason.startsWith('blocked:')) results[entry.step] = fail(entry.reason);
  return results;
}
async function planWith(results) {
  const calls = [];
  const stepResults = await runPlan(STEPS, PROBES, async id => { calls.push(id); return results[id]; });
  return { calls, stepResults };
}

test('frozen C1-C7 text is read from the review and pinned by digest', () => {
  const checks = parseFrozenChecks(review);
  assert.deepEqual(checks.map(check => check.id), CHECK_IDS);
  assert.match(checks[0].text, /Real Child, real Pi 3\..*at most 96 MiB.*within 2 s/);
  assert.equal(frozenDigest(checks), FROZEN_SHA256);
  const edited = review.replace('at most 96 MiB', 'at most 128 MiB');
  assert.notEqual(frozenDigest(parseFrozenChecks(edited)), FROZEN_SHA256);
  assert.throws(() => parseFrozenChecks(review.replace('| C7 |', '| C8 |')), /exactly C1-C7/);
});

test('every check has governed steps; only the partly checks carry baseline evidence', () => {
  for (const id of CHECK_IDS) assert.ok(STEPS.some(step => step.check === id && step.profile === 'governed'), id);
  const baseline = new Set(STEPS.filter(step => step.profile === 'baseline').map(step => step.check));
  assert.deepEqual([...baseline].sort(), ['C1', 'C3', 'C5', 'C6', 'C7']);
  for (const step of STEPS) {
    assert.ok(PROBES[step.id], `probe for ${step.id}`);
    const earlier = STEPS.slice(0, STEPS.indexOf(step)).filter(other => other.check === step.check).map(other => other.id);
    for (const need of step.needs ?? []) assert.ok(earlier.includes(need), `${step.check}/${step.id} needs earlier ${need}`);
  }
  const hostile = STEPS.filter(step => step.check === 'C3' && step.profile === 'governed').map(step => step.id);
  for (const name of ['sockets', 'http', 'ungranted-path', 'dotdot-escape', 'symlink-escape', 'no-grant-call', 'forged-principal']) {
    assert.ok(hostile.includes(`hostile-${name}-on-governed-pi`), name);
  }
});

test('blocked steps never run their probe and a shared probe runs once', async () => {
  const { calls, stepResults } = await planWith(todayResults());
  assert.equal(calls.filter(id => id === 'governed-runtime-startup').length, 1);
  assert.ok(!calls.includes('pi-call-within-budget'));
  const blocked = stepResults.find(r => r.check === 'C1' && r.step === 'pi-call-within-budget');
  assert.equal(blocked.status, 'blocked');
  assert.equal(blocked.reason, 'blocked:pi-installed-governed-mother');
});

test('today: every check is red or partly with a named reason and tracking is exact', async () => {
  const { stepResults } = await planWith(todayResults());
  const report = evaluate(STEPS, stepResults, tracked);
  assert.equal(report.exitCode, 1);
  assert.deepEqual(report.tracking, { untracked: [], stale: [] });
  const verdicts = Object.fromEntries(report.checks.map(check => [check.id, check.verdict]));
  assert.deepEqual(verdicts, { C1: 'partly', C2: 'pass', C3: 'partly', C4: 'fail', C5: 'partly', C6: 'partly', C7: 'partly' });
  for (const check of report.checks.filter(check => check.verdict !== 'pass')) assert.match(check.governed.reason, /^[a-z0-9:-]+$/, check.id);
  const c6 = report.checks.find(check => check.id === 'C6');
  assert.equal(c6.baseline.status, 'pass');
  assert.equal(c6.baseline.label, 'legacy-profile baseline, not governed-profile proof');
  assert.equal(report.checks.find(check => check.id === 'C2').governed.reason, null);
  assert.equal(report.checks.find(check => check.id === 'C4').governed.reason, 'receipts-not-implemented');
});

test('an untracked failure, a stale entry, or a changed reason is tracking drift', async () => {
  const untracked = todayResults(); untracked['nic63-two-mother-mesh-job'] = fail('evidence-two-mother-run-missing');
  assert.equal(evaluate(STEPS, (await planWith(untracked)).stepResults, tracked).exitCode, 2);
  const stale = todayResults(); stale['signed-receipt-per-outcome'] = pass;
  const staleReport = evaluate(STEPS, (await planWith(stale)).stepResults, tracked);
  assert.equal(staleReport.exitCode, 2);
  assert.deepEqual(staleReport.tracking.stale.map(entry => entry.step), ['signed-receipt-per-outcome', 'receipt-tamper-fails'], 'unblocking a step makes its blocked row stale too');
  const changed = todayResults(); changed['governed-runtime-startup'] = fail('governed-runtime-startup-failed');
  const changedReport = evaluate(STEPS, (await planWith(changed)).stepResults, tracked);
  assert.equal(changedReport.exitCode, 2);
  assert.ok(changedReport.tracking.untracked.some(entry => entry.reason === 'governed-runtime-startup-failed'));
});

test('all seven pass only when every governed step passes and nothing is tracked', async () => {
  const report = evaluate(STEPS, (await planWith(allPassing())).stepResults, { schema: tracked.schema, failures: [] });
  assert.equal(report.exitCode, 0);
  assert.ok(report.checks.every(check => check.verdict === 'pass'));
  const baselineOnly = allPassing(); baselineOnly['two-mother-governed-pi-run'] = fail('probe-not-implemented');
  const c6 = evaluate(STEPS, (await planWith(baselineOnly)).stepResults, { schema: tracked.schema, failures: [] });
  assert.equal(c6.checks.find(check => check.id === 'C6').verdict, 'partly');
  assert.equal(c6.exitCode, 2);
});

test('known-failure ledger rejects unknown steps, missing owners and duplicates', () => {
  assert.doesNotThrow(() => validateKnownFailures(tracked, STEPS));
  const entry = tracked.failures[0];
  assert.throws(() => validateKnownFailures({ ...tracked, failures: [...tracked.failures, { ...entry, step: 'nope' }] }, STEPS), /unknown step/);
  assert.throws(() => validateKnownFailures({ ...tracked, failures: [{ ...entry, owner: '' }] }, STEPS), /owner/);
  assert.throws(() => validateKnownFailures({ ...tracked, failures: [entry, entry] }, STEPS), /duplicate/);
  assert.throws(() => validateKnownFailures({ ...tracked, schema: 'other' }, STEPS), /schema/);
});

test('cargo results name the failure from the probe, not from the harness', () => {
  const ok = 'running 1 test\ntest x ... ok\n\ntest result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 9 filtered out\n';
  assert.deepEqual(parseCargoResult(ok, 0, 1), { status: 'pass', reason: null, passed: 1, failed: 0 });
  const named = "thread 'x' panicked:\nDONE-REASON: runtime-refuses-passkey-ledger: AuthorityReplayBlocked\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
  assert.equal(parseCargoResult(named, 101, 1).reason, 'runtime-refuses-passkey-ledger');
  const unnamed = 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n';
  assert.equal(parseCargoResult(unnamed, 101, 1).reason, 'cargo-test-failed');
  assert.equal(parseCargoResult('test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out\n', 0, 1).reason, 'probe-selected-no-tests');
  assert.equal(parseCargoResult(ok, 0, 2).reason, 'probe-below-minimum');
  assert.equal(parseCargoResult('error[E0425]: cannot find value', 101, 1).reason, 'probe-build-failed');
  const two = ok + ok.replace('1 passed', '2 passed');
  assert.deepEqual(parseCargoResult(two, 0, 3), { status: 'pass', reason: null, passed: 3, failed: 0 });
});

test('baseline evidence probes read the checked-in artifacts and fail on a mutated copy', t => {
  for (const id of Object.keys(PROBES).filter(id => PROBES[id].kind === 'evidence')) {
    assert.deepEqual(runEvidenceProbe(PROBES[id], repo).status, 'pass', id);
  }
  const root = mkdtempSync(join(tmpdir(), 'mct-done-evidence-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  cpSync(join(repo, 'docs/evidence'), join(root, 'docs/evidence'), { recursive: true });
  const mesh = join(root, 'docs/evidence/nic63-mesh-job/result.json');
  const value = JSON.parse(readFileSync(mesh, 'utf8'));
  value.ledger_facts.toy_expiry_facts = 0;
  writeFileSync(mesh, JSON.stringify(value));
  const refused = runEvidenceProbe(PROBES['expired-grant-refused-on-pi'], root);
  assert.equal(refused.status, 'fail');
  assert.equal(refused.reason, 'evidence-expired-grant-refusal-missing');
  rmSync(join(root, 'docs/evidence/nic63-slice3/stages.jsonl'));
  assert.equal(runEvidenceProbe(PROBES['sigkill-recovery-on-fleet'], root).reason, 'evidence-missing');
});

test('main writes a new evidence directory and exits with the report code', async t => {
  const root = mkdtempSync(join(tmpdir(), 'mct-done-main-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const evidence = join(root, 'evidence');
  const lines = [];
  const results = todayResults();
  const code = await main(['--evidence', evidence], { repo, log: line => lines.push(line), runProbe: async id => results[id] });
  assert.equal(code, 1);
  const written = JSON.parse(readFileSync(join(evidence, 'result.json'), 'utf8'));
  assert.equal(written.schema, 'mct-done-result/v1');
  assert.equal(written.frozen_sha256, FROZEN_SHA256);
  assert.equal(written.done, false);
  assert.match(lines.join('\n'), /C6 +PARTLY .*legacy-profile baseline, not governed-profile proof/);
  assert.equal(await main(['--evidence', evidence], { repo, log() {}, runProbe: async id => results[id] }), 2, 'evidence reuse is refused');
  assert.equal(await main(['--evidence', 'relative'], { repo, log() {}, runProbe: async id => results[id] }), 2);
});

test('suite.sh done forwards one absolute evidence path to the done runner', t => {
  const root = mkdtempSync(join(tmpdir(), 'mct-done-suite-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, 'bin'); mkdirSync(bin);
  const log = join(root, 'calls');
  writeFileSync(join(bin, 'node'), '#!/bin/sh\nprintf "%s\\n" "$@" > "$CALL_LOG"\nexit 1\n', { mode: 0o700 });
  const suite = fileURLToPath(new URL('./suite.sh', import.meta.url));
  const run = args => spawnSync('bash', [suite, ...args], { encoding: 'utf8', env: { ...process.env, PATH: `${bin}:/usr/bin:/bin`, CALL_LOG: log } });
  const evidence = join(root, 'done-evidence');
  assert.equal(run(['done', evidence]).status, 1, 'not-done exit code passes through');
  const forwarded = readFileSync(log, 'utf8').trim().split('\n');
  assert.match(forwarded[0], /done\.mjs$/);
  assert.deepEqual(forwarded.slice(1), ['--evidence', evidence]);
  rmSync(log);
  assert.equal(run(['done', 'relative']).status, 64);
  assert.equal(existsSync(log), false);
});
