import test from 'node:test';
import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { EventEmitter } from 'node:events';
import { PassThrough, Writable } from 'node:stream';
import { readFileSync } from 'node:fs';

for (const method of ['spawn', 'spawnSync', 'exec', 'execSync', 'execFile', 'execFileSync', 'fork']) {
  childProcess[method] = () => { throw Error('fence unit tests forbid real subprocesses'); };
}
syncBuiltinESMExports();
const { createSshTransport } = await import('./fleet.mjs');
const { createNetworkFence, assertFenceProperties, assertFenceAttachments, inspectNetworkFence } = await import('./fence.mjs');

const config = {
  package: '/nix/store/candidate', baselinePackage: '/nix/store/baseline', sshIdentity: '/private/identity',
  hosts: ['10.10.10.15', '10.10.10.20', '10.10.10.21'].map((address, i) => ({ id: `node-${i}`, address, ssh: `pi@${address}` })),
};
const unit = 'mct-federation-12345678-1234-1234-1234-123456789abc.service';
const fence = createNetworkFence(config, unit);
const properties = (f = fence) => ({
  LoadState: 'loaded', ActiveState: 'active', SubState: 'running', ControlGroup: f.group,
  IPAddressAllow: [...f.allow].reverse().join(' '), IPAddressDeny: '0.0.0.0/0 ::/0',
});
const show = props => Object.entries(props).map(([key, value]) => `${key}=${value}`).join('\n') + '\n';
const attachments = () => [
  { id: 67, attach_type: 'cgroup_inet_ingress', attach_flags: 'multi', name: 'sd_fw_ingress', attach_btf_obj_id: 0, attach_btf_id: 0 },
  { id: 66, attach_type: 'cgroup_inet_egress', attach_flags: 'multi', name: 'sd_fw_egress', attach_btf_obj_id: 0, attach_btf_id: 0 },
];

test('unit: service properties contain one exact RFC1918 host allowlist and deny-any', () => {
  assert.deepEqual(fence.properties, ['--property=IPAddressDeny=any', '--property=IPAddressAllow=10.10.10.15/32 10.10.10.20/32 10.10.10.21/32']);
  assert.equal(fence.bpftool, '/nix/store/candidate/libexec/bpftool');
  assert.equal(fence.group, `/system.slice/${unit}`);
  for (const address of ['8.8.8.8', '100.64.0.1', '127.0.0.1', '169.254.1.1', '172.32.0.1', '192.169.0.1', '::1', '10.0.0.1/24', '10.0.0.1\n']) {
    assert.throws(() => createNetworkFence({ ...config, hosts: [{ address }] }, unit));
  }
  for (const address of ['10.0.0.1', '172.16.0.1', '172.31.255.254', '192.168.0.1']) {
    assert.equal(createNetworkFence({ ...config, hosts: [{ address }] }, unit).allow[0], `${address}/32`);
  }
  for (const hosts of [[], [config.hosts[0], config.hosts[0]]]) assert.throws(() => createNetworkFence({ ...config, hosts }, unit));
  for (const name of ['other.service', `${unit}/child`, `../${unit}`, `${unit}\n`]) assert.throws(() => createNetworkFence(config, name));
  for (const path of ['/nix/store/../candidate', '/nix/store/candidate/', '/nix/store/candidate;evil', 'relative']) {
    assert.throws(() => createNetworkFence({ ...config, package: path }, unit));
  }
});

test('unit: observed active owned cgroup and unordered exact allow/deny sets pass', () => {
  assertFenceProperties(fence, show(properties()));
  assertFenceProperties(fence, show({ ...properties(), IPAddressDeny: '::/0 0.0.0.0/0' }));
});

const propertyFailures = [
  ['absent allow', p => { delete p.IPAddressAllow; }],
  ['empty allow', p => { p.IPAddressAllow = ''; }],
  ['widened allow', p => { p.IPAddressAllow = p.IPAddressAllow.replace('/32', '/24'); }],
  ['extra allow', p => { p.IPAddressAllow += ' 127.0.0.1/32'; }],
  ['missing host', p => { p.IPAddressAllow = fence.allow[0]; }],
  ['duplicate allow', p => { p.IPAddressAllow += ` ${fence.allow[0]}`; }],
  ['wrong host', p => { p.IPAddressAllow = p.IPAddressAllow.replace('10.10.10.21', '10.10.10.22'); }],
  ['absent deny', p => { delete p.IPAddressDeny; }],
  ['IPv4 only deny', p => { p.IPAddressDeny = '0.0.0.0/0'; }],
  ['IPv6 only deny', p => { p.IPAddressDeny = '::/0'; }],
  ['requested deny alias', p => { p.IPAddressDeny = 'any'; }],
  ['partial deny', p => { p.IPAddressDeny = '0.0.0.0/1 ::/0'; }],
  ['wrong cgroup', p => { p.ControlGroup = '/system.slice/unrelated.service'; }],
  ['descendant cgroup', p => { p.ControlGroup += '/child'; }],
  ['missing cgroup', p => { delete p.ControlGroup; }],
  ['inactive', p => { p.ActiveState = 'inactive'; }],
  ['activating', p => { p.ActiveState = 'activating'; }],
  ['exited', p => { p.SubState = 'exited'; }],
  ['not loaded', p => { p.LoadState = 'not-found'; }],
];
for (const [name, mutate] of propertyFailures) test(`unit: properties reject ${name}`, () => {
  const p = properties(); mutate(p); assert.throws(() => assertFenceProperties(fence, show(p)));
});

test('unit: malformed and duplicate properties cannot mask an unsafe state', () => {
  for (const output of ['', 'not properties', show(properties()) + 'IPAddressDeny=\n', show(properties()) + 'ActiveState=inactive\n']) {
    assert.throws(() => assertFenceProperties(fence, output));
  }
});

test('unit: kernel direct systemd filter IDs prove both directions without BTF', () => {
  assert.deepEqual(assertFenceAttachments(JSON.stringify(attachments().reverse())), [
    { attach_type: 'cgroup_inet_ingress', name: 'sd_fw_ingress', id: 67 },
    { attach_type: 'cgroup_inet_egress', name: 'sd_fw_egress', id: 66 },
  ]);
});
const attachmentFailures = [
  ['absent filters', () => []],
  ['ingress missing', a => a.slice(1)],
  ['egress missing', a => a.slice(0, 1)],
  ['no-op names', a => a.map(p => ({ ...p, name: 'allow_all' }))],
  ['wrong ingress type', a => { a[0].attach_type = 'cgroup_skb'; return a; }],
  ['wrong egress name', a => { a[1].name = 'sd_fw_ingress'; return a; }],
  ['zero ID', a => { a[0].id = 0; return a; }],
  ['negative ID', a => { a[1].id = -1; return a; }],
  ['string ID', a => { a[0].id = '67'; return a; }],
  ['fractional ID', a => { a[0].id = 1.5; return a; }],
  ['duplicate ID', a => { a[0].id = a[1].id; return a; }],
  ['extra filter', a => [...a, { ...a[0], id: 90 }]],
  ['duplicate direction', a => [a[0], a[0]]],
  ['null filter', a => [null, a[1]]],
];
for (const [name, mutate] of attachmentFailures) test(`unit: attachments reject ${name}`, () => {
  assert.throws(() => assertFenceAttachments(JSON.stringify(mutate(attachments()))));
});
for (const output of ['not json', '{}', 'null', 'true']) test(`unit: attachments reject ${output}`, () => {
  assert.throws(() => assertFenceAttachments(output));
});

test('unit: inspection retries startup and queries only owned direct attachments using candidate bpftool', async () => {
  const calls = []; const waits = [];
  const proof = await inspectNetworkFence(fence, async command => {
    calls.push(command);
    if (calls.length === 1) throw Error('not yet loaded');
    if (command.includes('systemctl')) return { stdout: show(properties()) };
    return { stdout: JSON.stringify(attachments()), stderr: 'Warning: kernel BTF missing' };
  }, { attempts: 3, retryMs: 1, wait: async ms => { waits.push(ms); } });
  assert.equal(calls.length, 3); assert.deepEqual(waits, [1]);
  assert.deepEqual(calls[2], ['sudo', '-n', '/nix/store/candidate/libexec/bpftool', '-j', 'cgroup', 'show', `/sys/fs/cgroup${fence.group}`]);
  assert.ok(!calls.flat().includes('effective'));
  assert.ok(calls[1].includes(unit));
  assert.equal(proof.status, 'verified'); assert.equal(proof.allowed_hosts, 3);
  for (const secret of [unit, fence.group, config.package, config.sshIdentity, ...config.hosts.map(h => h.address)]) {
    assert.ok(!JSON.stringify(proof).includes(secret));
  }
});

test('unit: inspection exhaustion is bounded and does not expose remote errors', async () => {
  let calls = 0; let waits = 0;
  await assert.rejects(inspectNetworkFence(fence, async () => {
    calls++; throw Error(`SSH ${config.sshIdentity} ${config.hosts[0].address}`);
  }, { attempts: 3, retryMs: 0, wait: async () => { waits++; } }), error => {
    assert.match(error.message, /network fence inspection failed/);
    assert.ok(!error.message.includes(config.sshIdentity)); return true;
  });
  assert.equal(calls, 3); assert.equal(waits, 2);
  for (const attempts of [0, -1, 21, 1.5]) await assert.rejects(inspectNetworkFence(fence, async () => {}, { attempts }));
});

function transportDouble({ changeProperties, changeAttachments, inspectError, cleanupFailure, onFence, beforeInspect } = {}) {
  const state = { writes: [], commands: [], closed: false, killed: false, stops: 0, inspections: 0, proofs: [] };
  const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
  child.stdin = new Writable({ write(chunk, _, done) {
    state.writes.push(chunk.toString());
    child.stdout.write(JSON.stringify({ ok: true, value: { started: true } }) + '\n'); done();
  } });
  child.stdin.on('finish', () => { state.closed = true; queueMicrotask(() => child.emit('exit', 0, null)); });
  child.kill = () => { state.killed = true; child.emit('exit', null, 'SIGTERM'); };
  const transport = createSshTransport({
    requireNetworkFence: true,
    spawn: (file, args) => {
      assert.equal(file, 'ssh'); state.commands.push(args.at(-1));
      state.unit = /--unit=(mct-federation-[a-f0-9-]+\.service)/.exec(args.at(-1))[1];
      state.fence = createNetworkFence(config, state.unit); return child;
    },
    exec: async (file, args, options) => {
      assert.equal(file, 'ssh'); const command = args.at(-1); state.commands.push(command);
      if (command.includes('load_state()')) {
        state.stops++; assert.ok(command.includes(state.unit));
        if (cleanupFailure === 'throw') throw Error('cleanup transport failed');
        return { stdout: cleanupFailure ? 'active\n' : 'absent\n', stderr: '' };
      }
      state.inspections++;
      assert.equal(options.timeout, 5000); assert.equal(state.writes.length, 0);
      if (beforeInspect) await beforeInspect(state, child);
      if (inspectError) throw Error('inspection unavailable');
      if (command.includes("'systemctl'")) {
        const p = properties(state.fence); changeProperties?.(p);
        return { stdout: show(p), stderr: '' };
      }
      assert.ok(command.includes(`'${config.package}/libexec/bpftool'`));
      assert.ok(command.includes(`'/sys/fs/cgroup${state.fence.group}'`));
      assert.ok(command.startsWith("'sudo' '-n'")); assert.ok(!command.includes('effective'));
      const a = changeAttachments ? changeAttachments(attachments()) : attachments();
      return { stdout: JSON.stringify(a), stderr: 'Warning: kernel BTF missing' };
    },
    onFence: async (node, proof) => { state.proofs.push({ node, proof }); if (onFence) await onFence(node, proof); },
  });
  return { transport, state };
}

test('unit: fenced transport cannot return or send helper requests until kernel inspection completes', async () => {
  let release; const gate = new Promise(resolve => { release = resolve; });
  const { transport, state } = transportDouble({ beforeInspect: () => gate });
  let returned = false;
  const pending = transport.connect(config, config.hosts[0]).then(session => { returned = true; return session; });
  await Promise.resolve(); assert.equal(returned, false); assert.deepEqual(state.writes, []);
  const command = state.commands[0];
  assert.equal((command.match(/--property=IPAddressAllow=/g) ?? []).length, 1);
  assert.ok(command.includes("'--property=IPAddressAllow=10.10.10.15/32 10.10.10.20/32 10.10.10.21/32'"));
  assert.ok(command.includes("'--property=IPAddressDeny=any'"));
  assert.ok(command.includes("'--uid=pi'"));
  release(); const session = await pending;
  assert.equal(state.proofs.length, 1); assert.equal(state.proofs[0].node, config.hosts[0].id);
  assert.equal(state.proofs[0].proof.status, 'verified');
  assert.deepEqual(await session.request({ op: 'start' }), { started: true });
  await session.close(); assert.equal(state.stops, 1); assert.ok(state.closed && state.killed);
});

test('unit: injected transport fails closed and cleans its owned unit on adversarial inspection', { concurrency: true }, async t => {
  const cases = [
    ['guards absent', { changeProperties: p => { delete p.IPAddressAllow; delete p.IPAddressDeny; } }],
    ['allow widened', { changeProperties: p => { p.IPAddressAllow += ' 0.0.0.0/0'; } }],
    ['incorrect cgroup', { changeProperties: p => { p.ControlGroup += '/other'; } }],
    ['inactive unit', { changeProperties: p => { p.ActiveState = 'inactive'; } }],
    ['one direction missing', { changeAttachments: a => a.slice(0, 1) }],
    ['no-op BPF state', { changeAttachments: () => [] }],
    ['inspection error', { inspectError: true }],
    ['cleanup unconfirmed', { inspectError: true, cleanupFailure: true }],
    ['cleanup throws', { inspectError: true, cleanupFailure: 'throw' }],
  ];
  await Promise.all(cases.map(([name, options]) => t.test(name, { concurrency: true }, async () => {
    const { transport, state } = transportDouble(options);
    await assert.rejects(transport.connect(config, config.hosts[0]), error => {
      assert.match(error.message, options.cleanupFailure ? /network fence failed and owned unit cleanup failed/ : /network fence inspection failed/);
      if (options.cleanupFailure) { assert.ok(error instanceof AggregateError); assert.equal(error.errors.length, 2); }
      return true;
    });
    assert.equal(state.stops, 1); assert.ok(state.closed && state.killed);
    assert.deepEqual(state.writes, []); assert.deepEqual(state.proofs, []);
    assert.ok(state.inspections >= 20 && state.inspections <= 40);
  })));
});

test('unit: failed helper or failed proof publication closes the fenced session', async () => {
  for (const options of [
    { beforeInspect: (_, child) => child.emit('exit', 1, null) },
    { onFence: () => { throw Error('proof sink unavailable'); } },
  ]) {
    const { transport, state } = transportDouble(options);
    await assert.rejects(transport.connect(config, config.hosts[0]), /helper failed|proof sink unavailable/);
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(state.stops, 1); assert.ok(state.closed && state.killed); assert.deepEqual(state.writes, []);
  }
});

test('unit: unfenced transport does not inspect or add guards merely because a baseline is configured', async () => {
  let command; let executions = 0; let proofs = 0;
  const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
  child.stdin = new Writable({ write(_, __, done) { done(); } });
  child.stdin.on('finish', () => child.emit('exit', 0, null));
  child.kill = () => child.emit('exit', null, 'SIGTERM');
  const transport = createSshTransport({
    spawn: (_, args) => { command = args.at(-1); return child; },
    exec: async () => { executions++; return { stdout: 'absent\n' }; },
    onFence: () => { proofs++; },
  });
  const session = await transport.connect(config, config.hosts[0]);
  assert.ok(!command.includes('IPAddress')); assert.equal(executions, 0); assert.equal(proofs, 0);
  await session.close(); assert.equal(executions, 1);
});

test('unit: absent subprocess injection fails locally before any real process', async () => {
  await assert.rejects(createSshTransport({ requireNetworkFence: true }).connect(config, config.hosts[0]), /forbid real subprocesses/);
});

test('unit: main requires the fence only for upgrade and publishes the proof stage', () => {
  const source = readFileSync(new URL('./fleet.mjs', import.meta.url), 'utf8');
  assert.match(source, /requireNetworkFence: values\['--slice'\] === 'upgrade'/);
  assert.match(source, /onFence: \(node, proof\) => emit\(\{ stage: 'network_fence', node, proof \}\)/);
});
