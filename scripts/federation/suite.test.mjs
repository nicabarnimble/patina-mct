import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'mct-federation-suite-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, 'bin'); mkdirSync(bin);
  const config = join(root, 'fleet.json'); writeFileSync(config, '{}', { mode: 0o600 });
  writeFileSync(join(bin, 'node'), '#!/bin/sh\nprintf "%s\\n" "$5" >> "$CALL_LOG"\nif [ "$5" = "${FAIL_SLICE:-}" ]; then exit 7; fi\n', { mode: 0o700 });
  const evidence = join(root, 'evidence'); const log = join(root, 'calls');
  return { evidence, log, run(fail = '') { return spawnSync('bash', [fileURLToPath(new URL('./suite.sh', import.meta.url)), config, evidence], {
    encoding: 'utf8', env: { ...process.env, PATH: `${bin}:/usr/bin:/bin`, CALL_LOG: log, FAIL_SLICE: fail },
  }); } };
}
test('suite invokes every real CLI surface in order before reporting success', t => {
  const f = fixture(t); const result = f.run(); assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(f.log, 'utf8'), 'two\nthree\nupgrade\n');
  assert.equal(JSON.parse(readFileSync(join(f.evidence, 'result.json'))).status, 'passed');
});
test('suite stops at the first failed slice and cannot report success', t => {
  const f = fixture(t); assert.equal(f.run('three').status, 7);
  assert.equal(readFileSync(f.log, 'utf8'), 'two\nthree\n');
  assert.equal(existsSync(join(f.evidence, 'result.json')), false);
});
test('suite refuses evidence reuse before invoking any slice', t => {
  const f = fixture(t); mkdirSync(f.evidence);
  assert.notEqual(f.run().status, 0); assert.equal(existsSync(f.log), false);
});
