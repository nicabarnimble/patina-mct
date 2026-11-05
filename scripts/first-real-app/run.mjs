#!/usr/bin/env node
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { resolve, dirname, join } from 'node:path';
import { loadConfig, file, hash, identity, getOrRun, freshRun } from './client.mjs';

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
  const {result,source}=await getOrRun(config.jobs_root,job,()=>freshRun(config,job,bytes));
  console.log(JSON.stringify({durable_job_id:job.job_id,source,report:result.report,route:result.route,worker_execution_count:result.worker_summary.runtime_started}));
}
function printError(error, prefix = '') {
  console.error(`${prefix}${error?.stack ?? String(error)}`);
  if (error instanceof AggregateError) {
    for (const nested of error.errors) printError(nested, `${prefix}caused by: `);
  }
}
main().catch(error => { printError(error); process.exitCode=1; });
