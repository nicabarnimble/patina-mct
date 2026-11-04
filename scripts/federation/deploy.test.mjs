import assert from 'node:assert/strict';
import test from 'node:test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { cacheCommand, cleanupCommand, importCommand, parseCacheReceipt } from './deploy.mjs';

const pkg = `/nix/store/${'0'.repeat(32)}-fleet`;
const key = `fleet-test:${Buffer.alloc(32, 7).toString('base64')}`;
test('cache preparation signs only the selected immutable package', () => {
  const command = cacheCommand(pkg);
  assert.equal(command.at(-1), pkg);
  assert.match(command[2], /readlink -f/);
  assert.match(command[2], /sudo -n "\$nix" store sign/);
  assert.match(command[2], /mktemp -d/);
  assert.match(command[2], /umask 077/);
  assert.doesNotMatch(command[2], /no-check-sigs|require-sigs false/);
});
test('import is offline from the copied cache and verifies signatures and content', () => {
  const command = importCommand(pkg, '/tmp/mct-import-safe', key);
  assert.deepEqual(command.slice(-3), [pkg, '/tmp/mct-import-safe', key]);
  assert.match(command[2], /max-jobs 0/);
  assert.match(command[2], /require-sigs true/);
  assert.match(command[2], /copy-sigs.*--substituter "file:\/\/\$cache"/);
  assert.ok(command[2].indexOf('copy-sigs') < command[2].indexOf('store verify'));
  assert.match(command[2], /readlink -f/);
  assert.match(command[2], /sudo -n "\$nix" store verify --recursive --offline/);
  assert.doesNotMatch(command[2], /no-check-sigs|require-sigs false|nix build/);
});
test('cleanup is confined to marked temporary transfer directories', () => {
  const root = '/home/operator/.mct-federation-import.ABC123xy';
  const command = cleanupCommand(root, 'import');
  assert.deepEqual(command.slice(-2), [root, 'import']);
  assert.match(command[2], /mct-federation-owned/);
  assert.match(command[2], /dirname/);
  for (const path of ['/home/operator', '/tmp/..', '/home/operator/.mct-federation-import.short', '/home/operator/.mct-federation-cache.ABC123xy']) assert.throws(() => cleanupCommand(path, 'import'));
  assert.throws(() => cleanupCommand(root, 'other'));
});
test('cache command handles a generated public key without a trailing newline', () => {
  const home = mkdtempSync(join(tmpdir(), 'mct-cache-test-'));
  const bin = join(home, 'bin'); mkdirSync(bin);
  const canonical = `/nix/store/${'0'.repeat(32)}-nix/bin/nix`;
  const stubs = {
    'nix-store': `printf '%s' '${key}' > "$4"; printf fake > "$3"`,
    nix: 'exit 0',
    readlink: `printf '%s\\n' '${canonical}'`,
    sudo: `test "$1" = -n && test "$2" = '${canonical}'`,
  };
  for (const [name, script] of Object.entries(stubs)) writeFileSync(join(bin, name), `#!/bin/sh\nset -eu\n${script}\n`, { mode: 0o755 });
  try {
    const [program, ...args] = cacheCommand(pkg);
    const stdout = execFileSync(program, args, { encoding: 'utf8', env: { HOME: home, PATH: `${bin}:/usr/bin:/bin` }, stdio: ['ignore', 'pipe', 'pipe'] });
    const receipt = parseCacheReceipt(stdout);
    assert.equal(receipt.publicKey, key);
    assert.ok(receipt.root.startsWith(`${home}/.mct-federation-cache.`));
  } finally { rmSync(home, { recursive: true }); }
});
test('untrusted receipts and paths fail closed', () => {
  assert.deepEqual(parseCacheReceipt(`/home/operator/.mct-federation-cache.abc12345\n${key}\n`), { root: '/home/operator/.mct-federation-cache.abc12345', publicKey: key });
  for (const text of ['', '/tmp/../root\nbad\n', `/tmp/cache\n${key}\nextra\n`, `/tmp/cache;id\n${key}\n`]) assert.throws(() => parseCacheReceipt(text));
  for (const path of ['/tmp/binary', `${pkg};id`, '/nix/store/short-package']) assert.throws(() => cacheCommand(path));
  assert.throws(() => importCommand(pkg, '/tmp/../root', key));
  assert.throws(() => importCommand(pkg, '/tmp/cache', key + '\nmalicious'));
});
