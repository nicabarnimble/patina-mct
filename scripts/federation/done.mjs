import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const CHECK_IDS = ['C1', 'C2', 'C3', 'C4', 'C5', 'C6', 'C7'];
export const FROZEN_SOURCE = 'docs/design/mct-shape-review-opus.md';
export const KNOWN_FAILURES = 'docs/done/known-failures.json';
export const FROZEN_SHA256 = '49f72e8398e5aee4fa36f921c6ff9e2d7f4dba93a1f6f7ffffd1aeb1af2425f5';
const KNOWN_SCHEMA = 'mct-done-known-failures/v1';
const BASELINE_LABEL = 'legacy-profile baseline, not governed-profile proof';

const cargo = (pkg, target, filter, minimum, includeIgnored = false) => ({ kind: 'cargo', package: pkg, target, filter, minimum, includeIgnored });
const evidence = predicate => ({ kind: 'evidence', predicate });
const declared = reason => ({ kind: 'declared', reason });

export const PROBES = {
  'governed-runtime-startup': cargo('mct-daemon', ['--test', 'passkey_admin'], 'done_governed_runtime_admits_passkey_ledger', 1, true),
  'pi-installed-governed-mother': declared('probe-not-implemented'),
  'pi-call-within-budget': declared('probe-not-implemented'),
  'nic63-repo-report-on-pi': evidence('repoReportOnPi'),
  'toy-grant-expiry-mandatory': cargo('mct-kernel', ['--lib'], 'toy::tests::done_c2_toy_grant_without_expiry_is_denied', 1, true),
  'day-key-approval-and-grant-chain': declared('probe-not-implemented'),
  'unix-socket-mutation-refused': declared('probe-not-implemented'),
  'hostile-sockets-on-governed-pi': declared('probe-not-implemented'),
  'hostile-http-on-governed-pi': declared('probe-not-implemented'),
  'hostile-ungranted-path-on-governed-pi': declared('probe-not-implemented'),
  'hostile-dotdot-escape-on-governed-pi': declared('probe-not-implemented'),
  'hostile-symlink-escape-on-governed-pi': declared('probe-not-implemented'),
  'hostile-no-grant-call-on-governed-pi': declared('probe-not-implemented'),
  'hostile-forged-principal-on-governed-pi': declared('probe-not-implemented'),
  'hostile-imports-refused-locally': cargo('mct-daemon', ['--lib'], 'wasm::tests::hostile::', 4),
  'forged-origin-refused-iroh': cargo('mct-iroh', ['--lib'], 'tests::call_origin::', 1),
  'forged-origin-refused-resident': cargo('mct-daemon', ['--bin', 'mct-daemon'], 'call_origin::', 2),
  'expired-grant-refused-on-pi': evidence('expiredGrantRefusedOnPi'),
  'signed-receipt-per-outcome': declared('receipts-not-implemented'),
  'receipt-tamper-fails': declared('probe-not-implemented'),
  'person-revokes-grant': declared('probe-not-implemented'),
  'revocation-survives-sigkill-restart': declared('probe-not-implemented'),
  'toy-grant-revocation-unit': cargo('mct-kernel', ['--lib'], 'toy::tests::revoked', 1),
  'peer-revocation-on-fleet': evidence('peerRevocationOnFleet'),
  'signed-peer-admission': declared('probe-not-implemented'),
  'two-mother-governed-pi-run': declared('probe-not-implemented'),
  'nic63-two-mother-mesh-job': evidence('twoMotherMeshJob'),
  'survivors-serve-during-peer-loss': evidence('survivorsServe'),
  'unkeyed-single-launch-after-state-loss': declared('child-start-ledger-fact-missing'),
  'power-cut-on-pi': declared('probe-not-implemented'),
  'keyed-dispatch-fence-sqlite-rollback': cargo('mct-daemon', ['--test', 'mother_restart'], 'sqlite_rollback', 2),
  'sigkill-recovery-on-fleet': evidence('sigkillRecovery'),
};

const governed = (check, id, needs) => ({ check, id, profile: 'governed', ...(needs ? { needs: [needs] } : {}) });
const baseline = (check, id) => ({ check, id, profile: 'baseline' });
const RUNTIME = 'governed-runtime-startup';

export const STEPS = [
  governed('C1', RUNTIME), governed('C1', 'pi-installed-governed-mother', RUNTIME), governed('C1', 'pi-call-within-budget', 'pi-installed-governed-mother'),
  baseline('C1', 'nic63-repo-report-on-pi'),
  governed('C2', RUNTIME), governed('C2', 'toy-grant-expiry-mandatory'), governed('C2', 'day-key-approval-and-grant-chain', RUNTIME), governed('C2', 'unix-socket-mutation-refused', RUNTIME),
  governed('C3', RUNTIME),
  ...['sockets', 'http', 'ungranted-path', 'dotdot-escape', 'symlink-escape', 'no-grant-call', 'forged-principal'].map(name => governed('C3', `hostile-${name}-on-governed-pi`, RUNTIME)),
  baseline('C3', 'hostile-imports-refused-locally'), baseline('C3', 'forged-origin-refused-iroh'), baseline('C3', 'forged-origin-refused-resident'), baseline('C3', 'expired-grant-refused-on-pi'),
  governed('C4', RUNTIME), governed('C4', 'signed-receipt-per-outcome'), governed('C4', 'receipt-tamper-fails', 'signed-receipt-per-outcome'),
  governed('C5', RUNTIME), governed('C5', 'person-revokes-grant', RUNTIME), governed('C5', 'revocation-survives-sigkill-restart', 'person-revokes-grant'),
  baseline('C5', 'toy-grant-revocation-unit'), baseline('C5', 'peer-revocation-on-fleet'),
  governed('C6', RUNTIME), governed('C6', 'signed-peer-admission', RUNTIME), governed('C6', 'two-mother-governed-pi-run', 'signed-peer-admission'),
  baseline('C6', 'nic63-two-mother-mesh-job'), baseline('C6', 'survivors-serve-during-peer-loss'),
  governed('C7', RUNTIME), governed('C7', 'unkeyed-single-launch-after-state-loss'), governed('C7', 'power-cut-on-pi', RUNTIME),
  baseline('C7', 'keyed-dispatch-fence-sqlite-rollback'), baseline('C7', 'sigkill-recovery-on-fleet'),
];

export function parseFrozenChecks(markdown) {
  const section = markdown.split(/^## /m).find(part => part.startsWith('4. Done and proven')) ?? '';
  const checks = section.split('\n').filter(line => /^\| C\d \|/.test(line)).map(line => {
    const cells = line.split('|').map(cell => cell.trim());
    return { id: cells[1], text: cells[2] };
  });
  if (checks.map(check => check.id).join() !== CHECK_IDS.join()) throw new Error('frozen table must hold exactly C1-C7 in order');
  return checks;
}

export function frozenDigest(checks) {
  return createHash('sha256').update(JSON.stringify(checks.map(check => [check.id, check.text]))).digest('hex');
}

export function validateKnownFailures(value, steps) {
  if (value?.schema !== KNOWN_SCHEMA || !Array.isArray(value.failures)) throw new Error(`known failures schema must be ${KNOWN_SCHEMA}`);
  const seen = new Set();
  for (const entry of value.failures) {
    const key = `${entry.check}/${entry.step}`;
    if (!steps.some(step => step.check === entry.check && step.id === entry.step)) throw new Error(`unknown step ${key}`);
    if (typeof entry.reason !== 'string' || !/^[a-z0-9:-]+$/.test(entry.reason)) throw new Error(`invalid reason for ${key}`);
    if (typeof entry.owner !== 'string' || !entry.owner.trim()) throw new Error(`missing owner for ${key}`);
    if (seen.has(key)) throw new Error(`duplicate known failure ${key}`);
    seen.add(key);
  }
  return value;
}

export function parseCargoResult(output, exitCode, minimum) {
  const totals = [...output.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed;/g)]
    .reduce((sum, match) => ({ passed: sum.passed + Number(match[1]), failed: sum.failed + Number(match[2]) }), { passed: 0, failed: 0 });
  const named = output.match(/DONE-REASON: ([a-z0-9-]+)/)?.[1];
  const result = reason => ({ status: reason ? 'fail' : 'pass', reason, ...totals });
  if (!/test result:/.test(output)) return result(named ?? 'probe-build-failed');
  if (totals.failed > 0 || exitCode !== 0) return result(named ?? 'cargo-test-failed');
  if (totals.passed === 0) return result('probe-selected-no-tests');
  if (totals.passed < minimum) return result('probe-below-minimum');
  return result(null);
}

function readJson(root, path) { return JSON.parse(readFileSync(join(root, path), 'utf8')); }
const MESH = 'docs/evidence/nic63-mesh-job/result.json';
const SLICE3 = 'docs/evidence/nic63-slice3/result.json';

export const PREDICATES = {
  repoReportOnPi(root) {
    const mesh = readJson(root, MESH);
    const delegated = mesh.job?.pieces?.find(piece => piece.piece_id === 'delegated');
    const report = JSON.parse(delegated?.decoded_result?.results?.[0]?.ok ?? '{}');
    return mesh.status === 'passed' && mesh.grant?.receiver === 'nicanode0' && delegated?.outcome === 'completed' && report.schema === 'mct-repo-report/v1'
      ? null : 'evidence-repo-report-run-missing';
  },
  expiredGrantRefusedOnPi(root) {
    const mesh = readJson(root, MESH);
    return mesh.status === 'passed' && mesh.grant?.receiver === 'nicanode0' && mesh.refusal?.outcome === 'denied' && mesh.ledger_facts?.toy_expiry_facts === 1
      ? null : 'evidence-expired-grant-refusal-missing';
  },
  peerRevocationOnFleet(root) {
    const two = readJson(root, 'docs/evidence/nic63-suite-two/result.json');
    return two.status === 'passed' && two.calls?.some(call => call.label?.startsWith('revoked-') && call.outcome === 'denied')
      ? null : 'evidence-revocation-denial-missing';
  },
  twoMotherMeshJob(root) {
    const mesh = readJson(root, MESH);
    return mesh.status === 'passed' && new Set(mesh.nodes).size === 2 && mesh.job?.outcome === 'completed' && mesh.job.pieces?.length === 2
      && mesh.job.pieces.every(piece => piece.outcome === 'completed') && mesh.ledgers?.length === 2
      ? null : 'evidence-two-mother-run-missing';
  },
  survivorsServe(root) {
    const victims = readJson(root, SLICE3).partition?.victims ?? [];
    return victims.length >= 2 && victims.every(victim => victim.survivors_served && victim.rejoined) ? null : 'evidence-survivor-service-missing';
  },
  sigkillRecovery(root) {
    const stages = readFileSync(join(root, 'docs/evidence/nic63-slice3/stages.jsonl'), 'utf8').trim().split('\n').map(line => JSON.parse(line));
    const crashes = stages.filter(stage => stage.stage === 'stopped' && stage.mode === 'crash' && stage.exit_signal === 9).length;
    const restarts = stages.filter(stage => stage.stage === 'restarted').length;
    return readJson(root, SLICE3).status === 'passed' && crashes >= 3 && restarts >= 3 ? null : 'evidence-sigkill-recovery-missing';
  },
};

export function runEvidenceProbe(def, root) {
  let reason;
  try { reason = PREDICATES[def.predicate](root); } catch { reason = 'evidence-missing'; }
  return { status: reason ? 'fail' : 'pass', reason, basis: 'observed' };
}

function runCargoProbe(id, def, repo, logs) {
  const args = ['test', '--locked', '-p', def.package, ...def.target, def.filter, '--', ...(def.includeIgnored ? ['--include-ignored'] : []), '--test-threads=1'];
  const run = spawnSync('/bin/sh', ['-c', 'umask 077; exec "$@"', 'done-probe', 'cargo', ...args], {
    cwd: repo, encoding: 'utf8', timeout: 1_800_000, maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, TMPDIR: realpathSync(tmpdir()) },
  });
  const output = `${run.stdout ?? ''}${run.stderr ?? ''}`;
  writeFileSync(join(logs, `${id}.log`), `$ cargo ${args.join(' ')}\n${output}`);
  const { status, reason, passed, failed } = parseCargoResult(output, run.status ?? 1, def.minimum);
  return { status, reason, basis: 'observed', detail: { command: `cargo ${args.join(' ')}`, passed, failed } };
}

export function probeRunner(repo, logs) {
  return async (id, def) => {
    if (def.kind === 'declared') return { status: 'fail', reason: def.reason, basis: 'declared' };
    if (def.kind === 'evidence') return runEvidenceProbe(def, repo);
    return runCargoProbe(id, def, repo, logs);
  };
}

export async function runPlan(steps, probes, runProbe) {
  const cache = new Map();
  const results = [];
  for (const step of steps) {
    const unmet = (step.needs ?? []).find(need => cache.get(need)?.status !== 'pass');
    if (unmet) {
      results.push({ check: step.check, step: step.id, profile: step.profile, status: 'blocked', reason: `blocked:${unmet}`, basis: 'blocked' });
      continue;
    }
    if (!cache.has(step.id)) cache.set(step.id, await runProbe(step.id, probes[step.id]));
    results.push({ check: step.check, step: step.id, profile: step.profile, ...cache.get(step.id) });
  }
  return results;
}

export function evaluate(steps, stepResults, known) {
  const entries = new Map(known.failures.map(entry => [`${entry.check}/${entry.step}`, entry]));
  const untracked = [];
  const stale = [];
  for (const result of stepResults) {
    const entry = entries.get(`${result.check}/${result.step}`);
    if (result.status !== 'pass' && entry?.reason !== result.reason) untracked.push({ check: result.check, step: result.step, reason: result.reason });
    if (entry && (result.status === 'pass' || entry.reason !== result.reason)) stale.push(entry);
  }
  const checks = CHECK_IDS.map(id => {
    const own = stepResults.filter(result => result.check === id);
    const gov = own.filter(result => result.profile === 'governed');
    const base = own.filter(result => result.profile === 'baseline');
    const firstFail = gov.find(result => result.status !== 'pass');
    const baseStatus = base.length === 0 ? 'none' : base.every(result => result.status === 'pass') ? 'pass' : 'fail';
    const verdict = !firstFail ? 'pass' : baseStatus === 'pass' ? 'partly' : 'fail';
    return {
      id, verdict,
      governed: firstFail ? { status: 'fail', reason: firstFail.reason, step: firstFail.step } : { status: 'pass', reason: null, step: null },
      baseline: { status: baseStatus, label: BASELINE_LABEL },
      steps: own.map(result => ({ ...result, owner: entries.get(`${id}/${result.step}`)?.owner ?? null })),
    };
  });
  const counts = Object.fromEntries(['pass', 'partly', 'fail'].map(verdict => [verdict, checks.filter(check => check.verdict === verdict).length]));
  const drift = untracked.length > 0 || stale.length > 0;
  return { checks, counts, tracking: { untracked, stale }, exitCode: drift ? 2 : counts.pass === CHECK_IDS.length ? 0 : 1 };
}

export function formatReport(report, digest) {
  const lines = [`MCT done: C1-C7 in the governed profile (frozen text sha256 ${digest.slice(0, 12)})`];
  for (const check of report.checks) {
    const gov = check.governed.status === 'pass' ? 'governed PASS' : `governed FAIL ${check.governed.reason} [${check.governed.step}]`;
    const base = check.baseline.status === 'none' ? 'baseline none' : `baseline ${check.baseline.status.toUpperCase()} (${check.baseline.label})`;
    lines.push(`${check.id}  ${check.verdict.toUpperCase().padEnd(6)}  ${gov} | ${base}`);
    for (const step of check.steps) {
      const note = [step.basis, step.owner].filter(Boolean).join(', ');
      lines.push(`      ${step.profile.padEnd(8)} ${step.status.toUpperCase().padEnd(7)} ${step.step}${step.reason ? `: ${step.reason}` : ''} (${note})`);
    }
  }
  const { pass, partly, fail } = report.counts;
  lines.push(`result: ${report.exitCode === 0 ? 'DONE' : 'NOT DONE'} (${pass} pass, ${partly} partly, ${fail} fail)`);
  if (report.exitCode === 2) {
    for (const entry of report.tracking.untracked) lines.push(`tracking drift: untracked ${entry.check}/${entry.step}: ${entry.reason}`);
    for (const entry of report.tracking.stale) lines.push(`tracking drift: stale ${entry.check}/${entry.step}: ${entry.reason} (${entry.owner})`);
  } else lines.push(`tracking: every non-passing step matches ${KNOWN_FAILURES}`);
  return lines;
}

function sourceCommit(repo) {
  const head = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).stdout?.trim() || null;
  const dirty = spawnSync('git', ['status', '--porcelain', '--untracked-files=no'], { cwd: repo, encoding: 'utf8' }).stdout?.trim() !== '';
  return { head, dirty };
}

export async function main(argv, { repo = fileURLToPath(new URL('../..', import.meta.url)), log = line => process.stdout.write(`${line}\n`), runProbe } = {}) {
  const fatal = message => { log(`done: ${message}`); return 2; };
  if (argv.length !== 2 || argv[0] !== '--evidence' || !isAbsolute(argv[1])) return fatal('usage: done.mjs --evidence /absolute/new/directory');
  const evidenceDir = argv[1];
  if (existsSync(evidenceDir)) return fatal('evidence directory must be new');
  if (!statSync(dirname(evidenceDir), { throwIfNoEntry: false })?.isDirectory()) return fatal('evidence parent must exist');
  let digest, known;
  try {
    digest = frozenDigest(parseFrozenChecks(readFileSync(join(repo, FROZEN_SOURCE), 'utf8')));
    if (digest !== FROZEN_SHA256) return fatal(`frozen C1-C7 text changed (${digest}); scope changes need Doug`);
    known = validateKnownFailures(JSON.parse(readFileSync(join(repo, KNOWN_FAILURES), 'utf8')), STEPS);
  } catch (error) { return fatal(error.message); }
  mkdirSync(evidenceDir, { mode: 0o700 });
  const logs = join(evidenceDir, 'logs');
  mkdirSync(logs, { mode: 0o700 });
  const stepResults = await runPlan(STEPS, PROBES, runProbe ?? probeRunner(repo, logs));
  const report = evaluate(STEPS, stepResults, known);
  const lines = formatReport(report, digest);
  for (const line of lines) log(line);
  writeFileSync(join(evidenceDir, 'summary.txt'), `${lines.join('\n')}\n`);
  writeFileSync(join(evidenceDir, 'result.json'), `${JSON.stringify({
    schema: 'mct-done-result/v1', frozen_source: FROZEN_SOURCE, frozen_sha256: digest, source: sourceCommit(repo),
    done: report.exitCode === 0, exit_code: report.exitCode, counts: report.counts, tracking: report.tracking, checks: report.checks,
  }, null, 2)}\n`);
  return report.exitCode;
}

let entrypoint;
try { entrypoint = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined; }
catch { entrypoint = undefined; }
if (import.meta.url === entrypoint) {
  main(process.argv.slice(2)).then(code => { process.exitCode = code; }, error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 2; });
}
