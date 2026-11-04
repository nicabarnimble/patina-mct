import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { isIP } from 'node:net';
import { promisify } from 'node:util';
import { runCalls, sshArgs } from './fleet.mjs';

const exec = promisify(execFile);
const TABLE = /^mct63_[a-f0-9]{32}$/;
const lan = address => {
  if (isIP(address) !== 4) return false;
  const [a, b] = address.split('.').map(Number);
  return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168);
};

export function partitionRules(table, victim, peers) {
  assert.match(table, TABLE, 'invalid owned table');
  assert.ok(lan(victim.address) && Number.isInteger(victim.port) && victim.port > 0 && victim.port <= 65535, 'invalid victim socket');
  assert.ok(peers.length > 0 && peers.every(peer => lan(peer.address) && peer.address !== victim.address), 'invalid LAN peers');
  const addresses = peers.map(peer => peer.address).join(', ');
  return `table inet ${table} {
 comment "mct-federation-test"
 chain ingress {
  type filter hook input priority -5; policy accept;
  ip saddr { ${addresses} } udp dport ${victim.port} counter drop
 }
 chain egress {
  type filter hook output priority -5; policy accept;
  ip daddr { ${addresses} } udp sport ${victim.port} counter drop
 }
}\n`;
}

export function policyDigest(ruleset) {
  const clean = value => {
    if (Array.isArray(value)) return value.filter(item => !item?.metainfo).map(clean);
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).filter(([key]) => !['handle', 'packets', 'bytes', 'genid'].includes(key)).sort(([a], [b]) => a.localeCompare(b)).map(([key, entry]) => [key, clean(entry)]));
    return value;
  };
  return createHash('sha256').update(JSON.stringify(clean(ruleset))).digest('hex');
}

export function droppedPackets(value) {
  let count = 0;
  const visit = item => {
    if (!item || typeof item !== 'object') return;
    if (Object.hasOwn(item, 'counter')) {
      const packets = item.counter.packets;
      assert.ok(Number.isSafeInteger(packets) && packets >= 0, 'invalid packet counter');
      count += packets;
    }
    for (const child of Object.values(item)) visit(child);
  };
  visit(value); return count;
}

async function remote(fleet, host, command) {
  const { stdout } = await exec('ssh', sshArgs(fleet.config, host, command), { timeout: 60_000, maxBuffer: 4 * 1024 * 1024 });
  return stdout;
}

export async function runPartitions(fleet) {
  assert.equal(fleet.hosts.length, 3, 'partition acceptance needs three hosts');
  const nft = `${fleet.config.package}/libexec/nft`;
  const results = [];
  for (const victim of fleet.hosts) {
    const peers = fleet.hosts.filter(host => host !== victim);
    const table = `mct63_${randomUUID().replaceAll('-', '')}`;
    const unit = `${table}-restore`;
    const rules = partitionRules(table, victim, peers);
    const before = policyDigest(JSON.parse(await remote(fleet, victim, ['sudo', '-n', nft, '-j', 'list', 'ruleset'])));
    let armed = false; let failure; let result;
    try {
      await remote(fleet, victim, ['sudo', '-n', 'systemd-run', '--quiet', `--unit=${unit}`, '--on-active=180s', '--timer-property=AccuracySec=1s', nft, 'delete', 'table', 'inet', table]);
      armed = true;
      await remote(fleet, victim, ['bash', '-c', 'set -euo pipefail; if sudo -n "$1" list table inet "$2" >/dev/null 2>&1; then exit 72; fi; printf "%s" "$3" | sudo -n "$1" -f -', '--', nft, table, rules]);
      fleet.emit('partition-started', { victim: victim.node, rollback_seconds: 180 });
      await runCalls(fleet, peers, `partition-survivors-${victim.node}`);
      const [blocked, serving] = await Promise.allSettled([
        fleet.request(peers[0], { op: 'call', destination: victim.index, label: `partition-probe-${victim.node}` }),
        fleet.call(peers[1], peers[0], `partition-serving-${victim.node}`),
      ]);
      if (blocked.status === 'rejected') throw blocked.reason;
      if (serving.status === 'rejected') throw serving.reason;
      const reply = blocked.value;
      assert.ok(['failed', 'timed_out'].includes(reply.outcome), 'partition must cause transport failure, not completion or policy denial');
      assert.ok(reply.submitted_call_id && reply.submitted_trace_id, 'partition probe lacks correlation');
      assert.ok(reply.decoded_result == null && reply.inline_result_payload_base64 == null, 'partition probe returned execution bytes');
      const counters = JSON.parse(await remote(fleet, victim, ['sudo', '-n', nft, '-j', 'list', 'table', 'inet', table]));
      const packets = droppedPackets(counters);
      assert.ok(packets > 0, 'no actual dropped packets observed');
      fleet.calls.push({ caller: peers[0].node, destination: victim.node, label: 'partition-probe', outcome: reply.outcome, call_id: reply.submitted_call_id, trace_id: reply.submitted_trace_id, expect_no_execution: true });
      result = { victim: victim.node, dropped_packets: packets, blocked_outcome: reply.outcome, survivors_served: true };
    } catch (error) { failure = error; }
    finally {
      if (armed) {
        try {
          const listed = await remote(fleet, victim, ['sudo', '-n', nft, '-j', 'list', 'tables']);
          if (JSON.parse(listed).nftables.some(item => item.table?.family === 'inet' && item.table?.name === table)) {
            const owned = JSON.parse(await remote(fleet, victim, ['sudo', '-n', nft, '-j', 'list', 'table', 'inet', table]));
            assert.ok(owned.nftables.some(item => item.table?.name === table && item.table?.comment === 'mct-federation-test'), 'refusing to delete a foreign table');
            await remote(fleet, victim, ['sudo', '-n', nft, 'delete', 'table', 'inet', table]);
          }
          await remote(fleet, victim, ['sudo', '-n', 'systemctl', 'stop', `${unit}.timer`]);
          const after = policyDigest(JSON.parse(await remote(fleet, victim, ['sudo', '-n', nft, '-j', 'list', 'ruleset'])));
          assert.equal(after, before, 'pre-existing firewall policy changed');
        } catch (cleanup) { failure = new AggregateError([failure, cleanup].filter(Boolean), 'partition or rollback failed'); }
      }
    }
    if (failure) throw failure;
    await runCalls(fleet, fleet.hosts, `partition-healed-${victim.node}`, { refresh: true });
    results.push({ ...result, policy_restored: true, rejoined: true });
    fleet.emit('partition-passed', results.at(-1));
  }
  return { status: 'passed', method: 'temporary-test-udp-drops', victims: results };
}
