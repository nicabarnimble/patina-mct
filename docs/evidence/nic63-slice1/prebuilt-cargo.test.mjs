import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const evidence = dirname(fileURLToPath(import.meta.url));

function bundle(t) {
  const root = mkdtempSync(join(tmpdir(), 'nic63-dispatch-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'tools'));
  const cargo = join(root, 'tools/cargo');
  copyFileSync(join(evidence, 'prebuilt-cargo.sh'), cargo);
  chmodSync(cargo, 0o700);
  const artifacts = readFileSync(join(evidence, 'artifacts.tsv'), 'utf8');
  writeFileSync(join(root, 'artifacts.tsv'), artifacts);
  for (const row of artifacts.trim().split('\n')) {
    const [, relative] = row.split('\t');
    const binary = join(root, relative);
    mkdirSync(dirname(binary), { recursive: true });
    writeFileSync(binary, '#!/bin/sh\nprintf "<%s>\\n" "$@"\nexit 9\n', { mode: 0o700 });
  }
  return (target, filter, tail) => spawnSync(cargo,
    ['test', '--locked', '-p', ...target, filter, '--', ...tail],
    { encoding: 'utf8' });
}

const tails = [['--list'], ['--nocapture', '--test-threads=1']];
const targets = [
  ['mct-kernel', '--lib'],
  ['mct-observation', '--lib'],
  ['mct-daemon', '--lib'],
  ['mct-daemon', '--bin', 'mct-daemon'],
  ['mct-daemon', '--test', 'federation_fleet'],
  ['mct-daemon', '--test', 'mother_restart'],
];

for (const target of targets) {
  for (const tail of tails) {
    test(`preserves arguments and exit status: ${target.join(' ')} ${tail.join(' ')}`, (t) => {
      const result = bundle(t)(target, 'module::proof_case', tail);
      assert.equal(result.status, 9, result.stderr);
      assert.equal(result.stdout, ['module::proof_case', ...tail].map(arg => `<${arg}>\n`).join(''));
    });
  }
}

for (const filter of ['--ignored', '--help', '--list', '--nocapture', '--exact', '-Z']) {
  for (const tail of tails) {
    test(`refuses option-like filter before execution: ${filter} ${tail.join(' ')}`, (t) => {
      const result = bundle(t)(['mct-daemon', '--lib'], filter, tail);
      assert.equal(result.status, 64, result.stderr);
      assert.equal(result.stdout, '');
    });
  }
}

test('refuses --ignored after the separator', (t) => {
  const result = bundle(t)(['mct-daemon', '--lib'], 'proof_case', ['--ignored']);
  assert.equal(result.status, 64, result.stderr);
  assert.equal(result.stdout, '');
});
