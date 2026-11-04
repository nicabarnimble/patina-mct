import test from 'node:test';
import assert from 'node:assert/strict';
import { parseArgs } from './run.mjs';

test('wrapper validates the complete CLI before any deployment', () => {
  const args = ['--config', '/tmp/fleet.json', '--slice', 'two', '--evidence', '/tmp/new-proof'];
  assert.deepEqual(parseArgs(args), { config: '/tmp/fleet.json', slice: 'two', evidence: '/tmp/new-proof' });
  for (const invalid of [args.slice(0, -1), [...args, '--unknown', 'x'], [...args, '--config', '/tmp/other'], ['--config', '/tmp/fleet.json', '--slice', 'typo', '--evidence', '/tmp/new'], ['--config', '/tmp/fleet.json', '--slice', 'two', '--evidence', 'relative']]) assert.throws(() => parseArgs(invalid));
});
