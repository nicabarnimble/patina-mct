import test from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync, spawnSync} from 'node:child_process';
import {mkdirSync, mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join, resolve} from 'node:path';

const POLICY = resolve(import.meta.dirname, 'push-policy.mjs');

function git(root, ...args) {
  return execFileSync('git', args, {cwd: root, encoding: 'utf8'}).trim();
}

function write(root, path, content) {
  mkdirSync(dirname(join(root, path)), {recursive: true});
  writeFileSync(join(root, path), content);
}

function repository(initial) {
  const root = mkdtempSync(join(tmpdir(), 'mct-push-policy-'));
  git(root, 'init', '-q');
  git(root, 'config', 'user.name', 'Policy Test');
  git(root, 'config', 'user.email', 'policy@example.test');
  for (const [path, content] of Object.entries(initial)) write(root, path, content);
  git(root, 'add', '.');
  git(root, 'commit', '-qm', 'base');
  return root;
}

function classify(root, base, head, route = 'direct') {
  return spawnSync(process.execPath, [POLICY, '--route', route, '--base', base, '--head', head],
    {cwd: root, encoding: 'utf8'});
}

function commit(root, message = 'change') {
  git(root, 'add', '-A');
  git(root, 'commit', '-qm', message);
  return git(root, 'rev-parse', 'HEAD');
}

test('direct-main accepts documentation changes', () => {
  const root = repository({'docs/guide.txt': 'before\n'});
  try {
    const base = git(root, 'rev-parse', 'HEAD');
    write(root, 'docs/guide.txt', 'after\n');
    const result = classify(root, base, commit(root));
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /direct-main lane accepted/);
  } finally { rmSync(root, {recursive: true, force: true}); }
});

test('direct-main refuses crate source and names the PR route', () => {
  const root = repository({'crates/example/src/lib.rs': 'pub fn value() -> u8 { 1 }\n'});
  try {
    const base = git(root, 'rev-parse', 'HEAD');
    write(root, 'crates/example/src/lib.rs', 'pub fn value() -> u8 { 2 }\n');
    const result = classify(root, base, commit(root));
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /crates\/example\/src\/lib\.rs/);
    assert.match(result.stderr, /open a PR/);
  } finally { rmSync(root, {recursive: true, force: true}); }
});

test('direct-main accepts a new test and an additions-only extension', () => {
  const root = repository({'scripts/example.test.mjs': "import test from 'node:test';\n"});
  try {
    let base = git(root, 'rev-parse', 'HEAD');
    write(root, 'scripts/new.test.mjs', "import test from 'node:test';\ntest('new', () => {});\n");
    let result = classify(root, base, commit(root, 'new test'));
    assert.equal(result.status, 0, result.stderr);

    base = git(root, 'rev-parse', 'HEAD');
    write(root, 'scripts/example.test.mjs', "import test from 'node:test';\ntest('added', () => {});\n");
    result = classify(root, base, commit(root, 'extend test'));
    assert.equal(result.status, 0, result.stderr);
  } finally { rmSync(root, {recursive: true, force: true}); }
});

test('direct-main refuses test deletion or an added assertion bypass', () => {
  const root = repository({'scripts/example.test.mjs': "import test from 'node:test';\ntest('kept', () => { assert(true); });\n"});
  try {
    let base = git(root, 'rev-parse', 'HEAD');
    write(root, 'scripts/example.test.mjs', "import test from 'node:test';\ntest('changed', () => { assert(true); });\n");
    let result = classify(root, base, commit(root, 'replace test'));
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /may not delete or replace/);

    base = git(root, 'rev-parse', 'HEAD');
    write(root, 'scripts/example.test.mjs', "import test from 'node:test';\ntest('changed', () => { assert(true); });\ntest('bypass', () => { return; assert(false); });\n");
    result = classify(root, base, commit(root, 'bypass'));
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /bypass assertions/);
  } finally { rmSync(root, {recursive: true, force: true}); }
});

test('PR route accepts high-risk paths and requires BOX verification', () => {
  const root = repository({'scripts/push-policy.mjs': 'before\n'});
  try {
    const base = git(root, 'rev-parse', 'HEAD');
    write(root, 'scripts/push-policy.mjs', 'after\n');
    const result = classify(root, base, commit(root), 'pr');
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /PR \+ BOX verify required/);
  } finally { rmSync(root, {recursive: true, force: true}); }
});
