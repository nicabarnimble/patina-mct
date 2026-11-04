import assert from 'node:assert/strict';
import { existsSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { dirname, isAbsolute } from 'node:path';
import { pathToFileURL } from 'node:url';
import { deploy } from './deploy.mjs';
import { main, validateConfig } from './fleet.mjs';

export function parseArgs(args) {
  assert.equal(args.length, 6, 'usage: --config private.json --slice two|three|upgrade --evidence absolute_new_directory');
  const result = {};
  for (let i = 0; i < args.length; i += 2) {
    const name = args[i].replace(/^--/, '');
    assert.ok(['config', 'slice', 'evidence'].includes(name) && args[i] === `--${name}` && !Object.hasOwn(result, name) && args[i + 1], 'invalid or duplicate argument');
    result[name] = args[i + 1];
  }
  assert.ok(['two', 'three', 'upgrade'].includes(result.slice), 'invalid slice');
  assert.ok(isAbsolute(result.evidence), 'evidence directory must be absolute');
  return result;
}

export function errorDetails(error) {
  return [error.message, ...(error.errors ?? []).map(errorDetails)].join('\n');
}

export async function run(args = process.argv.slice(2)) {
  const values = parseArgs(args);
  assert.ok(!existsSync(values.evidence), 'evidence directory must be new');
  assert.ok(statSync(dirname(values.evidence)).isDirectory(), 'evidence parent must exist');
  const config = validateConfig(JSON.parse(readFileSync(values.config, 'utf8')));
  assert.ok(statSync(config.sshIdentity).isFile(), 'SSH identity must be a file');
  await deploy(config, values.slice, event => process.stdout.write(JSON.stringify(event) + '\n'));
  await main(args);
}

let entrypoint;
try { entrypoint = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined; }
catch { entrypoint = undefined; }
if (import.meta.url === entrypoint) {
  run().catch(error => { process.stderr.write(`${errorDetails(error)}\n`); process.exitCode = 1; });
}
