#!/usr/bin/env node
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { resolve, dirname, join } from 'node:path';
import { loadConfig, file, hash, identity, getOrRun, getOrRunRevocation, freshRun, revocationRun, revocationIdentity, revocationEnabled, validateExecutorAttestation } from './client.mjs';

async function main() {
  const args=process.argv.slice(2);
  assert.ok(args.length === 0 || (args.length === 2 && args[0] === '--config' && args[1]), 'usage: --config /absolute/private/config.json');
  const path=args.length ? args[1] : process.env.MCT_FIRST_REAL_APP_CONFIG;
  assert.ok(path, 'config required');
  const config=loadConfig(path);
  const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
  for (const [actual,expected] of [[config.component,join(root,'target/first-real-app/mct-repo-report.wasm')],[config.manifest,join(root,'target/first-real-app/child.toml')],[config.studio.binary,join(root,'target/debug/mct-daemon')],[config.studio.helper,join(root,'target/debug/examples/first_real_app_host')]]) assert.equal(actual,expected,'config path does not name built output');
  const bytes=Object.fromEntries(['input','component','manifest'].map(k => [k,file(config[k],2*1024*1024)]));
  const job=identity(hash(bytes.input),hash(bytes.component),config.worker.node);
  const jobRun=await getOrRun(config.jobs_root,job,(_job,state)=>freshRun(config,job,bytes,state),{requireCurrent:revocationEnabled(config)});
  if(!revocationEnabled(config)){console.log(JSON.stringify({durable_job_id:job.job_id,source:jobRun.source,report:jobRun.result.report,route:jobRun.result.route,worker_execution_count:jobRun.result.worker_summary.runtime_started}));return;}
  assert.equal(jobRun.result.schema,'mct-first-real-app/result-v2','version 2 requires a current evidence result');validateExecutorAttestation(config,jobRun.result.executor_attestation);assert.equal(jobRun.result.digests.worker_binary_sha256,config.worker.binary_sha256,'durable result daemon digest differs from config pin');assert.equal(jobRun.result.digests.worker_helper_sha256,config.worker.helper_sha256,'durable result helper digest differs from config pin');
  const revocation=revocationIdentity(job,config.studio.node,config.worker.node);const revocationRunResult=await getOrRunRevocation(config.jobs_root,job,jobRun.result,revocation,(_job,_result,state)=>revocationRun(config,job,jobRun.result,state));validateExecutorAttestation(config,revocationRunResult.receipt.executor_attestation);
  const source=jobRun.source==='durable_cache'&&revocationRunResult.source==='durable_cache'?'durable_cache':'executed';
  console.log(JSON.stringify({durable_job_id:job.job_id,source,durable_sources:{job:jobRun.source,revocation:revocationRunResult.source},execution_mode:jobRun.result.execution_mode,executor_attestation:jobRun.result.executor_attestation,report:jobRun.result.report,report_sha256:jobRun.result.report_sha256,route:jobRun.result.route,worker_execution_count:jobRun.result.worker_summary.runtime_started,endpoints:jobRun.result.endpoints,revocation:{probe_call_id:revocationRunResult.receipt.probe_call_id,outcome:revocationRunResult.receipt.outcome,protocol_reason:revocationRunResult.receipt.protocol_reason,revocation_count:revocationRunResult.receipt.ledger_summary.revocation_count,revocation_sequence:revocationRunResult.receipt.ledger_summary.revocation_sequence,rejection_sequence:revocationRunResult.receipt.ledger_summary.rejection_sequence,hello_reason:revocationRunResult.receipt.ledger_summary.hello_reason,baseline_sequence:revocationRunResult.receipt.baseline.ledger_sequence,baseline_head_hash:revocationRunResult.receipt.baseline.ledger_head_hash}}));
}
function printError(error, prefix = '') {
  console.error(`${prefix}${error?.stack ?? String(error)}`);
  if (error instanceof AggregateError) {
    for (const nested of error.errors) printError(nested, `${prefix}caused by: `);
  }
}
main().catch(error => { printError(error); process.exitCode=1; });
