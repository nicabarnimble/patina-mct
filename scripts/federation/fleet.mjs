import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFileSync, realpathSync, mkdirSync, appendFileSync, writeFileSync, statSync } from 'node:fs';
import { isIP } from 'node:net';
import { posix } from 'node:path';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';
import { stageCommand, cleanupStagedCommand, stagedPath } from './stage.mjs';
import { setTimeout as sleep } from 'node:timers/promises';
import { assertNoPeerGateway, createNetworkFence, inspectNetworkFence } from './fence.mjs';

const exec = promisify(execFile);
const SHA256 = /^[a-f0-9]{64}$/;
const NAME = /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$/;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const nonempty = value => typeof value === 'string' && value.length > 0;
const hostname = value => nonempty(value) && value.length <= 253 && value.split('.').every(label => /^[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/.test(label));

function absolutePath(value) {
  return nonempty(value) && value.startsWith('/') && value !== '/' &&
    !/[\x00-\x1f\x7f]/.test(value) && !value.endsWith('/') &&
    value.split('/').slice(1).every(part => part && part !== '.' && part !== '..');
}

function exactKeys(value, required, optional = []) {
  assert.ok(object(value), 'expected object');
  assert.ok(required.every(key => Object.hasOwn(value, key)), 'missing required config key');
  assert.ok(Object.keys(value).every(key => [...required, ...optional].includes(key)), 'unknown config key');
}

export function validateConfig(input) {
  exactKeys(input, ['builder', 'sshIdentity', 'package', 'runRoot', 'vision', 'hosts'], ['baselinePackage']);
  for (const key of ['sshIdentity', 'package', 'runRoot']) assert.ok(absolutePath(input[key]), `invalid ${key}`);
  if (input.baselinePackage !== undefined) assert.ok(absolutePath(input.baselinePackage), 'invalid baselinePackage');
  for (const key of ['package', 'baselinePackage', 'runRoot']) {
    if (input[key] !== undefined) assert.match(input[key], /^\/[a-zA-Z0-9/._+-]+$/, `native helper rejects unsafe ${key}`);
  }
  assert.ok(typeof input.vision === 'string' && NAME.test(input.vision), 'unsafe vision');
  assert.ok(Array.isArray(input.hosts) && input.hosts.length >= 2, 'at least two hosts required');
  const ids = new Set(); const addresses = new Set(); const hostnames = new Set();
  for (const h of input.hosts) {
    exactKeys(h, ['id', 'ssh', 'hostname', 'address', 'port']);
    assert.ok(typeof h.id === 'string' && NAME.test(h.id), 'unsafe node name');
    assert.ok(!ids.has(h.id), 'duplicate node id'); ids.add(h.id);
    assert.ok(hostname(h.hostname), 'invalid hostname');
    assert.ok(!hostnames.has(h.hostname.toLowerCase()), 'duplicate expected hostname'); hostnames.add(h.hostname.toLowerCase());
    assert.ok(typeof h.ssh === 'string' && /^[a-zA-Z_][a-zA-Z0-9_-]*@[^@]+$/.test(h.ssh) && hostname(h.ssh.split('@')[1]), 'invalid SSH target');
    assert.equal(isIP(h.address), 4, 'LAN IPv4 address required');
    const [a, b] = h.address.split('.').map(Number);
    assert.ok(a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168), 'private LAN address required');
    assert.equal(h.ssh.split('@')[1], h.address, 'SSH destination must exactly match configured LAN address');
    assert.ok(!addresses.has(h.address), 'duplicate host address'); addresses.add(h.address);
    assert.ok(Number.isInteger(h.port) && h.port > 0 && h.port <= 65535, 'invalid port');
  }
  assert.ok(ids.has(input.builder), 'builder must name a configured host');
  return { ...input, hosts: [input.hosts.find(h => h.id === input.builder), ...input.hosts.filter(h => h.id !== input.builder)].map(h => ({ ...h })) };
}

export function unwrapReply(reply) {
  assert.ok(object(reply) && typeof reply.ok === 'boolean', 'malformed helper envelope');
  if (!reply.ok) throw Error(`helper error: ${typeof reply.error === 'string' ? reply.error : 'malformed error'}`);
  assert.ok(Object.hasOwn(reply, 'value') && !Object.hasOwn(reply, 'error'), 'malformed helper success');
  return reply.value;
}

export class JsonLineSession {
  constructor(child, { timeoutMs = 150_000, maxLineBytes = 16 * 1024 * 1024, onStderr = () => {}, stopUnit = async () => {} } = {}) {
    this.child = child; this.timeoutMs = timeoutMs; this.maxLineBytes = maxLineBytes; this.stopUnit = stopUnit;
    this.buffer = ''; this.diagnostic = ''; this.pending = null; this.failure = null; this.closing = false; this.tail = Promise.resolve();
    this.exit = new Promise(resolveExit => { this.resolveExit = resolveExit; });
    child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
    child.stdout.on('data', chunk => this.receive(chunk));
    const diagnostic = text => { try { onStderr(text); } catch (error) { this.fail(error); } };
    child.stderr.on('data', chunk => {
      this.diagnostic += chunk;
      let end;
      while ((end = this.diagnostic.indexOf('\n')) !== -1) {
        diagnostic(this.diagnostic.slice(0, end)); this.diagnostic = this.diagnostic.slice(end + 1);
      }
      if (Buffer.byteLength(this.diagnostic) > 64 * 1024) {
        this.diagnostic = ''; this.fail(Error('helper diagnostic line exceeds limit'));
      }
    });
    child.stderr.on('end', () => { if (this.diagnostic) diagnostic(this.diagnostic); this.diagnostic = ''; });
    child.on('error', error => { this.fail(error); this.resolveExit(); });
    child.stdin.on('error', error => this.fail(error));
    child.stdout.on('error', error => this.fail(error));
    child.stdout.on('end', () => { if (!this.closing) this.fail(Error('helper stdout EOF')); });
    child.on('exit', (code, signal) => {
      if (!this.closing || this.pending) this.fail(Error(`helper exited (${code ?? signal})`));
      this.resolveExit();
    });
  }

  fail(error) {
    this.failure ??= error;
    if (this.pending) {
      clearTimeout(this.pending.timer); this.pending.reject(this.failure); this.pending = null;
    }
  }

  receive(chunk) {
    if (this.failure) return;
    if (!this.pending) return this.fail(Error('unsolicited helper stdout'));
    this.buffer += chunk;
    if (Buffer.byteLength(this.buffer) > this.maxLineBytes) return this.fail(Error('helper response exceeds limit'));
    while (this.buffer.includes('\n')) {
      const end = this.buffer.indexOf('\n');
      const line = this.buffer.slice(0, end); this.buffer = this.buffer.slice(end + 1);
      if (!this.pending) return this.fail(Error('unsolicited helper stdout'));
      try {
        const value = unwrapReply(JSON.parse(line));
        assert.equal(this.buffer, '', 'extra helper stdout after response');
        const pending = this.pending; this.pending = null; clearTimeout(pending.timer); pending.resolve(value);
      } catch (error) { this.fail(error); return; }
    }
  }

  request(request) {
    const result = this.tail.then(() => {
      if (this.failure) throw this.failure;
      assert.ok(!this.closing, 'helper session is closing');
      const line = JSON.stringify(request) + '\n';
      return new Promise((resolveReply, reject) => {
        const timer = setTimeout(() => this.fail(Error(`helper ${request.op} timed out`)), this.timeoutMs);
        this.pending = { resolve: resolveReply, reject, timer };
        try { this.child.stdin.write(line); } catch (error) { this.fail(error); }
      });
    });
    this.tail = result.catch(() => {});
    return result;
  }

  close() {
    this.closePromise ??= this.closeOwned();
    return this.closePromise;
  }

  async closeOwned() {
    this.closing = true;
    this.fail(Error('helper session closed'));
    let timer;
    try {
      this.child.stdin.end();
      await Promise.race([this.exit, new Promise(resolve => { timer = setTimeout(resolve, 40_000); })]);
    } finally {
      clearTimeout(timer);
      try { await this.stopUnit(); }
      finally {
        this.child.kill('SIGTERM');
        const force = setTimeout(() => this.child.kill('SIGKILL'), 5_000);
        try { await this.exit; } finally { clearTimeout(force); }
      }
    }
  }
}

const quote = value => `'${String(value).replaceAll("'", "'\"'\"'")}'`;
export function sshArgs(config, host, command) {
  return ['-T', '-o', 'BatchMode=yes', '-o', 'IdentitiesOnly=yes', '-o', 'StrictHostKeyChecking=yes',
    '-o', 'ControlMaster=no', '-o', 'ControlPath=none', '-o', 'ControlPersist=no',
    '-o', 'ConnectTimeout=15', '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=2',
    '-i', config.sshIdentity, '--', host.ssh, command.map(quote).join(' ')];
}

export function createSshTransport({ onDiagnostic = () => {}, onFence = () => {}, requireNetworkFence = false, spawn: spawnProcess = spawn, exec: execute = exec } = {}) {
  return {
    async stageBinary(config, host, source, kind, digest) {
      const { stdout, stderr } = await execute('ssh', sshArgs(config, host, stageCommand(source, host.root, kind, digest)), { timeout: 120_000, maxBuffer: 1024 * 1024 });
      if (stderr) onDiagnostic(host.id, stderr);
      assert.equal(stdout.trim(), stagedPath(host.root, kind), 'unexpected staged executable path');
      return stdout.trim();
    },
    async cleanupBinaries(config, host) {
      await execute('ssh', sshArgs(config, host, cleanupStagedCommand(host.root)), { timeout: 60_000, maxBuffer: 1024 * 1024 });
    },
    async preflight(config, host) {
      const binaries = [posix.join(config.package, 'bin/mct-daemon')];
      if (config.baselinePackage) binaries.push(posix.join(config.baselinePackage, 'bin/mct-daemon'));
      const script = [
        'set -eu', 'hostname', 'command -v pgrep >/dev/null',
        'command -v systemd-run >/dev/null', 'command -v systemctl >/dev/null', 'sudo -n true',
        'sha256sum < /proc/sys/kernel/random/boot_id',
        'if pgrep -x mct-daemon >/dev/null; then echo "existing mct-daemon" >&2; exit 71; else rc=$?; [ "$rc" -eq 1 ] || exit "$rc"; fi',
        `test -x ${quote(posix.join(config.package, 'bin/mct-federation-host'))}`,
        ...binaries.flatMap(binary => [`test -x ${quote(binary)}`,
          `if command -v sha256sum >/dev/null; then sha256sum < ${quote(binary)}; else shasum -a 256 < ${quote(binary)}; fi`]),
      ].join('\n');
      const { stdout, stderr } = await execute('ssh', sshArgs(config, host, ['sh', '-c', script]), { timeout: 60_000, maxBuffer: 1024 * 1024 });
      if (stderr) onDiagnostic(host.id, stderr);
      const lines = stdout.trim().split(/\r?\n/);
      assert.equal(lines.length, binaries.length + 2, 'unexpected preflight output');
      const hashes = lines.slice(1).map(line => {
        const match = /^([a-f0-9]{64})\s+-?$/.exec(line); assert.ok(match, 'invalid binary SHA256'); return match[1];
      });
      if (requireNetworkFence) {
        const routes = await execute('ssh', sshArgs(config, host, ['ip', '-j', 'route', 'show', 'default']), { timeout: 15_000, maxBuffer: 1024 * 1024 });
        assertNoPeerGateway(config, routes.stdout);
      }
      return { hostname: lines[0], physical_id: hashes[0], candidate_sha256: hashes[1], baseline_sha256: hashes[2] };
    },
    async connect(config, host) {
      const username = host.ssh.split('@')[0];
      assert.match(username, /^[a-zA-Z_][a-zA-Z0-9_-]*$/, 'invalid service username');
      const unit = `mct-federation-${randomUUID()}.service`;
      const fence = requireNetworkFence ? createNetworkFence(config, unit) : null;
      const command = ['sudo', '-n', 'systemd-run', '--quiet', '--pipe', '--wait', '--collect',
        `--uid=${username}`, `--unit=${unit}`, '--property=KillMode=control-group',
        '--property=TimeoutStopSec=35s', '--property=RuntimeMaxSec=900s',
        ...(fence?.properties ?? []),
        '--', posix.join(config.package, 'bin/mct-federation-host')];
      const child = spawnProcess('ssh', sshArgs(config, host, command), { stdio: ['pipe', 'pipe', 'pipe'] });
      const session = new JsonLineSession(child, {
        onStderr: text => onDiagnostic(host.id, text),
        async stopUnit() {
          const script = [
            'set -eu', `unit=${quote(unit)}`,
            'load_state() { load=$(sudo -n systemctl show --property=LoadState --value "$unit") || { [ "$load" = not-found ] || return 1; }; }',
            'load_state',
            'if [ "$load" != not-found ]; then if ! sudo -n systemctl stop "$unit"; then load_state; [ "$load" = not-found ] || exit 1; fi; fi',
            'load_state',
            'if [ "$load" = not-found ]; then echo absent; else sudo -n systemctl show --property=ActiveState --value "$unit"; fi',
          ].join('\n');
          const { stdout, stderr } = await execute('ssh', sshArgs(config, host, ['sh', '-c', script]), { timeout: 60_000, maxBuffer: 1024 * 1024 });
          if (stderr) onDiagnostic(host.id, stderr);
          assert.ok(['inactive', 'absent'].includes(stdout.trim()), 'owned unit cleanup not confirmed inactive or absent');
        },
      });
      if (fence) {
        try {
          const proof = await inspectNetworkFence(fence, command => execute('ssh', sshArgs(config, host, command), { timeout: 5_000, maxBuffer: 1024 * 1024 }));
          if (session.failure) throw Error('helper failed during network fence inspection');
          await onFence(host.id, proof);
          if (session.failure) throw Error('helper failed during network fence proof publication');
        } catch (error) {
          try { await session.close(); }
          catch (cleanupError) { throw new AggregateError([error, cleanupError], 'network fence failed and owned unit cleanup failed'); }
          throw error;
        }
      }
      return session;
    },
  };
}

export function assertStart(reply, host, vision) {
  assert.ok(Number.isInteger(reply?.pid) && reply.pid > 0, 'invalid daemon PID');
  assert.equal(reply.owned_mother_count, 1, 'host must have exactly the owned Mother');
  assert.equal(reply.status?.readiness, 'ready', 'daemon not ready');
  assert.equal(reply.status.resident?.node_id, host.node, 'wrong daemon node');
  assert.equal(reply.status.resident?.vision_id, vision, 'wrong daemon vision');
  const endpoint = reply.status.iroh_endpoint;
  assert.equal(endpoint?.relay_mode, 'disabled', 'relay mode must be disabled');
  assert.deepEqual(endpoint.relay_urls, [], 'relay URLs must be empty');
  assert.ok(nonempty(endpoint.endpoint_id), 'missing endpoint id');
  assert.equal(reply.ticket?.endpoint_id, endpoint.endpoint_id, 'ticket identity mismatch');
  assert.deepEqual(reply.ticket.relay_urls, [], 'ticket relays forbidden');
  assert.deepEqual(reply.ticket.direct_addresses, [`${host.address}:${host.port}`], 'ticket must contain only configured LAN socket');
  if (host.endpoint) assert.equal(endpoint.endpoint_id, host.endpoint, 'endpoint changed across restart');
  return endpoint.endpoint_id;
}

export function assertCall(reply, caller, destination, expected = 'completed') {
  assert.ok(nonempty(reply?.submitted_call_id) && nonempty(reply?.submitted_trace_id), 'missing submitted call correlation');
  assert.equal(reply.outcome, expected, `unexpected call outcome ${caller.node} -> ${destination.node}`);
  if (expected === 'denied') {
    assert.ok(reply.route_taken == null, 'denied call reports execution route');
    assert.ok(reply.inline_result_payload_base64 == null && reply.decoded_result == null, 'denied call reports execution result');
    return;
  }
  assert.equal(expected, 'completed', 'unsupported call expectation');
  const expectedResult = { results: [41 + destination.index] };
  assert.deepEqual(reply.decoded_result, expectedResult, 'incorrect arithmetic result');
  assert.ok(nonempty(reply.inline_result_payload_base64), 'missing actual result bytes');
  assert.deepEqual(JSON.parse(Buffer.from(reply.inline_result_payload_base64, 'base64').toString('utf8')), expectedResult, 'result bytes disagree');
  assert.equal(reply.route_taken?.node_id, destination.node, 'wrong executor route');
  assert.equal(reply.route_taken?.runtime_kind, caller.node === destination.node ? 'wasm_component' : 'remote_peer', 'wrong execution locality');
}

function markers(observation) {
  const detail = observation.detail_ref ?? '';
  assert.equal(typeof detail, 'string', 'invalid observation detail');
  const values = {};
  for (const part of detail.split(';')) {
    const match = /^(executed_on|forwarded_from|forwarded_to|operation|hello_reason|remote_reason|remote_reply):(.*)$/.exec(part);
    if (match) { assert.ok(match[2] && !Object.hasOwn(values, match[1]), 'invalid provenance marker'); values[match[1]] = match[2]; }
    else assert.ok(!part.includes('executed_on:'), 'malformed executor marker');
  }
  return values;
}

export function assertVerification(reply, host) {
  assert.equal(reply?.verified, true, 'ledger verification missing');
  assert.equal(reply.node_id, host.node, 'wrong ledger node');
  assert.equal(reply.verification?.verified, true, 'missing canonical verification summary');
  assert.equal(reply.verification.head?.ledger_id, 'ledger-local', 'wrong ledger lineage');
  assert.equal(reply.verification.head?.mother_node_id, host.node, 'wrong canonical ledger owner');
  assert.ok(Number.isSafeInteger(reply.verification.entry_count) && reply.verification.entry_count > 0, 'empty canonical ledger');
  assert.ok(SHA256.test(reply.verification.prefix_blake3), 'missing canonical prefix digest');
  assert.equal(reply.previous_checkpoint_bytes, host.checkpoint?.ledger_bytes ?? null, 'wrong checkpoint ancestry');
  assert.equal(reply.endpoint_id, host.endpoint, 'ledger endpoint changed');
  assert.equal(reply.prefix_preserved, true, 'ledger prefix not preserved');
  assert.ok(Number.isSafeInteger(reply.ledger_bytes) && reply.ledger_bytes > 0, 'empty or invalid ledger length');
  assert.ok(SHA256.test(reply.ledger_sha256), 'invalid ledger SHA256');
  assert.ok(Array.isArray(reply.observations), 'missing reduced observations');
  if (host.checkpoint) {
    assert.ok(reply.ledger_bytes >= host.checkpoint.ledger_bytes, 'ledger shrank');
    if (reply.ledger_bytes === host.checkpoint.ledger_bytes) assert.equal(reply.ledger_sha256, host.checkpoint.ledger_sha256, 'same-length ledger changed');
  }
  assert.ok(Number.isSafeInteger(reply.verification.head.local_sequence) && reply.verification.head.local_sequence >= 0, 'invalid canonical head sequence');
  let previousSequence = -1;
  const correlatedIds = new Set();
  for (const observation of reply.observations) {
    assert.ok(object(observation), 'invalid reduced observation');
    assert.ok(Number.isSafeInteger(observation.local_sequence) && observation.local_sequence > previousSequence &&
      observation.local_sequence <= reply.verification.head.local_sequence, 'invalid canonical source sequence');
    previousSequence = observation.local_sequence;
    if (Object.hasOwn(observation, 'peer_correlation')) {
      const correlation = observation.peer_correlation;
      exactKeys(correlation, ['caller_call_id', 'peer_binding_id', 'caller_node_id', 'caller_vision_id']);
      assert.ok(Object.values(correlation).every(v => nonempty(v) && v.trim().length > 0), 'invalid peer correlation strings');
      assert.equal(observation.kind, 'call_constructed', 'correlation must originate in CallConstructed');
      assert.ok(nonempty(observation.call_id) && nonempty(observation.trace_id), 'missing receiver correlation IDs');
      assert.notEqual(observation.call_id, correlation.caller_call_id, 'receiver must mint its own call ID');
      assert.ok(!correlatedIds.has(observation.call_id), 'duplicate receiver correlation');
      correlatedIds.add(observation.call_id);
    }
    const provenance = markers(observation);
    if (provenance.executed_on) assert.equal(provenance.executed_on, host.node, 'foreign executor in local ledger');
  }
  return reply;
}

const operation = host => `patina:fleet-n${host.index}/work@0.1.0.step`;
const traceId = observation => observation.trace_id ?? observation.trace?.trace_id;
const executionKinds = new Set(['runtime_execution_started', 'runtime_execution_completed', 'runtime_execution_failed', 'runtime_execution_timed_out']);

function assertExecution(observations, call, executionId, caller, destination) {
  const executed = observations.filter(o => markers(o).executed_on &&
    (o.call_id === executionId || traceId(o) === call.trace_id));
  assert.equal(executed.length, 1, `missing or duplicate actual executor ${call.call_id}`);
  const [event] = executed;
  assert.equal(event.call_id, executionId, 'wrong execution call ID');
  assert.equal(traceId(event), call.trace_id, 'wrong execution trace');
  assert.equal(event.kind, 'runtime_execution_completed', 'not a completed execution event');
  assert.equal(event.outcome, 'completed', 'execution did not complete');
  assert.equal(event.source_plane, 'child', 'not local executor evidence');
  assert.deepEqual(markers(event), { executed_on: destination.node, forwarded_from: caller.node, operation: operation(destination) }, 'wrong executor provenance or operation');
  return event;
}

function assertForwarding(observations, call, caller, destination) {
  const sent = observations.filter(o => o.call_id === call.call_id && traceId(o) === call.trace_id && o.kind === 'peer_call_sent');
  assert.equal(sent.length, 1, `missing or duplicate caller forwarding ${call.call_id}`);
  assert.deepEqual(markers(sent[0]), { forwarded_from: caller.node, forwarded_to: destination.node, operation: operation(destination) }, 'wrong forwarding provenance or operation');
}

export function assertLedgers(fleet) {
  const directions = [];
  for (const caller of fleet.hosts) {
    const own = caller.checkpoint.observations;
    for (const call of fleet.calls.filter(c => c.caller === caller.node && c.destination === caller.node && c.outcome === 'completed')) {
      assert.ok(!own.some(o => o.peer_correlation && (traceId(o) === call.trace_id || o.peer_correlation.caller_call_id === call.call_id)), 'local call has peer correlation');
      assertExecution(own, call, call.call_id, caller, caller);
    }
    for (const destination of fleet.hosts.filter(h => h !== caller)) {
      assert.ok(own.some(o => o.kind === 'peer_admitted' && o.outcome === 'allowed' && o.subject_id === `${caller.node}-admits-${destination.node}`), `missing admitted hello at ${caller.node} for ${destination.node}`);
      const calls = fleet.calls.filter(c => c.caller === caller.node && c.destination === destination.node && c.outcome === 'completed');
      assert.ok(calls.length > 0, 'missing remote direction calls');
      let forwarded = 0; let executed = 0;
      for (const call of calls) {
        assertForwarding(own, call, caller, destination);
        assert.ok(!own.some(o => (executionKinds.has(o.kind) || markers(o).executed_on) && (o.call_id === call.call_id || traceId(o) === call.trace_id || markers(o).operation === operation(destination))), 'caller locally executed remote operation');
        const receiver = destination.checkpoint.observations;
        const correlations = receiver.filter(o => o.peer_correlation &&
          (o.peer_correlation.caller_call_id === call.call_id || traceId(o) === call.trace_id));
        assert.equal(correlations.length, 1, `missing or duplicate peer correlation ${call.call_id}`);
        const [correlation] = correlations;
        assert.equal(traceId(correlation), call.trace_id, 'wrong correlation trace');
        assert.deepEqual(correlation.peer_correlation, {
          caller_call_id: call.call_id, peer_binding_id: `${destination.node}-admits-${caller.node}`,
          caller_node_id: caller.node, caller_vision_id: fleet.config.vision,
        }, 'wrong peer correlation authority');
        const ran = assertExecution(receiver, call, correlation.call_id, caller, destination);
        assert.ok(correlation.local_sequence < ran.local_sequence, 'execution precedes durable correlation');
        forwarded++; executed++;
      }
      directions.push({ caller: caller.node, destination: destination.node, calls: calls.length, forwarded, executed });
    }
  }
  for (const denied of fleet.calls.filter(c => c.outcome === 'denied' || c.expect_no_execution)) {
    if (denied.partition_probe) {
      const caller = fleet.hosts.find(host => host.node === denied.caller);
      const destination = fleet.hosts.find(host => host.node === denied.destination);
      assert.equal(denied.outcome, 'failed', 'partition probe did not fail in transport');
      assert.equal(denied.safe_message, 'remote peer unavailable', 'unrelated partition probe failure');
      assertForwarding(caller.checkpoint.observations, denied, caller, destination);
    }
    const all = fleet.hosts.flatMap(h => h.checkpoint.observations);
    const ids = new Set([denied.call_id, ...all.filter(o => o.peer_correlation &&
      (o.peer_correlation.caller_call_id === denied.call_id || traceId(o) === denied.trace_id)).map(o => o.call_id)]);
    assert.ok(!all.some(o => (executionKinds.has(o.kind) || markers(o).executed_on) &&
      (traceId(o) === denied.trace_id || ids.has(o.call_id))), 'denied call executed');
  }
  const [a, b] = fleet.hosts;
  const denied = fleet.calls.find(c => c.call_id === fleet.revocationCall);
  assert.equal(denied?.outcome, 'denied', 'missing receiver revocation call');
  assert.notEqual(denied.protocol_reason, 'no_route', 'no_route is not receiver revocation evidence');
  assertForwarding(a.checkpoint.observations, denied, a, b);
  const binding = `${b.node}-admits-${a.node}`;
  const revocations = b.checkpoint.observations.filter(o => o.kind === 'peer_binding_revoked' &&
    o.outcome === 'allowed' && o.subject_id === a.node && o.resource_id === binding);
  assert.equal(revocations.length, 1, 'missing or duplicate receiver binding revocation');
  const rejections = b.checkpoint.observations.filter(o => o.kind === 'peer_rejected' && traceId(o) === denied.trace_id);
  assert.equal(rejections.length, 1, 'missing or duplicate receiver rejection for denied trace');
  const [rejection] = rejections;
  assert.equal(rejection.outcome, 'denied', 'receiver did not deny');
  assert.ok(rejection.local_sequence > revocations[0].local_sequence, 'receiver rejection predates revocation');
  // Denied hello evaluations have no selected binding. The forwarding trace joins them to this caller.
  assert.ok(rejection.subject_id == null || rejection.subject_id === binding, 'receiver rejected another binding');
  assert.equal(markers(rejection).hello_reason, 'BindingRevoked', 'receiver denial is not revoked authority');
  return directions;
}

export async function createFleet(input, slice = 'two', options = {}) {
  const config = validateConfig(input);
  assert.ok(['two', 'three', 'upgrade'].includes(slice), 'invalid slice');
  const count = slice === 'two' ? 2 : 3;
  assert.ok(config.hosts.length >= count, `${slice} requires ${count} hosts`);
  if (slice === 'upgrade') assert.ok(config.baselinePackage && config.baselinePackage !== config.package, 'distinct baselinePackage required');
  const token = `fleet-${randomUUID()}`;
  const hosts = config.hosts.slice(0, count).map((host, index) => ({ ...host, index, node: host.id,
    root: posix.join(config.runRoot, token, String(index)), state: 'new', session: null, endpoint: null, checkpoint: null }));
  for (const host of hosts) assert.ok(Buffer.byteLength(posix.join(host.root, 'control.sock')) < 104, 'runRoot is too long for native Unix socket');
  const transport = options.transport ?? createSshTransport();
  const fleet = {
    config, hosts, token, calls: [], callIds: new Set(), traceIds: new Set(), binaries: [],
    pollMs: options.pollMs ?? 500, refreshTimeoutMs: options.refreshTimeoutMs ?? 120_000,
    signal: options.signal,
    emit(stage, details = {}) { options.onStage?.(sanitize({ stage, ...details }, config)); },
    async request(host, request) { this.signal?.throwIfAborted(); return host.session.request(request); },
    async control(host, path, body) {
      return this.request(host, { op: 'control', method: 'POST', path, body: { expected_config_path: posix.join(host.root, 'config.json'), ...body } });
    },
    async start(host, binary) {
      assert.ok(['stopped', 'crashed'].includes(host.state), 'start requires stopped daemon');
      const reply = await this.request(host, { op: 'start', ...(binary ? { binary } : {}) });
      host.endpoint = assertStart(reply, host, config.vision); host.ticket = reply.ticket; host.state = 'running';
      const expectedBinary = binary ?? host.binary;
      const digests = this.binaries.find(b => b.node === host.node);
      const expectedDigest = expectedBinary === digests.candidate ? digests.candidate_sha256 : digests.baseline_sha256;
      assert.equal(reply.binary, expectedBinary, 'started wrong binary');
      assert.ok(SHA256.test(expectedDigest), 'missing expected binary digest');
      assert.equal(reply.binary_sha256, expectedDigest, 'started binary digest differs from preflight');
      this.emit('started', { node: host.node, endpoint_id: host.endpoint, binary: expectedBinary, binary_sha256: reply.binary_sha256, pid: reply.pid, owned_mother_count: reply.owned_mother_count });
      host.binary = expectedBinary;
    },
    async stop(host, { crash = false } = {}) {
      assert.equal(typeof crash, 'boolean', 'invalid crash option');
      const reply = await this.request(host, { op: 'stop', ...(crash ? { crash: true } : {}) });
      assert.equal(reply?.stopped, true, 'stop was not confirmed');
      assert.equal(reply.mode, crash ? 'crash' : 'graceful', 'wrong stop mode');
      assert.equal(typeof reply.escalated, 'boolean', 'missing stop escalation evidence');
      if (crash) assert.equal(reply.exit_signal, 9, 'crash requires an observed SIGKILL exit');
      host.state = crash || reply.escalated || reply.recovery_required || host.state === 'crashed' ? 'crashed' : 'stopped';
      this.emit('stopped', { node: host.node, ...reply });
      return reply;
    },
    async verify(host) {
      assert.notEqual(host.state, 'crashed', 'restart for daemon recovery before verification');
      assert.equal(host.state, 'stopped', 'verify requires stopped daemon');
      host.checkpoint = assertVerification(await this.request(host, { op: 'verify' }), host);
      this.emit('checkpoint', { node: host.node, endpoint_id: host.endpoint, ledger_bytes: host.checkpoint.ledger_bytes,
        ledger_sha256: host.checkpoint.ledger_sha256, prefix_preserved: host.checkpoint.prefix_preserved });
    },
    async call(caller, destination, label, expected = 'completed', refresh = false) {
      const deadline = Date.now() + this.refreshTimeoutMs;
      for (;;) {
        assert.equal(caller.state, 'running', 'caller is stopped');
        assert.equal(destination.state, 'running', 'destination is stopped');
        assert.ok(nonempty(label) && /^[a-zA-Z0-9][a-zA-Z0-9_-]*$/.test(label), 'invalid scenario label');
        const uniqueLabel = `${label.slice(0, 27)}-${randomUUID()}`;
        const reply = await this.request(caller, { op: 'call', destination: destination.index, label: uniqueLabel });
        if (refresh) assert.ok(Date.now() <= deadline, 'catalog refresh deadline exceeded');
        assert.ok(nonempty(reply?.submitted_call_id) && nonempty(reply?.submitted_trace_id), 'missing submitted call correlation');
        assert.ok(!this.callIds.has(reply.submitted_call_id) && !this.traceIds.has(reply.submitted_trace_id), 'reused call or trace id');
        this.callIds.add(reply.submitted_call_id); this.traceIds.add(reply.submitted_trace_id);
        if (refresh && expected === 'completed' && reply.outcome === 'denied' && reply.protocol_reason === 'no_route') {
          assertCall(reply, caller, destination, 'denied');
          const pending = { caller: caller.node, destination: destination.node, label: uniqueLabel, outcome: 'denied',
            protocol_reason: 'no_route', call_id: reply.submitted_call_id, trace_id: reply.submitted_trace_id };
          this.calls.push(pending); this.emit('refresh-pending', pending);
          assert.ok(Date.now() < deadline, 'catalog refresh deadline exceeded');
          await sleep(this.pollMs, undefined, { signal: this.signal }); continue;
        }
        assertCall(reply, caller, destination, expected);
        const record = { caller: caller.node, destination: destination.node, label: uniqueLabel, outcome: reply.outcome,
          call_id: reply.submitted_call_id, trace_id: reply.submitted_trace_id, protocol_reason: reply.protocol_reason,
          route: reply.route_taken, result: reply.decoded_result };
        this.calls.push(record); this.emit('call', record); return reply;
      }
    },
    async close() {
      const errors = [];
      await Promise.all(hosts.filter(h => h.session).map(async host => {
        try {
          const reply = await host.session.request({ op: 'stop' });
          assert.equal(reply?.stopped, true); assert.equal(reply.mode, 'graceful'); assert.equal(typeof reply.escalated, 'boolean');
          this.emit('stopped', { node: host.node, ...reply });
        }
        catch (error) { errors.push({ node: host.node, error: error.message }); }
        finally {
          try {
            await host.session.close();
            if (host.stagingRequested) await transport.cleanupBinaries(config, host);
          } catch (error) { errors.push({ node: host.node, error: error.message }); }
          host.state = 'closed';
        }
      }));
      return errors;
    },
  };
  try {
    const physicalIds = new Set();
    for (const host of hosts) {
      fleet.signal?.throwIfAborted();
      const preflightConfig = { ...config };
      if (slice !== 'upgrade') delete preflightConfig.baselinePackage;
      const proof = await transport.preflight(preflightConfig, host);
      assert.equal(proof.hostname, host.hostname, 'preflight hostname mismatch');
      assert.ok(typeof proof.physical_id === 'string' && SHA256.test(proof.physical_id), 'missing physical host identity digest');
      assert.ok(!physicalIds.has(proof.physical_id), 'duplicate physical host identity'); physicalIds.add(proof.physical_id);
      assert.ok(SHA256.test(proof.candidate_sha256), 'missing candidate binary digest');
      if (slice === 'upgrade') {
        assert.ok(SHA256.test(proof.baseline_sha256), 'missing baseline binary digest');
        assert.notEqual(proof.candidate_sha256, proof.baseline_sha256, 'same-binary restart is not an upgrade');
      }
      fleet.binaries.push({ node: host.node, candidate: posix.join(config.package, 'bin/mct-daemon'),
        candidate_sha256: proof.candidate_sha256, ...(slice === 'upgrade' ? { baseline: posix.join(config.baselinePackage, 'bin/mct-daemon'), baseline_sha256: proof.baseline_sha256 } : {}) });
      fleet.emit('preflight', fleet.binaries.at(-1));
    }
    for (const host of hosts) host.session = await transport.connect(config, host);
    const fixtures = [];
    for (const host of hosts) {
      const fixture = await fleet.request(hosts[0], { op: 'fixture', index: host.index });
      assert.equal(fixture?.index, host.index, 'wrong fixture index');
      assert.ok(nonempty(fixture.component_base64) && nonempty(fixture.manifest) && nonempty(fixture.component_blake3) && nonempty(fixture.manifest_sha256), 'incomplete fixture');
      fixtures.push(fixture);
    }
    const staging = await Promise.allSettled(hosts.map(async host => {
      host.stagingRequested = true;
      const binaries = fleet.binaries.find(item => item.node === host.node);
      for (const kind of slice === 'upgrade' ? ['candidate', 'baseline'] : ['candidate']) {
        binaries[`${kind}_source`] = binaries[kind];
        binaries[kind] = await transport.stageBinary(config, host, binaries[kind], kind, binaries[`${kind}_sha256`]);
        assert.equal(binaries[kind], stagedPath(host.root, kind), 'executable staging escaped owned root');
      }
      fleet.emit('executables-staged', binaries);
    }));
    const stagingErrors = staging.filter(result => result.status === 'rejected');
    if (stagingErrors.length) throw new AggregateError(stagingErrors.map(result => result.reason), 'private executable staging failed');
    for (const host of hosts) {
      host.binary = fleet.binaries.find(item => item.node === host.node)[slice === 'upgrade' ? 'baseline' : 'candidate'];
      await fleet.request(host, { op: 'init', root: host.root, node: host.node, vision: config.vision,
        bind: `${host.address}:${host.port}`, binary: host.binary, fixture: fixtures[host.index] });
      host.state = 'stopped'; await fleet.start(host);
      const provision = await fleet.request(host, { op: 'provision' });
      assert.equal(provision?.verification_outcome, 'verified', 'fixture was not verified');
      assert.equal(provision?.approval_state, 'approved', 'fixture was not approved');
      fleet.emit('provisioned', { node: host.node, root: host.root });
      if (options.provisionHost) await options.provisionHost(fleet, host);
    }
    assert.equal(new Set(hosts.map(h => h.endpoint)).size, hosts.length, 'duplicate endpoint identities');
    for (const receiver of hosts) for (const caller of hosts.filter(h => h !== receiver)) {
      await fleet.control(receiver, '/peers/add', {
        peer_node_id: caller.node, binding_id: `${receiver.node}-admits-${caller.node}`,
        endpoint_id: caller.endpoint, vision_id: config.vision, ticket: caller.ticket, binding_signature_ref: null,
        policy_revision: 1, expires_at: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
      });
    }
    for (const receiver of hosts) {
      const peers = await fleet.request(receiver, { op: 'peers' });
      for (const caller of hosts.filter(h => h !== receiver)) {
        const binding = peers?.[caller.node];
        assert.ok(typeof binding?.binding_signature_ref === 'string' && binding.binding_signature_ref.startsWith('mct-ed25519-binding-v1:') && binding.binding_signature_ref.length > 'mct-ed25519-binding-v1:'.length, 'missing real receiver binding signature');
        assert.equal(binding.binding_id, `${receiver.node}-admits-${caller.node}`, 'wrong binding id');
        assert.ok(Number.isSafeInteger(binding.policy_revision) && binding.policy_revision > 0 && Date.parse(binding.expires_at) > Date.now(), 'invalid binding revision or expiry');
        await fleet.control(caller, '/peers/proof', { peer_node_id: receiver.node, binding_id: binding.binding_id,
          policy_revision: binding.policy_revision, signature_ref: binding.binding_signature_ref, expires_at: binding.expires_at });
      }
    }
    fleet.emit('admitted', { nodes: hosts.map(h => h.node) });
    for (const host of hosts) await fleet.stop(host);
    for (const host of hosts) await fleet.start(host);
    return fleet;
  } catch (error) {
    const cleanup = await fleet.close(); fleet.emit('failed', { error: error.message, cleanup });
    if (cleanup.length) throw new AggregateError([error, ...cleanup.map(c => Error(`${c.node}: ${c.error}`))], `${error.message}; helper cleanup was not confirmed`);
    throw error;
  }
}

export async function runCalls(fleet, hosts = fleet.hosts, label = 'calls', { local = false, refresh = false } = {}) {
  for (const caller of hosts) for (const destination of hosts) {
    if (local || caller !== destination) await fleet.call(caller, destination, label, 'completed', refresh);
  }
}

export async function runSlice(config, slice, options = {}) {
  let fleet; let report; let failure;
  try {
    fleet = await createFleet(config, slice, options);
    await runCalls(fleet, fleet.hosts, 'connected', { local: true, refresh: true });
    fleet.emit('connected');
    if (slice !== 'two') {
      for (const victim of fleet.hosts) {
        await fleet.stop(victim); await fleet.verify(victim);
        if (slice === 'three') { await fleet.start(victim); await fleet.stop(victim, { crash: true }); }
        await runCalls(fleet, fleet.hosts.filter(h => h !== victim), `down-${victim.node}`);
        fleet.emit('survivors', { victim: victim.node });
        await fleet.start(victim, slice === 'upgrade' ? fleet.binaries.find(item => item.node === victim.node).candidate : undefined);
        await runCalls(fleet, fleet.hosts, `${slice === 'upgrade' ? 'mixed' : 'resumed'}-${victim.node}`, { local: true, refresh: true });
        if (slice === 'three') { await fleet.stop(victim); await fleet.verify(victim); await fleet.start(victim); }
        fleet.emit(slice === 'upgrade' ? 'replaced' : 'restarted', { node: victim.node });
      }
    }
    const partition = options.scenarioHook ? await options.scenarioHook(fleet) ?? { status: 'not_verified' } : { status: 'not_run' };
    if (partition.status === 'passed') {
      assert.equal(partition.method, 'temporary-test-udp-drops', 'not a packet partition');
      assert.deepEqual(partition.victims.map(v => v.victim).sort(), fleet.hosts.map(h => h.node).sort(), 'missing partition victim');
      assert.ok(partition.victims.every(v => v.dropped_packets > 0 && v.survivors_served && v.policy_restored && v.rejoined), 'unproven partition recovery');
    }
    const [a, b] = fleet.hosts;
    await fleet.control(b, '/peers/revoke', { peer_node_id: a.node });
    fleet.revocationCall = (await fleet.call(a, b, 'revoked', 'denied')).submitted_call_id;
    fleet.emit('revocation', { caller: a.node, receiver: b.node });
    for (const host of fleet.hosts) { await fleet.stop(host); await fleet.verify(host); }
    const directions = assertLedgers(fleet);
    report = { status: slice === 'three' && partition.status !== 'passed' ? 'incomplete' : 'passed', slice, run: fleet.token, nodes: fleet.hosts.map(h => h.node),
      partition,
      binaries: fleet.binaries, calls: fleet.calls, directions,
      ledgers: fleet.hosts.map(h => ({ node: h.node, root: h.root, ...h.checkpoint })) };
  } catch (error) {
    failure = error; fleet?.emit('failed', { error: error.message });
  } finally {
    if (fleet) {
      const cleanup = await fleet.close();
      if (cleanup.length) {
        fleet.emit('cleanup-errors', { cleanup });
        failure = new AggregateError([...(failure ? [failure] : []), ...cleanup.map(c => Error(`${c.node}: ${c.error}`))],
          `${failure ? `${failure.message}; ` : ''}helper cleanup was not confirmed`);
      }
    }
  }
  if (failure) throw failure;
  fleet.emit('complete', { slice, partition: report.partition });
  return sanitize(report, fleet.config);
}

export function sanitize(value, config) {
  const replacements = [config.sshIdentity, ...config.hosts.flatMap(h => [h.ssh, h.address])].filter(nonempty).sort((a, b) => b.length - a.length);
  const clean = item => {
    if (typeof item === 'string') {
      for (const secret of replacements) item = item.replaceAll(secret, '[redacted]');
      return item.replace(/\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b/g, '[redacted-address]');
    }
    if (Array.isArray(item)) return item.map(clean);
    if (object(item)) return Object.fromEntries(Object.entries(item).filter(([key]) => !/secret|private_key|component_base64/i.test(key)).map(([key, entry]) => [key, key === 'node' || key === 'caller' || key === 'destination' || key === 'receiver' ? entry : clean(entry)]));
    return item;
  };
  return clean(value);
}

export async function main(args = process.argv.slice(2)) {
  const values = {};
  for (let i = 0; i < args.length; i += 2) {
    assert.ok(['--config', '--slice', '--evidence'].includes(args[i]) && nonempty(args[i + 1]) && !Object.hasOwn(values, args[i]), 'usage: --config private.json --slice two|three|upgrade --evidence absolute_dir');
    values[args[i]] = args[i + 1];
  }
  assert.equal(Object.keys(values).length, 3, 'config, slice and evidence are required');
  assert.ok(absolutePath(values['--evidence']), 'evidence must be an absolute new directory');
  assert.ok(['two', 'three', 'upgrade'].includes(values['--slice']), 'invalid slice');
  const config = validateConfig(JSON.parse(readFileSync(values['--config'], 'utf8')));
  assert.ok(statSync(config.sshIdentity).isFile(), 'SSH identity is not a file');
  const evidence = values['--evidence'];
  mkdirSync(evidence, { mode: 0o700 });
  const emit = event => {
    const line = JSON.stringify(sanitize({ time: new Date().toISOString(), ...event }, config)) + '\n';
    appendFileSync(posix.join(evidence, 'stages.jsonl'), line, { mode: 0o600 }); process.stdout.write(line);
  };
  const abort = new AbortController();
  const stop = () => abort.abort(Error('controller interrupted'));
  process.once('SIGINT', stop); process.once('SIGTERM', stop);
  const transport = createSshTransport({
    requireNetworkFence: values['--slice'] === 'upgrade',
    onFence: (node, proof) => emit({ stage: 'network_fence', node, proof }),
    onDiagnostic: (node, text) => emit({ stage: 'diagnostic', node, text }),
  });
  try {
    const scenarioHook = values['--slice'] === 'three' ? (await import('./partition.mjs')).runPartitions : undefined;
    const report = await runSlice(config, values['--slice'], { transport, onStage: emit, signal: abort.signal, scenarioHook });
    assert.equal(report.status, 'passed', 'slice gates are incomplete');
    writeFileSync(posix.join(evidence, 'result.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  } catch (error) {
    const failure = sanitize({ status: 'failed', error: error.message, causes: (error.errors ?? []).map(cause => cause.message), slice: values['--slice'], partition: { status: 'not_verified' } }, config);
    writeFileSync(posix.join(evidence, 'result.json'), JSON.stringify(failure, null, 2) + '\n', { mode: 0o600 });
    emit({ stage: 'failed', ...failure }); throw Error([failure.error, ...failure.causes].join('\n'));
  } finally { process.removeListener('SIGINT', stop); process.removeListener('SIGTERM', stop); }
}

let entrypoint;
try { entrypoint = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined; }
catch { entrypoint = undefined; }
if (import.meta.url === entrypoint) {
  main().catch(error => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
}
