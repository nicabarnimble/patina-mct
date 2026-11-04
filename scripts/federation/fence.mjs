import assert from 'node:assert/strict';
import { isIP } from 'node:net';
import { setTimeout as sleep } from 'node:timers/promises';

const filters = [
  { attach_type: 'cgroup_inet_ingress', name: 'sd_fw_ingress' },
  { attach_type: 'cgroup_inet_egress', name: 'sd_fw_egress' },
];

export function createNetworkFence(config, unit) {
  assert.match(unit, /^mct-federation-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\.service$/, 'invalid fence unit');
  assert.ok(Array.isArray(config.hosts) && config.hosts.length > 0, 'missing fence hosts');
  const allow = config.hosts.map(({ address }) => {
    assert.equal(isIP(address), 4, 'fence requires IPv4 hosts');
    const [a, b] = address.split('.').map(Number);
    assert.ok(a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168), 'fence requires RFC1918 hosts');
    return `${address}/32`;
  });
  assert.equal(new Set(allow).size, allow.length, 'duplicate fence host');
  assert.ok(typeof config.package === 'string' && /^\/[a-zA-Z0-9/._+-]+$/.test(config.package) &&
    config.package.split('/').slice(1).every(part => part && part !== '.' && part !== '..'), 'invalid fence package');
  return {
    unit, allow, group: `/system.slice/${unit}`,
    bpftool: `${config.package}/libexec/bpftool`,
    properties: ['--property=IPAddressDeny=any', `--property=IPAddressAllow=${allow.join(' ')}`],
  };
}

function exactSet(value, expected) {
  const entries = value.trim() ? value.trim().split(/\s+/) : [];
  return entries.length === expected.length && new Set(entries).size === entries.length &&
    entries.every(entry => expected.includes(entry));
}

export function assertFenceProperties(fence, stdout) {
  assert.equal(typeof stdout, 'string', 'invalid fence properties');
  const properties = new Map();
  for (const line of stdout.trim().split(/\r?\n/)) {
    const match = /^([A-Za-z]+)=(.*)$/.exec(line);
    assert.ok(match && !properties.has(match[1]), 'malformed fence properties');
    properties.set(match[1], match[2]);
  }
  assert.ok(properties.get('LoadState') === 'loaded' && properties.get('ActiveState') === 'active' &&
    properties.get('SubState') === 'running', 'fence service is not running');
  assert.ok(properties.get('ControlGroup') === fence.group, 'fence cgroup does not match owned unit');
  assert.ok(exactSet(properties.get('IPAddressAllow') ?? '', fence.allow), 'fence allowlist does not match configured hosts');
  assert.ok(exactSet(properties.get('IPAddressDeny') ?? '', ['0.0.0.0/0', '::/0']), 'fence must deny all IPv4 and IPv6');
}

export function assertFenceAttachments(stdout) {
  let attachments;
  try { attachments = JSON.parse(stdout); } catch { throw Error('invalid fence attachment JSON'); }
  assert.ok(Array.isArray(attachments) && attachments.length === filters.length, 'fence requires two direct filters');
  const programs = filters.map(filter => {
    const matches = attachments.filter(entry => entry?.attach_type === filter.attach_type && entry?.name === filter.name);
    assert.ok(matches.length === 1 && Number.isSafeInteger(matches[0].id) && matches[0].id > 0, 'missing systemd fence filter');
    return { ...filter, id: matches[0].id };
  });
  assert.ok(programs[0].id !== programs[1].id, 'fence filter IDs must be distinct');
  return programs;
}

export async function inspectNetworkFence(fence, execute, { attempts = 20, retryMs = 250, wait = sleep } = {}) {
  assert.ok(Number.isSafeInteger(attempts) && attempts > 0 && attempts <= 20, 'invalid fence inspection attempts');
  assert.ok(Number.isFinite(retryMs) && retryMs >= 0 && retryMs <= 1000, 'invalid fence retry delay');
  for (let attempt = 0; attempt < attempts; attempt++) {
    try {
      const properties = await execute(['sudo', '-n', 'systemctl', 'show',
        '--property=LoadState,ActiveState,SubState,ControlGroup,IPAddressAllow,IPAddressDeny', '--', fence.unit]);
      assertFenceProperties(fence, properties.stdout);
      // Without "effective", bpftool reports only this cgroup's direct attachments.
      const attachments = await execute(['sudo', '-n', fence.bpftool, '-j', 'cgroup', 'show', `/sys/fs/cgroup${fence.group}`]);
      const programs = assertFenceAttachments(attachments.stdout);
      return {
        status: 'verified', scope: 'owned-transient-service', allowed_hosts: fence.allow.length,
        active_service: true, owned_cgroup: true, exact_host_allowlist: true,
        deny_ipv4: true, deny_ipv6: true, direct_filters: programs,
      };
    } catch {
      if (attempt + 1 === attempts) throw Error('network fence inspection failed; helper must not start a Mother');
      await wait(retryMs);
    }
  }
}
