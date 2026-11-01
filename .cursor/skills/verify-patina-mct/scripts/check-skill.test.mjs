import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const skill = path.resolve('.cursor/skills/verify-patina-mct');
const scripts = path.join(skill, 'scripts');
const helper = path.join(scripts, 'verify-patina-mct.sh');

const fakeCommand = String.raw`#!/usr/bin/env node
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const command = path.basename(process.argv[1]);
const args = process.argv.slice(2);
fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ command, args }) + '\n');
function fail(message, code = 1) { console.error(message); process.exit(code); }
function json(value) { console.log(JSON.stringify(value)); }
switch (command) {
  case 'uname':
    if (process.env.TEST_UNAME_EXIT) process.exit(Number(process.env.TEST_UNAME_EXIT));
    console.log(args[0] === '-s' ? process.env.TEST_OS : process.env.TEST_ARCH);
    break;
  case 'stat': fail('nonportable stat invocation', 97); break;
  case 'nix': process.exit(Number(process.env.TEST_NIX_EXIT || 0)); break;
  case 'bash': {
    if (args[0] === 'scripts/ci-tier0.sh' && args[1] === 'specs') process.exit(0);
    const result = spawnSync('/bin/bash', args, { stdio: 'inherit' });
    process.exit(result.status ?? 1);
    break;
  }
  case 'cargo':
    if (args[0] === 'test') {
      if (process.env.TMPDIR !== fs.realpathSync(process.env.TMPDIR)) fail('noncanonical TMPDIR', 69);
      process.exit(Number(process.env.TEST_TEST_EXIT || 0));
    }
    break;
  default:
    if (args[0] === 'version') console.log(process.env.TEST_VERSION || 'mct-daemon 0.2.0');
    else if (args[0] === 'help') console.log('passkey-admin');
    else if (args[0] === 'install') {
      if (process.env.TEST_OS === 'Darwin') fail('unsafe Darwin install invoked', 99);
      fail('supervisor lifecycle is supported only on macOS');
    } else if (args[0] === 'status') fail('resident not running at verified control socket');
    else if (args[0] === 'not-a-command') fail("unknown command 'not-a-command'");
    else if (args[1] === 'day-keygen') {
      const output = args[args.indexOf('--secret-output') + 1];
      if (fs.existsSync(output)) fail('refusing to replace an existing day-key secret');
      fs.writeFileSync(output, 'test secret', { flag: 'wx', mode: Number(process.env.TEST_SECRET_MODE || 384) });
      json({ scheme: 'ed25519', public_key: 'a'.repeat(64) });
    } else if (args[1] === 'prepare-genesis') {
      json({ intent: { mother_node_id: 'mother-verify', ledger_id: 'ledger-verify', action: { action: 'bootstrap' } }, challenge: 'test' });
    } else fail('unexpected fixture command');
}
`;

function fixture(t) {
  const base = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'verify-skill-test-')));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const home = path.join(base, 'home');
  const bin = path.join(base, 'bin');
  const root = path.join(base, 'run');
  for (const directory of [home, bin, path.join(base, 'tmp')]) fs.mkdirSync(directory, { mode: 0o700 });
  fs.symlinkSync(path.join(base, 'tmp'), path.join(base, 'tmp-alias'));
  for (const name of ['mct-daemon', 'cargo', 'stat', 'uname', 'nix', 'bash']) {
    fs.writeFileSync(path.join(bin, name), fakeCommand, { mode: 0o700 });
  }
  const env = {
    ...process.env, HOME: home, PATH: `${bin}:${process.env.PATH}`,
    TMPDIR: path.join(base, 'tmp-alias'), VERIFY_PATINA_MCT_ROOT: root,
    VERIFY_PATINA_MCT_BIN: path.join(bin, 'mct-daemon'),
    TEST_LOG: path.join(base, 'commands.jsonl'), TEST_OS: 'Darwin', TEST_ARCH: 'arm64',
  };
  return {
    base, home, bin, root, env,
    run(command, overrides = {}) {
      return spawnSync('/bin/bash', [helper, command], {
        env: { ...env, ...overrides }, encoding: 'utf8', timeout: 30000,
      });
    },
    calls() {
      if (!fs.existsSync(env.TEST_LOG)) return [];
      return fs.readFileSync(env.TEST_LOG, 'utf8').trim().split('\n').map(JSON.parse);
    },
    rows(name) {
      return fs.readFileSync(path.join(root, 'proof', name), 'utf8').trimEnd().split('\n').map(line => line.split('\t'));
    },
  };
}

function success(result) {
  assert.equal(result.status, 0, result.stdout + result.stderr + (result.error || ''));
}

function refused(result) {
  assert.equal(result.signal, null);
  assert.notEqual(result.status, 0, result.stdout + result.stderr);
}

test('skill structure, links, and Pi discovery are valid', () => {
  const text = fs.readFileSync(path.join(skill, 'SKILL.md'), 'utf8');
  assert.match(text, /^---\n/);
  const frontmatter = text.split('---')[1];
  assert.match(frontmatter, /^name:\s*verify-patina-mct\s*$/m);
  assert.match(frontmatter, /^description:\s*\S/m);
  for (const heading of ['Launch', 'Doctor', 'Drive', 'Evidence', 'Cleanup', 'Helpers']) {
    assert.ok(text.includes(`## ${heading}`), heading);
  }
  const features = path.join(skill, 'features');
  const index = fs.readFileSync(path.join(features, 'README.md'), 'utf8');
  const links = [...index.matchAll(/\]\(\.\/([^)]+)\)/g)].map(match => match[1]);
  assert.equal(links.length, 5);
  assert.deepEqual([...links].sort(), fs.readdirSync(features).filter(name => name !== 'README.md').sort());
  const files = [path.join(skill, 'SKILL.md'), ...fs.readdirSync(features).map(name => path.join(features, name))];
  for (const file of files) {
    const content = fs.readFileSync(file, 'utf8');
    assert.doesNotMatch(content, /[\u2013\u2014]/);
    assert.doesNotMatch(content, /\b(chrome|chromium|firefox|safari|webkit|edge|brave)\b/i);
    assert.doesNotMatch(content, /This VM does not/);
    for (const token of ['~/.mct', '$HOME/.mct', '.mct-pr', '.mct-passkey', '.mct-approval']) {
      assert.ok(!content.includes(token), token);
    }
    for (const match of content.matchAll(/\]\(([^)]+)\)/g)) {
      if (!match[1].includes('://')) assert.ok(fs.existsSync(path.resolve(path.dirname(file), match[1].split('#')[0])), match[1]);
    }
    if (path.dirname(file) === features && path.basename(file) !== 'README.md') {
      assert.deepEqual(content.match(/^## .+$/gm), [
        '## Sub-features', '## How to get to it (user POV)', '## Driving it with verify-patina-mct', '## Gotchas',
      ]);
    }
  }
  for (const name of ['check-skill.sh', 'verify-patina-mct.sh']) {
    assert.ok(fs.statSync(path.join(scripts, name)).mode & 0o111, name);
  }
  const repo = path.resolve(skill, '../../..');
  const link = path.join(repo, '.pi/skills/verify-patina-mct');
  assert.ok(fs.lstatSync(link).isSymbolicLink());
  assert.equal(fs.realpathSync(link), fs.realpathSync(skill));
});

test('doctor permits a read-only binary inside home', t => {
  const f = fixture(t);
  const binary = path.join(f.home, 'mct-daemon');
  fs.copyFileSync(f.env.VERIFY_PATINA_MCT_BIN, binary);
  success(f.run('doctor', { VERIFY_PATINA_MCT_BIN: binary }));
});

test('doctor rejects a missing binary and a wrong version on stock Bash', t => {
  const f = fixture(t);
  refused(f.run('doctor', { VERIFY_PATINA_MCT_BIN: path.join(f.base, 'missing') }));
  refused(f.run('doctor', { TEST_VERSION: 'mct-daemon 9.9.9' }));
});

for (const osName of ['Darwin', 'Linux']) {
  test(`${osName} drive produces TSV and never installs on Darwin`, t => {
    const f = fixture(t);
    success(f.run('drive-admin', { TEST_OS: osName }));
    const rows = f.rows('drive-results.tsv');
    assert.equal(rows.length, 7);
    assert.ok(rows.every(row => row.length === 4), JSON.stringify(rows));
    const install = rows.find(row => row[0] === 'install');
    assert.equal(install[1], osName === 'Darwin' ? 'manual' : 'auto');
    assert.equal(install[2], osName === 'Darwin' ? 'not-run' : '1');
    assert.equal(f.calls().filter(call => call.args[0] === 'install').length, osName === 'Darwin' ? 0 : 1);
    assert.ok(!fs.existsSync(path.join(f.root, 'scratch/day-key.json')));
  });
}

test('failed host detection never invokes install', t => {
  const f = fixture(t);
  refused(f.run('drive-admin', { TEST_UNAME_EXIT: '3' }));
  assert.equal(f.calls().filter(call => call.args[0] === 'install').length, 0);
});

test('drive refuses a secret with the wrong permissions', t => {
  const f = fixture(t);
  refused(f.run('drive-admin', { TEST_SECRET_MODE: String(0o400) }));
});

for (const alias of ['direct', 'symlink', 'dotdot']) {
  test(`home state is refused through ${alias} paths before deletion`, t => {
    const f = fixture(t);
    const root = path.join(f.home, 'run');
    fs.mkdirSync(path.join(root, 'scratch'), { recursive: true });
    fs.mkdirSync(path.join(root, 'proof'));
    const sentinel = path.join(root, 'scratch/sentinel');
    fs.writeFileSync(sentinel, 'keep');
    fs.symlinkSync(root, path.join(f.base, 'alias'));
    const selected = { direct: root, symlink: path.join(f.base, 'alias'), dotdot: `${f.base}/tmp/../home/run` }[alias];
    for (const command of ['cleanup', 'drive-admin', 'doctor']) {
      refused(f.run(command, { VERIFY_PATINA_MCT_ROOT: selected }));
      assert.equal(fs.readFileSync(sentinel, 'utf8'), 'keep');
    }
    assert.equal(f.calls().length, 0);
  });
}

for (const root of ['', 'relative', '/', '/.']) {
  test(`invalid root ${JSON.stringify(root)} is refused before any command`, t => {
    const f = fixture(t);
    const result = f.run('doctor', { VERIFY_PATINA_MCT_ROOT: root });
    refused(result);
    assert.match(result.stderr, /must be absolute|broad run directory/);
    assert.equal(f.calls().length, 0);
  });
}

test('root symlinks outside home are refused', t => {
  const f = fixture(t);
  success(f.run('doctor'));
  const alias = path.join(f.base, 'alias');
  fs.symlinkSync(f.root, alias);
  const result = f.run('cleanup', { VERIFY_PATINA_MCT_ROOT: alias });
  refused(result);
  assert.match(result.stderr, /symlink run directory/);
  assert.ok(fs.existsSync(path.join(f.root, 'scratch')));
});

test('a private root without a marker cannot be adopted', t => {
  const f = fixture(t);
  fs.mkdirSync(f.root, { mode: 0o700 });
  refused(f.run('doctor'));
  assert.deepEqual(fs.readdirSync(f.root), []);
});

test('unmarked roots are refused without chmod or deletion', t => {
  const f = fixture(t);
  fs.mkdirSync(f.root, { mode: 0o755 });
  fs.mkdirSync(path.join(f.root, 'proof'));
  fs.mkdirSync(path.join(f.root, 'scratch'));
  const sentinel = path.join(f.root, 'scratch/sentinel');
  fs.writeFileSync(sentinel, 'keep');
  const mode = fs.statSync(f.root).mode;
  for (const command of ['cleanup', 'drive-admin', 'doctor']) {
    refused(f.run(command));
    assert.equal(fs.readFileSync(sentinel, 'utf8'), 'keep');
    assert.equal(fs.statSync(f.root).mode, mode);
  }
});

for (const kind of ['symlink', 'hardlink']) {
  test(`proof ${kind} cannot overwrite an unrelated file`, t => {
    const f = fixture(t);
    success(f.run('doctor'));
    const outside = path.join(f.base, 'outside');
    fs.writeFileSync(outside, 'keep', { mode: 0o600 });
    const proof = path.join(f.root, 'proof/version.txt');
    fs.unlinkSync(proof);
    if (kind === 'symlink') fs.symlinkSync(outside, proof);
    else fs.linkSync(outside, proof);
    refused(f.run('doctor'));
    assert.equal(fs.readFileSync(outside, 'utf8'), 'keep');
  });
}

test('scratch symlinks are refused before reset and cleanup', t => {
  const f = fixture(t);
  success(f.run('doctor'));
  const outside = path.join(f.base, 'outside');
  fs.mkdirSync(outside);
  fs.writeFileSync(path.join(outside, 'sentinel'), 'keep');
  fs.rmSync(path.join(f.root, 'scratch'), { recursive: true });
  fs.symlinkSync(outside, path.join(f.root, 'scratch'));
  refused(f.run('drive-admin'));
  refused(f.run('cleanup'));
  assert.equal(fs.readFileSync(path.join(outside, 'sentinel'), 'utf8'), 'keep');
});

for (const field of ['version', 'uid']) {
  test(`a wrong marker ${field} is refused`, t => {
    const f = fixture(t);
    success(f.run('doctor'));
    const marker = path.join(f.root, '.verify-patina-mct.json');
    const value = JSON.parse(fs.readFileSync(marker, 'utf8'));
    value[field] += 1;
    fs.writeFileSync(marker, JSON.stringify(value));
    const result = f.run('cleanup');
    refused(result);
    assert.match(result.stderr, /marker does not match/);
    assert.ok(fs.existsSync(path.join(f.root, 'scratch')));
  });
}

for (const kind of ['public-file', 'fifo', 'file-instead-of-directory']) {
  test(`unsafe ${kind} is refused before writes`, t => {
    const f = fixture(t);
    success(f.run('doctor'));
    const target = path.join(f.root, 'scratch', 'unsafe');
    if (kind === 'fifo') {
      success(spawnSync('mkfifo', [target], { encoding: 'utf8' }));
      fs.chmodSync(target, 0o600);
    } else if (kind === 'public-file') {
      fs.writeFileSync(target, 'keep');
      fs.chmodSync(target, 0o644);
    } else {
      fs.rmSync(path.join(f.root, 'scratch'), { recursive: true });
      fs.writeFileSync(path.join(f.root, 'scratch'), 'keep', { mode: 0o600 });
    }
    refused(f.run('doctor'));
  });
}

test('a copied ownership marker does not authorize another root', t => {
  const f = fixture(t);
  success(f.run('doctor'));
  const copy = path.join(f.base, 'copy');
  fs.cpSync(f.root, copy, { recursive: true });
  for (const name of ['', 'proof', 'scratch', 'scratch/mother']) fs.chmodSync(path.join(copy, name), 0o700);
  const result = f.run('cleanup', { VERIFY_PATINA_MCT_ROOT: copy });
  refused(result);
  assert.match(result.stderr, /marker does not match/);
  assert.ok(fs.existsSync(path.join(copy, 'scratch')));
});

test('cleanup is repeatable and retains proof, then a new drive works', t => {
  const f = fixture(t);
  success(f.run('drive-admin'));
  const proof = fs.readFileSync(path.join(f.root, 'proof/drive-results.tsv'));
  success(f.run('cleanup'));
  success(f.run('cleanup'));
  assert.ok(!fs.existsSync(path.join(f.root, 'scratch')));
  assert.deepEqual(fs.readFileSync(path.join(f.root, 'proof/drive-results.tsv')), proof);
  success(f.run('drive-admin'));
});

test('gates canonicalize TMPDIR and propagate a failing test', t => {
  const f = fixture(t);
  success(f.run('gates'));
  assert.equal(f.rows('gates-results.tsv').length, 5);
  assert.equal(f.run('gates', { TEST_TEST_EXIT: '17' }).status, 1);
  assert.equal(f.rows('gates-results.tsv').find(row => row[0] === 'test')[2], '17');
});

test('supported host without Nix is skipped with exit 127', t => {
  const f = fixture(t);
  fs.unlinkSync(path.join(f.bin, 'nix'));
  fs.symlinkSync(process.execPath, path.join(f.bin, 'node'));
  fs.symlinkSync('/usr/bin/dirname', path.join(f.bin, 'dirname'));
  assert.equal(f.run('nix-smoke', { TEST_OS: 'Linux', TEST_ARCH: 'aarch64', PATH: f.bin }).status, 127);
  assert.deepEqual(f.rows('nix-results.tsv')[0], ['nix-smoke', 'out-of-vm', '127']);
});

for (const [osName, arch, exit, classification, expected] of [
  ['Darwin', 'arm64', '0', 'out-of-vm', 2],
  ['Linux', 'x86_64', '0', 'out-of-vm', 2],
  ['Linux', 'aarch64', '0', 'auto', 0],
  ['Linux', 'aarch64', '23', 'auto', 23],
]) {
  test(`Nix classification for ${osName}/${arch} with child exit ${exit}`, t => {
    const f = fixture(t);
    const result = f.run('nix-smoke', { TEST_OS: osName, TEST_ARCH: arch, TEST_NIX_EXIT: exit });
    assert.equal(result.status, expected, result.stderr);
    assert.deepEqual(f.rows('nix-results.tsv')[0].slice(0, 3), ['nix-smoke', classification, String(expected)]);
    assert.equal(f.calls().filter(call => call.command === 'nix').length, classification === 'auto' ? 1 : 0);
  });
}
