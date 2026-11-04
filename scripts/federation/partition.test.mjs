import assert from 'node:assert/strict';
import test from 'node:test';
import { partitionRules, policyDigest, droppedPackets } from './partition.mjs';
const victim = { address: '192.168.50.10', port: 24863 };
const peers = [{ address: '192.168.50.11' }, { address: '192.168.50.12' }];
const table = 'mct63_0123456789abcdef0123456789abcdef';
test('partition rules affect only the configured Mother UDP traffic', () => {
  const rules = partitionRules(table, victim, peers);
  assert.match(rules, /udp dport 24863 counter drop/);
  assert.match(rules, /udp sport 24863 counter drop/);
  assert.match(rules, /192\.168\.50\.11, 192\.168\.50\.12/);
  assert.equal((rules.match(/policy accept/g) ?? []).length, 2);
  assert.doesNotMatch(rules, /flush|tcp|policy drop|masquerade|dnat|snat/);
  assert.throws(() => partitionRules('other-table', victim, peers));
  assert.throws(() => partitionRules(table, { ...victim, port: '22; flush ruleset' }, peers));
  assert.throws(() => partitionRules(table, victim, [{ address: '1.1.1.1' }]));
});
test('policy digest excludes volatile counters and handles, not rule changes', () => {
  const before = { nftables: [{ metainfo: { version: '1', genid: 1 } }, { rule: { handle: 1, expr: [{ counter: { packets: 2, bytes: 7 } }, { accept: null }] } }] };
  const counters = structuredClone(before); counters.nftables[0].metainfo.genid = 4; counters.nftables[1].rule.handle = 9; counters.nftables[1].rule.expr[0].counter.packets = 100;
  assert.equal(policyDigest(before), policyDigest(counters));
  const changed = structuredClone(before); changed.nftables[1].rule.expr[1] = { drop: null };
  assert.notEqual(policyDigest(before), policyDigest(changed));
  assert.equal(droppedPackets(before), 2);
  assert.throws(() => droppedPackets({ counter: { packets: -1 } }));
});
