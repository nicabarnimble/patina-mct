import assert from 'node:assert/strict';
import test from 'node:test';
import { stageCommand, cleanupStagedCommand, stagedPath } from './stage.mjs';
const root = '/home/operator/.mct/fleet-01234567-89ab-cdef-0123-456789abcdef/0';
const source = `/nix/store/${'0'.repeat(32)}-fleet/bin/mct-daemon`;
test('private staging preserves executable trust and checks the preflight digest', () => {
  const command = stageCommand(source, root, 'candidate', 'a'.repeat(64));
  assert.deepEqual(command.slice(-4), [source, `${root}-executables`, 'candidate', 'a'.repeat(64)]);
  for (const expected of ['install -m700', 'sha256sum', 'mkdir -m700', 'mct-federation-executables-v1', '8#$mode & 022']) assert.ok(command[2].includes(expected));
  assert.equal(stagedPath(root, 'candidate'), `${root}-executables/candidate/mct-daemon`);
  assert.throws(() => stageCommand('/tmp/untrusted', root, 'candidate', 'a'.repeat(64)));
  assert.throws(() => stageCommand(source, root, '../escape', 'a'.repeat(64)));
  assert.throws(() => cleanupStagedCommand('/home/operator'));
  assert.match(cleanupStagedCommand(root)[2], /mct-federation-executables-v1/);
});
