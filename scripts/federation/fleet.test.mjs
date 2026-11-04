import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import childProcess, { spawnSync } from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
import { PassThrough, Writable } from 'node:stream';
for (const method of ['spawn', 'execFile']) childProcess[method] = () => {
  throw Error('tests forbid live transport subprocesses; inject a fake transport');
};
syncBuiltinESMExports();
const {
  validateConfig, unwrapReply, assertCall, assertStart, assertVerification,
  JsonLineSession, sshArgs, createSshTransport, createFleet, runCalls, runSlice, sanitize,
} = await import('./fleet.mjs');

const config = () => ({
  builder: 'a', sshIdentity: '/private/fixture-key', package: '/nix/store/candidate',
  baselinePackage: '/nix/store/baseline', runRoot: '/tmp/fleet', vision: 'vision-fleet',
  hosts: JSON.parse(readFileSync(new URL('./config.example.json', import.meta.url), 'utf8')).hosts.map(h => ({ ...h, hostname: `host-${h.id}` })),
});
const host = (id = 'a', index = 0) => ({ ...config().hosts[index], id, node: id, index, endpoint: `endpoint-${id}` });
const completed = (destination = host('b', 1), local = false) => ({
  outcome: 'completed', decoded_result: { results: [41 + destination.index] },
  inline_result_payload_base64: Buffer.from(JSON.stringify({ results: [41 + destination.index] })).toString('base64'),
  route_taken: { node_id: destination.node, runtime_kind: local ? 'wasm_component' : 'remote_peer' },
  submitted_call_id: 'call-1', submitted_trace_id: 'trace-1',
});
const started = h => ({
  pid: 100, owned_mother_count: 1, binary: '/nix/store/candidate/bin/mct-daemon', binary_sha256: 'c'.repeat(64), status: { readiness: 'ready', resident: { node_id: h.node, vision_id: 'vision-fleet' },
    iroh_endpoint: { endpoint_id: h.endpoint, relay_mode: 'disabled', relay_urls: [] } },
  ticket: { endpoint_id: h.endpoint, direct_addresses: [`${h.address}:${h.port}`], relay_urls: [] },
});
const verified = h => ({
  verified: true, endpoint_id: h.endpoint, ledger_bytes: 100, ledger_sha256: 'a'.repeat(64),
  node_id: h.node, prefix_preserved: true, previous_checkpoint_bytes: null,
  verification: { verified: true, head: { ledger_id: 'ledger-local', mother_node_id: h.node, local_sequence: 0 }, entry_count: 1, prefix_blake3: 'a'.repeat(64) },
  observations: [],
});

test('config validates the exact public shape and puts builder first', () => {
  const c = config(); c.builder = 'c';
  assert.deepEqual(validateConfig(c).hosts.map(h => h.id), ['c', 'a', 'b']);
});
for (const [name, mutate] of [
  ['missing key', c => { delete c.sshIdentity; }],
  ['relative key', c => { c.sshIdentity = 'key'; }],
  ['unsafe root', c => { c.runRoot = '/tmp/../'; }],
  ['root directory', c => { c.runRoot = '/'; }],
  ['missing builder', c => { c.builder = 'z'; }],
  ['duplicate id', c => { c.hosts[1].id = 'a'; }],
  ['duplicate hostname', c => { c.hosts[1].hostname = c.hosts[0].hostname.toUpperCase(); }],
  ['duplicate address', c => { c.hosts[1].address = c.hosts[0].address; }],
  ['public address', c => { c.hosts[0].address = '203.0.113.1'; }],
  ['mismatched LAN SSH target', c => { c.hosts[0].ssh = `tester@${c.hosts[1].address}`; }],
  ['public SSH target', c => { c.hosts[0].ssh = 'tester@203.0.113.1'; }],
  ['DNS SSH target', c => { c.hosts[0].ssh = 'tester@builder.example.com'; }],
  ['Tailscale SSH target', c => { c.hosts[0].ssh = 'tester@100.64.0.1'; }],
  ['Tailscale address', c => { c.hosts[0].address = '100.64.0.1'; c.hosts[0].ssh = 'tester@100.64.0.1'; }],
  ['loopback', c => { c.hosts[0].address = '127.0.0.1'; }],
  ['link local', c => { c.hosts[0].address = '169.254.1.1'; }],
  ['IPv6', c => { c.hosts[0].address = '::1'; }],
  ['port', c => { c.hosts[0].port = 65536; }],
  ['fractional port', c => { c.hosts[0].port = 2.5; }],
  ['ssh option', c => { c.hosts[0].ssh = '-oProxyCommand=bad'; }],
  ['ssh injection', c => { c.hosts[0].ssh = 'x@host;true'; }],
  ['node injection', c => { c.hosts[0].id = 'a;true'; }],
  ['hostname injection', c => { c.hosts[0].hostname = 'host\nother'; }],
  ['malformed ssh hostname', c => { c.hosts[0].ssh = 'tester@bad..host'; }],
  ['undefined vision', c => { c.vision = undefined; }],
  ['native-unsafe package path', c => { c.package = '/nix/store/unsafe package'; }],
  ['unknown key', c => { c.sshOptions = ['-oStrictHostKeyChecking=no']; }],
]) test(`config rejects ${name}`, () => {
  const c = config(); mutate(c); assert.throws(() => validateConfig(c));
});

test('helper envelopes fail closed', () => {
  assert.deepEqual(unwrapReply({ ok: true, value: { stopped: true } }), { stopped: true });
  for (const value of [null, {}, { ok: false, error: 'bad' }, { ok: 'true', value: 1 }, { ok: true }, { ok: true, value: 1, error: 'bad' }]) {
    assert.throws(() => unwrapReply(value));
  }
});

test('completed calls require decoded bytes, actual remote route and fresh correlation fields', () => {
  assertCall(completed(), host(), host('b', 1));
  for (const mutate of [
    r => { r.outcome = 'denied'; }, r => { r.outcome = 'timed_out'; },
    r => { r.decoded_result.results = [41]; }, r => { r.route_taken.node_id = 'a'; },
    r => { r.route_taken.runtime_kind = 'wasm_component'; },
    r => { delete r.submitted_call_id; }, r => { delete r.submitted_trace_id; },
    r => { r.inline_result_payload_base64 = Buffer.from('{"results":[99]}').toString('base64'); },
  ]) { const r = completed(); mutate(r); assert.throws(() => assertCall(r, host(), host('b', 1))); }
  assertCall({ outcome: 'denied', submitted_call_id: 'd', submitted_trace_id: 'dt', route_taken: null }, host(), host('b', 1), 'denied');
  for (const outcome of ['failed', 'timed_out', 'completed']) {
    assert.throws(() => assertCall({ ...completed(), outcome }, host(), host('b', 1), 'denied'));
  }
});

test('startup enforces LAN-only readiness, exact ticket and stable identity', () => {
  assertStart(started(host()), host(), 'vision-fleet');
  for (const mutate of [
    r => { r.status.iroh_endpoint.relay_mode = 'default'; },
    r => { r.status.iroh_endpoint.relay_urls = ['https://relay.invalid']; },
    r => { r.ticket.direct_addresses.push('127.0.0.1:1234'); },
    r => { r.ticket.endpoint_id = 'changed'; },
    r => { r.status.resident.node_id = 'other'; },
    r => { r.status.resident.vision_id = 'other'; },
    r => { r.status.readiness = 'starting'; },
    r => { delete r.owned_mother_count; }, r => { r.owned_mother_count = 2; },
  ]) { const r = started(host()); mutate(r); assert.throws(() => assertStart(r, host(), 'vision-fleet')); }
});

test('ledger verification rejects missing verification, shrinkage, changed identity and broken prefixes', () => {
  assertVerification(verified(host()), host());
  for (const mutate of [
    r => { r.verified = false; }, r => { r.prefix_preserved = false; },
    r => { r.endpoint_id = 'other'; }, r => { r.ledger_sha256 = 'not-a-hash'; },
    r => { r.verification.head.mother_node_id = 'other'; },
    r => { r.verification.verified = false; },
    r => { r.observations = [{ detail_ref: 'executed_on:other;forwarded_from:a' }]; },
  ]) { const r = verified(host()); mutate(r); assert.throws(() => assertVerification(r, host())); }
  assert.throws(() => assertVerification(verified(host()), { ...host(), checkpoint: { ledger_bytes: 101 } }));
});

test('reduced observations retain ordered zero-based canonical source sequences, including gaps', () => {
  const reply = verified(host());
  reply.verification.entry_count = 8; reply.verification.head.local_sequence = 7;
  reply.observations = [0, 3, 7].map(local_sequence => ({ local_sequence }));
  assertVerification(reply, host());
  for (const sequences of [[-1], [0, 0], [3, 0], [8], [1.5], [undefined]]) {
    const changed = structuredClone(reply);
    changed.observations = sequences.map(local_sequence => ({ local_sequence }));
    assert.throws(() => assertVerification(changed, host()), /sequence/);
  }
});

function processDouble(onLine) {
  const child = new EventEmitter();
  child.stdout = new PassThrough(); child.stderr = new PassThrough();
  let buffer = '';
  child.stdin = new Writable({ write(chunk, _, done) {
    buffer += chunk;
    while (buffer.includes('\n')) {
      const index = buffer.indexOf('\n'); const line = buffer.slice(0, index); buffer = buffer.slice(index + 1);
      onLine(JSON.parse(line), child);
    }
    done();
  } });
  child.stdin.on('finish', () => queueMicrotask(() => child.emit('exit', 0, null)));
  child.kill = () => child.emit('exit', null, 'SIGTERM');
  return child;
}

test('persistent session serializes JSON requests and accepts fragmented replies', async () => {
  const requests = [];
  const child = processDouble((request, p) => {
    requests.push(request);
    const reply = JSON.stringify({ ok: true, value: request });
    setTimeout(() => { p.stdout.write(reply.slice(0, 4)); p.stdout.write(reply.slice(4) + '\n'); }, 2);
  });
  const session = new JsonLineSession(child, { timeoutMs: 100 });
  assert.deepEqual(await Promise.all([session.request({ op: 'one' }), session.request({ op: 'two' })]), [{ op: 'one' }, { op: 'two' }]);
  assert.equal(requests.length, 2); await session.close();
});
for (const [name, reply] of [
  ['malformed JSON', p => p.stdout.write('not json\n')],
  ['helper error', p => p.stdout.write('{"ok":false,"error":"bad"}\n')],
  ['EOF', p => p.emit('exit', 1, null)],
  ['oversized response', p => p.stdout.write('x'.repeat(1025))],
  ['timeout', () => {}],
]) test(`session fails closed on ${name}`, async () => {
  const session = new JsonLineSession(processDouble((_, p) => reply(p)), { timeoutMs: 10, maxLineBytes: 1024 });
  await assert.rejects(session.request({ op: 'start' }));
  await assert.rejects(session.request({ op: 'stop' }));
  await session.close();
});

test('transport tests fail locally if subprocess injection is missing', async () => {
  const transport = createSshTransport();
  await assert.rejects(transport.connect(config(), host()), /tests forbid live transport/);
  await assert.rejects(transport.preflight(config(), host()), /tests forbid live transport/);
});

test('transport owns a unique supervised unit and independently stops and confirms it', async () => {
  const commands = []; const children = [];
  const transport = createSshTransport({
    spawn: (file, args) => {
      assert.equal(file, 'ssh'); commands.push(args.at(-1));
      const child = processDouble((request, p) => p.stdout.write(JSON.stringify({ ok: true, value: request }) + '\n'));
      children.push(child); return child;
    },
    exec: async (file, args) => { assert.equal(file, 'ssh'); commands.push(args.at(-1)); return { stdout: 'inactive\n', stderr: '' }; },
  });
  const sessions = await Promise.all([transport.connect(config(), host()), transport.connect(config(), host())]);
  for (const command of commands) {
    for (const flag of ['sudo', '-n', 'systemd-run', '--quiet', '--pipe', '--wait', '--collect', '--property=KillMode=control-group', '--property=TimeoutStopSec=35s', '--property=RuntimeMaxSec=900s']) assert.ok(command.includes(`'${flag}'`), flag);
    assert.ok(command.includes(`'--uid=${host().ssh.split('@')[0]}'`));
  }
  const units = commands.map(c => /--unit=(mct-federation-[a-f0-9-]+\.service)/.exec(c)?.[1]);
  assert.ok(units.every(Boolean)); assert.notEqual(units[0], units[1]);
  assert.deepEqual(await sessions[0].request({ op: 'peers' }), { op: 'peers' });
  children[0].emit('exit', 1, null);
  await sessions[0].close(); await sessions[0].close(); await sessions[1].close();
  assert.equal(commands.length, 4);
  for (let i = 0; i < 2; i++) {
    assert.ok(commands[i + 2].includes(units[i])); assert.ok(!commands[i + 2].includes(units[1 - i]));
    assert.match(commands[i + 2], /systemctl.*stop/s); assert.match(commands[i + 2], /systemctl.*show/s);
  }
});

for (const output of ['active\n', 'failed\n', '']) test(`supervised cleanup rejects unconfirmed state ${JSON.stringify(output)}`, async () => {
  const transport = createSshTransport({ spawn: () => processDouble(() => {}), exec: async () => ({ stdout: output, stderr: '' }) });
  const session = await transport.connect(config(), host());
  await assert.rejects(session.close(), /inactive|cleanup/);
  await assert.rejects(session.close());
});

test('supervised cleanup accepts confirmed absent unit', async () => {
  const transport = createSshTransport({ spawn: () => processDouble(() => {}), exec: async () => ({ stdout: 'absent\n', stderr: '' }) });
  const session = await transport.connect(config(), host()); await session.close();
});

test('supervised cleanup exposes transport failure', async () => {
  const transport = createSshTransport({ spawn: () => processDouble(() => {}), exec: async () => { throw Error('stop SSH unavailable'); } });
  const session = await transport.connect(config(), host());
  await assert.rejects(session.close(), /stop SSH unavailable/);
});

test('preflight requires supervisor permissions and hashes a physical boot identity', async () => {
  let command;
  const transport = createSshTransport({ exec: async (_, args) => {
    command = args.at(-1); return { stdout: `host-a\n${'d'.repeat(64)}  -\n${'c'.repeat(64)}  -\n${'b'.repeat(64)}  -\n`, stderr: '' };
  } });
  const proof = await transport.preflight(config(), host());
  assert.equal(proof.physical_id, 'd'.repeat(64));
  assert.match(command, /\/proc\/sys\/kernel\/random\/boot_id/);
  assert.match(command, /systemd-run/); assert.match(command, /sudo -n/);
});

test('SSH uses strict nonmultiplexed authentication and quotes remote arguments', () => {
  const args = sshArgs(config(), config().hosts[0], ['/path/with space/it\'s-helper']);
  for (const option of ['BatchMode=yes', 'IdentitiesOnly=yes', 'StrictHostKeyChecking=yes', 'ControlMaster=no', 'ControlPath=none']) assert.ok(args.includes(option));
  assert.match(args.at(-1), /^'\/path\/with space\/it'"'"'s-helper'$/);
});

function fakeTransport(overrides = {}) {
  const state = { requests: [], running: new Set(), crashed: new Set(), sessions: [], observations: new Map(), binaries: new Map(), checkpoints: new Map(), serial: 0, revoked: false };
  const transport = {
    async stageBinary(c, h, source, kind) { return `${h.root}-executables/${kind}/mct-daemon`; },
    async cleanupBinaries() {},
    async preflight(c, h) { return { hostname: h.hostname, physical_id: `${h.id}`.repeat(64), candidate_sha256: 'c'.repeat(64), baseline_sha256: 'b'.repeat(64) }; },
    async connect(c, h) {
      state.sessions.push(h.id); state.observations.set(h.id, []);
      return { async request(r) {
        state.requests.push({ host: h.id, ...r });
        if (overrides.request) {
          const value = await overrides.request(h, r, state); if (value !== undefined) return value;
        }
        if (r.op === 'fixture') return { index: r.index, component_base64: 'YQ==', manifest: 'fixture', component_blake3: 'a'.repeat(64), manifest_sha256: 'b'.repeat(64) };
        if (r.op === 'init') { state.binaries.set(h.id, r.binary); return {}; }
        if (r.op === 'start') {
          state.running.add(h.id); state.crashed.delete(h.id);
          if (r.binary) state.binaries.set(h.id, r.binary);
          const binary = state.binaries.get(h.id);
          return { ...started({ ...h, endpoint: `endpoint-${h.id}` }), binary, binary_sha256: (binary.includes('baseline') ? 'b' : 'c').repeat(64) };
        }
        if (r.op === 'stop') {
          state.running.delete(h.id); if (r.crash) state.crashed.add(h.id);
          return { stopped: true, mode: r.crash ? 'crash' : 'graceful', escalated: false, recovery_required: state.crashed.has(h.id) };
        }
        if (r.op === 'provision') return { verification_outcome: 'verified', approval_state: 'approved' };
        if (r.op === 'control') {
          if (r.path === '/receiver-authority') return { policy_revision: 1 };
          if (r.path === '/peers/add') state.observations.get(h.id).push({ kind: 'peer_admitted', outcome: 'allowed', subject_id: r.body.binding_id });
          if (r.path === '/peers/revoke') { state.revoked = true; state.observations.get(h.id).push({ kind: 'peer_binding_revoked', outcome: 'allowed', subject_id: r.body.peer_node_id, resource_id: `${h.id}-admits-${r.body.peer_node_id}` }); }
          return {};
        }
        if (r.op === 'peers') return Object.fromEntries(config().hosts.filter(p => p.id !== h.id).map(p => [p.id, {
          binding_id: `${h.id}-admits-${p.id}`, policy_revision: 1,
          binding_signature_ref: `mct-ed25519-binding-v1:${'a'.repeat(128)}`, expires_at: '2099-01-01T00:00:00Z',
        }]));
        if (r.op === 'call') {
          assert.match(r.label, /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$/);
          const destination = config().hosts[r.destination];
          assert.ok(state.running.has(h.id)); assert.ok(state.running.has(destination.id));
          const id = `call-${++state.serial}`; const trace = `trace-${state.serial}`;
          if (state.revoked && h.id === 'a' && destination.id === 'b') {
            state.denied = { call_id: id, trace_id: trace };
            state.observations.get(h.id).push({ kind: 'peer_call_sent', call_id: id, trace_id: trace, detail_ref: `forwarded_from:a;forwarded_to:b;operation:patina:fleet-n1/work@0.1.0.step` });
            state.observations.get('b').push({ kind: 'peer_rejected', outcome: 'denied', subject_id: null, trace_id: trace, detail_ref: 'hello_reason:BindingRevoked' });
            return { outcome: 'denied', route_taken: null, submitted_call_id: id, submitted_trace_id: trace };
          }
          const result = { ...completed({ node: destination.id, index: r.destination }, h.id === destination.id), submitted_call_id: id, submitted_trace_id: trace };
          if (h.id !== destination.id) {
            state.observations.get(h.id).push({ kind: 'peer_call_sent', call_id: id, trace_id: trace, detail_ref: `forwarded_from:${h.id};forwarded_to:${destination.id};operation:patina:fleet-n${r.destination}/work@0.1.0.step` });
          }
          const executionId = h.id === destination.id ? id : `mct-peer-execution-v1:${id}`;
          if (h.id !== destination.id) state.observations.get(destination.id).push({
            kind: 'call_constructed', call_id: executionId, trace_id: trace,
            peer_correlation: { caller_call_id: id, peer_binding_id: `${destination.id}-admits-${h.id}`, caller_node_id: h.id, caller_vision_id: c.vision },
          });
          state.observations.get(destination.id).push({ kind: 'runtime_execution_completed', outcome: 'completed', source_plane: 'child', call_id: executionId, trace_id: trace, detail_ref: `executed_on:${destination.id};forwarded_from:${h.id};operation:patina:fleet-n${r.destination}/work@0.1.0.step` });
          return result;
        }
        if (r.op === 'verify') {
          assert.ok(!state.running.has(h.id)); assert.ok(!state.crashed.has(h.id), 'unrecovered torn tail cannot be verified');
          const reply = { ...verified({ node: h.node, endpoint: `endpoint-${h.id}` }), ledger_bytes: 100 + state.serial,
            previous_checkpoint_bytes: state.checkpoints.get(h.id) ?? null, observations: state.observations.get(h.id).map((o, i) => ({ ...o, local_sequence: i })) };
          overrides.reduce?.(h, reply.observations, state);
          reply.verification.entry_count = reply.observations.length;
          reply.verification.head.local_sequence = reply.observations.length - 1;
          state.checkpoints.set(h.id, reply.ledger_bytes); return reply;
        }
        throw Error(`unexpected ${r.op}`);
      }, async close() { state.running.delete(h.id); } };
    },
  };
  return { transport: { ...transport, ...overrides.transport }, state };
}

test('mocked two-host lifecycle accepts distinct receiver IDs with real-shaped correlation facts', async () => {
  const { transport, state } = fakeTransport();
  const stages = [];
  const result = await runSlice(config(), 'two', { transport, onStage: s => stages.push(s), pollMs: 0 });
  assert.equal(result.status, 'passed'); assert.equal(result.partition.status, 'not_run');
  assert.equal(state.running.size, 0);
  assert.deepEqual(state.requests.filter(r => r.op === 'fixture').map(r => [r.host, r.index]), [['a', 0], ['a', 1]]);
  assert.equal(state.requests.filter(r => r.path === '/peers/proof').length, 2);
  assert.equal(state.requests.filter(r => r.op === 'init').length, 2);
  assert.ok(stages.some(s => s.stage === 'revocation'));
  assert.equal(result.directions.length, 2);
  assert.ok(result.directions.every(d => d.executed > 0 && d.forwarded > 0));
  for (const ledger of result.ledgers) for (const o of ledger.observations.filter(o => o.peer_correlation)) {
    assert.notEqual(o.call_id, o.peer_correlation.caller_call_id);
    assert.ok(ledger.observations.some(e => e.kind === 'runtime_execution_completed' && e.call_id === o.call_id && e.trace_id === o.trace_id));
  }
});

for (const [name, mutate] of [
  ['missing', (o, all) => { all.splice(all.indexOf(o), 1); }],
  ['wrong caller call', o => { o.peer_correlation.caller_call_id = 'other'; }],
  ['wrong trace', o => { o.trace_id = 'other'; }],
  ['wrong caller node', o => { o.peer_correlation.caller_node_id = 'other'; }],
  ['wrong caller vision', o => { o.peer_correlation.caller_vision_id = 'other'; }],
  ['wrong binding', o => { o.peer_correlation.peer_binding_id = 'other'; }],
  ['wrong receiver ID', o => { o.call_id = 'other'; }],
  ['caller ID reused as receiver ID', (o, all) => {
    all.find(e => e.call_id === o.call_id && e.kind === 'runtime_execution_completed').call_id = o.peer_correlation.caller_call_id;
    o.call_id = o.peer_correlation.caller_call_id;
  }],
  ['wrong record kind', o => { o.kind = 'peer_call_sent'; }],
  ['empty field', o => { o.peer_correlation.caller_node_id = ''; }],
  ['missing field', o => { delete o.peer_correlation.caller_node_id; }],
  ['nonstring field', o => { o.peer_correlation.caller_node_id = 1; }],
  ['execution before correlation', (o, all) => { all.push(...all.splice(all.indexOf(o), 1)); }],
  ['duplicate', (o, all) => { all.splice(all.indexOf(o) + 1, 0, structuredClone(o)); }],
  ['second receiver ID', (o, all) => { all.splice(all.indexOf(o) + 1, 0, { ...structuredClone(o), call_id: 'another-minted-id' }); }],
]) test(`remote correlation rejects ${name}`, async () => {
  const { transport } = fakeTransport({ reduce(h, observations) {
    if (h.id !== 'b') return;
    mutate(observations.find(o => o.peer_correlation), observations);
    observations.forEach((o, i) => { o.local_sequence = i; });
  } });
  await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0 }));
});

for (const [name, mutate] of [
  ['remote wrong operation', (h, all) => { if (h.id === 'b') all.find(o => o.detail_ref?.startsWith('executed_on:b;forwarded_from:a;')).detail_ref = 'executed_on:b;forwarded_from:a;operation:wrong'; }],
  ['local wrong operation', (h, all) => { if (h.id === 'a') all.find(o => o.detail_ref?.startsWith('executed_on:a;forwarded_from:a;')).detail_ref = 'executed_on:a;forwarded_from:a;operation:wrong'; }],
  ['wrong executor source', (h, all) => { all.filter(o => o.kind === 'runtime_execution_completed').forEach(o => { o.source_plane = 'peer'; }); }],
  ['wrong local ID', (h, all) => { if (h.id === 'a') all.find(o => o.detail_ref?.startsWith('executed_on:a;forwarded_from:a;')).call_id = 'other'; }],
  ['wrong remote trace', (h, all) => { if (h.id === 'b') all.find(o => o.detail_ref?.startsWith('executed_on:b;forwarded_from:a;')).trace_id = 'other'; }],
  ['duplicate remote execution', (h, all) => { if (h.id === 'b') all.push({ ...all.find(o => o.detail_ref?.startsWith('executed_on:b;forwarded_from:a;')) }); }],
  ['caller local execution of remote operation', (h, all) => { if (h.id === 'a') {
    const sent = all.find(o => o.kind === 'peer_call_sent');
    all.push({ ...sent, kind: 'runtime_execution_completed', outcome: 'completed', source_plane: 'child', detail_ref: 'executed_on:a;forwarded_from:a;operation:patina:fleet-n1/work@0.1.0.step' });
  } }],
  ['denied trace executed under minted ID', (h, all, state) => { if (h.id === 'b') all.push({ kind: 'runtime_execution_completed', outcome: 'completed', call_id: 'illegal-minted', trace_id: state.denied.trace_id, detail_ref: 'executed_on:b;forwarded_from:a;operation:patina:fleet-n1/work@0.1.0.step' }); }],
  ['denied correlated ID executed under different trace', (h, all, state) => { if (h.id === 'b') all.push(
    { kind: 'call_constructed', call_id: 'illegal-minted', trace_id: state.denied.trace_id, peer_correlation: { caller_call_id: state.denied.call_id, peer_binding_id: 'b-admits-a', caller_node_id: 'a', caller_vision_id: 'vision-fleet' } },
    { kind: 'runtime_execution_failed', call_id: 'illegal-minted', trace_id: 'different', detail_ref: 'executed_on:b' }); }],
  ['missing rejection', (h, all) => { const i = all.findIndex(o => o.kind === 'peer_rejected'); if (i >= 0) all.splice(i, 1); }],
  ['unrelated rejection trace', (h, all) => { all.filter(o => o.kind === 'peer_rejected').forEach(o => { o.trace_id = 'unrelated'; }); }],
  ['unrelated rejection binding', (h, all) => { all.filter(o => o.kind === 'peer_rejected').forEach(o => { o.subject_id = 'b-admits-other'; }); }],
  ['unrelated revoked binding', (h, all) => { all.filter(o => o.kind === 'peer_binding_revoked').forEach(o => { o.resource_id = 'b-admits-other'; }); }],
  ['non-revocation reason', (h, all) => { all.filter(o => o.kind === 'peer_rejected').forEach(o => { o.detail_ref = 'hello_reason:MissingBinding'; }); }],
  ['missing revocation reason', (h, all) => { all.filter(o => o.kind === 'peer_rejected').forEach(o => { delete o.detail_ref; }); }],
  ['rejection before revocation', (h, all) => { const i = all.findIndex(o => o.kind === 'peer_rejected'); if (i >= 0) all.unshift(...all.splice(i, 1)); }],
]) test(`ledger rejects ${name}`, async () => {
  const { transport } = fakeTransport({ reduce(h, all, state) {
    mutate(h, all, state); all.forEach((o, i) => { o.local_sequence = i; });
  } });
  await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0 }), name.startsWith('denied') ? /denied call executed/ : undefined);
});

for (const kind of ['runtime_execution_started', 'runtime_execution_completed', 'runtime_execution_failed', 'runtime_execution_timed_out']) {
  for (const correlation of ['trace', 'id']) test(`denial rejects markerless ${kind} by ${correlation}`, async () => {
    const { transport } = fakeTransport({ reduce(h, all, state) {
      if (h.id !== 'b') return;
      if (correlation === 'id') all.push({ kind: 'call_constructed', call_id: 'denied-execution', trace_id: state.denied.trace_id,
        peer_correlation: { caller_call_id: state.denied.call_id, peer_binding_id: 'b-admits-a', caller_node_id: 'a', caller_vision_id: 'vision-fleet' } });
      all.push({ kind, call_id: 'denied-execution', trace_id: correlation === 'trace' ? state.denied.trace_id : 'other' });
      all.forEach((o, i) => { o.local_sequence = i; });
    } });
    await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0 }), /denied call executed/);
  });
}

test('receiver revocation cannot pass on caller no_route fallback', async () => {
  const { transport } = fakeTransport({ request(h, r, state) {
    if (r.op === 'call' && state.revoked) return { outcome: 'denied', protocol_reason: 'no_route', submitted_call_id: 'fallback', submitted_trace_id: 'fallback-trace' };
  } });
  await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0 }));
});

test('private staging failure prevents every init and launch', async () => {
  const cleaned = [];
  const { transport, state } = fakeTransport({ transport: {
    async stageBinary() { throw Error('copy digest mismatch'); },
    async cleanupBinaries(c, h) { cleaned.push(h.id); },
  } });
  await assert.rejects(runSlice(config(), 'two', { transport }), /staging/);
  assert.ok(!state.requests.some(request => ['init', 'start'].includes(request.op)));
  assert.deepEqual(cleaned.sort(), ['a', 'b']);
});

test('three-host lifecycle calls both survivors while each victim is stopped, then resumes without re-admission', async () => {
  const { transport, state } = fakeTransport();
  const result = await runSlice(config(), 'three', { transport, pollMs: 0 });
  assert.equal(result.status, 'incomplete');
  assert.equal(state.requests.filter(r => r.path === '/peers/add').length, 6);
  for (const victim of ['a', 'b', 'c']) {
    const calls = state.requests.filter(r => r.op === 'call' && r.label.includes(`down-${victim}`));
    assert.equal(calls.length, 2); assert.ok(calls.every(r => r.host !== victim));
  }
  assert.equal(state.requests.filter(r => r.op === 'verify').length, 9);
  assert.equal(state.requests.filter(r => r.op === 'stop' && r.crash).length, 3);
  for (const [i, request] of state.requests.entries()) if (request.op === 'stop' && request.crash) {
    const next = state.requests.slice(i + 1).find(r => r.host === request.host && ['start', 'verify'].includes(r.op));
    assert.equal(next.op, 'start', 'crash recovery must precede canonical verification');
  }
});

test('rolling replacement records distinct hashes and calls survivors and mixed versions', async () => {
  const { transport, state } = fakeTransport();
  const result = await runSlice(config(), 'upgrade', { transport, pollMs: 0 });
  assert.equal(result.status, 'passed');
  assert.ok(result.binaries.every(b => b.candidate_sha256 !== b.baseline_sha256));
  assert.equal(state.requests.filter(r => r.op === 'start' && r.binary?.endsWith('/candidate/mct-daemon')).length, 3);
  assert.ok(state.requests.some(r => r.op === 'call' && r.label.includes('mixed-a')));
});

test('duplicate observed physical identities fail before connecting or starting', async () => {
  const { transport, state } = fakeTransport({ transport: { async preflight(c, h) {
    return { hostname: h.hostname, physical_id: 'd'.repeat(64), candidate_sha256: 'c'.repeat(64) };
  } } });
  await assert.rejects(createFleet(config(), 'two', { transport }), /duplicate physical/);
  assert.equal(state.sessions.length, 0); assert.equal(state.requests.length, 0);
});

test('normal lifecycle stops drain and crash verification requires restart', async () => {
  const { transport, state } = fakeTransport(); const stages = [];
  const fleet = await createFleet(config(), 'two', { transport, onStage: s => stages.push(s) });
  assert.ok(state.requests.filter(r => r.op === 'stop').every(r => !r.crash));
  await fleet.stop(fleet.hosts[0], { crash: true });
  await assert.rejects(fleet.verify(fleet.hosts[0]), /recovery|restart/);
  await fleet.start(fleet.hosts[0]); await fleet.stop(fleet.hosts[0]); await fleet.verify(fleet.hosts[0]);
  assert.ok(stages.some(s => s.stage === 'stopped' && s.mode === 'graceful' && s.escalated === false));
  await fleet.close();
});

test('cleanup failure remains visible alongside a scenario failure without an event sink', async () => {
  const fake = fakeTransport(); const connect = fake.transport.connect;
  fake.transport.connect = async (...args) => {
    const session = await connect(...args); session.close = async () => { throw Error('owned unit still active'); }; return session;
  };
  await assert.rejects(runSlice(config(), 'two', { transport: fake.transport, scenarioHook() { throw Error('scenario failed'); } }), error => {
    assert.match(error.message, /scenario failed/); assert.match(error.message, /cleanup/);
    assert.ok(error.errors.some(e => e.message.includes('owned unit still active'))); return true;
  });
});

for (const reply of [
  { stopped: true },
  { stopped: true, mode: 'crash', escalated: false },
  { stopped: true, mode: 'graceful', escalated: 'no' },
]) test(`stop evidence fails closed ${JSON.stringify(reply)}`, async () => {
  const { transport } = fakeTransport({ request(h, r) { if (r.op === 'stop') return reply; } });
  await assert.rejects(createFleet(config(), 'two', { transport }), /stop|mode|escalation/);
});

test('rolling replacement never requests intentional crash', async () => {
  const { transport, state } = fakeTransport(); await runSlice(config(), 'upgrade', { transport, pollMs: 0 });
  assert.ok(state.requests.filter(r => r.op === 'stop').every(r => !r.crash));
});

test('same-binary upgrade fails before init', async () => {
  const { transport, state } = fakeTransport({ transport: { async preflight(c, h) { return { hostname: h.hostname, physical_id: h.id.repeat(64), candidate_sha256: 'a'.repeat(64), baseline_sha256: 'a'.repeat(64) }; } } });
  await assert.rejects(runSlice(config(), 'upgrade', { transport }), /same-binary/);
  assert.equal(state.requests.filter(r => r.op === 'init').length, 0);
});

for (const [name, override] of [
  ['wrong hostname', { transport: { async preflight() { return { hostname: 'wrong', candidate_sha256: 'a'.repeat(64) }; } } }],
  ['unsigned proof', { request: (h, r) => r.op === 'peers' ? { a: { binding_signature_ref: 'fake' }, b: { binding_signature_ref: 'fake' } } : undefined }],
  ['arbitrary call failure', { request: (h, r) => r.op === 'call' ? { outcome: 'failed' } : undefined }],
  ['bad executor evidence', { request: (h, r) => r.op === 'verify' ? verified({ endpoint: `endpoint-${h.id}` }) : undefined }],
  ['revocation timeout', { request: (h, r, s) => r.op === 'call' && s.revoked ? { outcome: 'timed_out' } : undefined }],
]) test(`lifecycle rejects ${name} and cleans up`, async () => {
  const { transport, state } = fakeTransport(override);
  await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0, refreshTimeoutMs: 20 }));
  assert.equal(state.running.size, 0);
});

test('refresh retries only explicit no-route responses and never reuses labels', async () => {
  let first = true;
  const { transport, state } = fakeTransport({ request(h, r) {
    if (r.op === 'call' && first) { first = false; return { outcome: 'denied', protocol_reason: 'no_route', submitted_call_id: 'pending', submitted_trace_id: 'pending-trace', route_taken: null }; }
  } });
  await runSlice(config(), 'two', { transport, pollMs: 0 });
  const labels = state.requests.filter(r => r.op === 'call').map(r => r.label);
  assert.equal(new Set(labels).size, labels.length);
});

test('partition hook is explicit and is not silently promoted to proof', async () => {
  const { transport } = fakeTransport(); let invoked = false;
  const report = await runSlice(config(), 'three', { transport, pollMs: 0, async scenarioHook(fleet) {
    invoked = true; assert.equal(fleet.hosts.length, 3); await runCalls(fleet, fleet.hosts, 'external-hook');
  } });
  assert.ok(invoked); assert.equal(report.partition.status, 'not_verified');
  assert.equal(report.status, 'incomplete');
});

test('startup digest must equal preflight, not just name the intended binary', async () => {
  const { transport, state } = fakeTransport({ request(h, r) {
    if (r.op === 'start') return { ...started({ ...h, endpoint: `endpoint-${h.id}` }), binary: state.binaries.get(h.id), binary_sha256: 'd'.repeat(64) };
  } });
  await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0 }), /digest/);
  assert.equal(state.running.size, 0);
});

test('a successful local response without matching local execution evidence fails', async () => {
  const { transport } = fakeTransport({ request(h, r, state) {
    if (r.op === 'verify') return { ...verified({ node: h.node, endpoint: `endpoint-${h.id}` }),
      observations: state.observations.get(h.id).filter(o => !o.detail_ref?.startsWith(`executed_on:${h.id};forwarded_from:${h.id};`)) };
  } });
  await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0 }));
});

test('scenario labels remain within the native helper limit', async () => {
  const { transport } = fakeTransport();
  await runSlice(config(), 'two', { transport, pollMs: 0, scenarioHook: fleet => runCalls(fleet, fleet.hosts, 'a'.repeat(64)) });
});

test('no-route refresh expires rather than passing indefinitely', async () => {
  let serial = 0;
  const { transport } = fakeTransport({ request(h, r) {
    if (r.op === 'call') return { outcome: 'denied', protocol_reason: 'no_route', submitted_call_id: `pending-${++serial}`, submitted_trace_id: `trace-${serial}` };
  } });
  await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0, refreshTimeoutMs: 5 }), /deadline/);
});

test('reused call IDs fail closed', async () => {
  const { transport } = fakeTransport({ request(h, r) {
    if (r.op === 'call') return completed({ node: config().hosts[r.destination].id, index: r.destination }, h.index === r.destination);
  } });
  await assert.rejects(runSlice(config(), 'two', { transport, pollMs: 0 }), /reused/);
});

test('two-host preflight does not require the optional upgrade baseline', async () => {
  const fake = fakeTransport(); const original = fake.transport.preflight;
  fake.transport.preflight = async (c, h) => { assert.equal(c.baselinePackage, undefined); return original(c, h); };
  await runSlice(config(), 'two', { transport: fake.transport, pollMs: 0 });
});

test('CLI runs through an absolute symlinked worktree path and rejects missing arguments', () => {
  const script = fileURLToPath(new URL('./fleet.mjs', import.meta.url)).replace(/^\/private\/tmp\//, '/tmp/');
  const result = spawnSync(process.execPath, [script], { encoding: 'utf8' });
  assert.equal(result.status, 1); assert.match(result.stderr, /required/);
});

test('scenario API can be imported from a stdin-driven operator script without invoking the CLI', () => {
  const script = new URL('./fleet.mjs', import.meta.url).href;
  const result = spawnSync(process.execPath, ['--input-type=module', '-'], {
    input: `import { createFleet, runCalls } from ${JSON.stringify(script)}; console.log(typeof createFleet, typeof runCalls);`, encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr); assert.equal(result.stdout.trim(), 'function function');
});

test('public evidence keeps node names even when they also name inventory hosts', () => {
  const c = config(); c.hosts[0].hostname = c.hosts[0].id;
  assert.deepEqual(sanitize({ nodes: ['a', 'b', 'c'], node_id: 'a' }, c), { nodes: ['a', 'b', 'c'], node_id: 'a' });
});

test('stderr diagnostics are assembled before redaction across stream chunks', async () => {
  const diagnostics = []; const c = config();
  const child = processDouble(() => {});
  const session = new JsonLineSession(child, { onStderr: text => diagnostics.push(sanitize(text, c)) });
  const address = c.hosts[0].address;
  child.stderr.write(address.slice(0, 4)); child.stderr.end(address.slice(4) + '\n');
  await new Promise(resolve => child.stderr.once('end', resolve));
  assert.equal(diagnostics.length, 1); assert.ok(!diagnostics.join('').includes(address));
  await session.close();
});

test('public evidence redacts LAN and SSH values without removing input node names', () => {
  const c = config(); const result = sanitize({ node: 'a', detail: `${c.hosts[0].ssh} ${c.hosts[1].address}:24863`, identity: c.sshIdentity }, c);
  assert.equal(result.node, 'a');
  for (const h of c.hosts) { assert.ok(!JSON.stringify(result).includes(h.address)); assert.ok(!JSON.stringify(result).includes(h.ssh)); }
  assert.ok(!JSON.stringify(result).includes(c.sshIdentity));
});
