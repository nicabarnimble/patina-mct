import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { constants, lstatSync, fstatSync, mkdirSync, openSync, closeSync, readFileSync, writeFileSync, fsyncSync, linkSync, unlinkSync } from 'node:fs';
import { dirname, join, resolve, isAbsolute, basename } from 'node:path';
import { spawn } from 'node:child_process';

export const TARGET = Object.freeze({ namespace: 'patina:repo-report', interface_name: 'report@0.1.0', function_name: 'inspect' });
export const CHILD = Object.freeze({ name: 'mct-repo-report', version: '0.1.0' });
export const hash = value => createHash('sha256').update(value).digest('hex');
const hex = /^[a-f0-9]{64}$/;
const portable = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/;
const same = (a, b) => assert.deepStrictEqual(a, b);
function keys(o, names) { assert.ok(o && typeof o === 'object' && !Array.isArray(o)); same(Object.keys(o).sort(), [...names].sort()); }
function safePath(p) {
  assert.ok(typeof p === 'string' && isAbsolute(p) && resolve(p) === p && !/[\x00-\x1f\x7f]/.test(p));
  assert.ok(p !== '/' && p !== '/tmp' && !p.startsWith('/tmp/') && p !== '/private/tmp' && !p.startsWith('/private/tmp/'));
}
function noSymlinks(p) {
  safePath(p); let current = '/';
  for (const part of p.slice(1).split('/')) { current = join(current, part); assert.ok(!lstatSync(current).isSymbolicLink(), `symlink: ${current}`); }
}
function privateDir(p, create = false) {
  safePath(p);
  if (create) { noSymlinks(dirname(p)); try { mkdirSync(p, { mode: 0o700 }); } catch (e) { if (e.code !== 'EEXIST') throw e; } }
  noSymlinks(p); const s = lstatSync(p); assert.ok(s.isDirectory() && s.uid === process.getuid() && (s.mode & 0o7777) === 0o700, `private directory required: ${p}`);
}
export function file(p, max, privateMode = false) {
  noSymlinks(p); const s = lstatSync(p);
  assert.ok(s.isFile() && s.size <= max && (!privateMode || (s.uid === process.getuid() && (s.mode & 0o7777) === 0o600 && s.nlink === 1)), `invalid file: ${p}`);
  const fd = openSync(p, constants.O_RDONLY | constants.O_NOFOLLOW);
  try { const actual = fstatSync(fd); assert.ok(actual.isFile() && actual.dev === s.dev && actual.ino === s.ino && actual.size <= max && (!privateMode || (actual.uid === process.getuid() && (actual.mode & 0o7777) === 0o600 && actual.nlink === 1))); const b = readFileSync(fd); assert.ok(b.length <= max); return b; } finally { closeSync(fd); }
}
function bind(s) {
  assert.equal(typeof s, 'string'); const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3}):(\d{1,5})$/.exec(s); assert.ok(m);
  const [a,b,c,d,port] = m.slice(1).map(Number); assert.ok([a,b,c,d].every(n => n <= 255) && port > 0 && port <= 65535 && (a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168)));
}
export function loadConfig(path) {
  safePath(path); privateDir(dirname(path)); const c = JSON.parse(file(path, 65536, true));
  keys(c, ['version','vision','input','component','manifest','jobs_root','studio','worker']); assert.equal(c.version, 1);
  assert.match(c.vision, portable);
  for (const p of [c.input,c.component,c.manifest]) file(p, 2 * 1024 * 1024);
  assert.equal(basename(c.component), 'mct-repo-report.wasm'); assert.equal(basename(c.manifest), 'child.toml');
  for (const [name, side] of [['studio',c.studio],['worker',c.worker]]) {
    keys(side, ['node','root','data_root','binary','helper','bind', ...(name === 'worker' ? ['ssh','binary_sha256'] : [])]);
    if (name === 'worker') assert.match(side.binary_sha256, hex);
    assert.match(side.node, portable); for (const p of [side.root,side.data_root,side.binary,side.helper]) { safePath(p); assert.match(p, /^\/[A-Za-z0-9_./-]+$/); }
    assert.ok(Buffer.byteLength(join(side.root,'control.sock')) < 104); bind(side.bind);
  }
  keys(c.worker.ssh, ['host','user','identity']); assert.match(c.worker.ssh.host, /^[A-Za-z0-9][A-Za-z0-9.-]*$/); assert.match(c.worker.ssh.user, portable); file(c.worker.ssh.identity, 1024 * 1024, true);
  for (const k of ['node','root','data_root','bind']) assert.notEqual(c.studio[k], c.worker[k]);
  privateDir(c.jobs_root, true); return c;
}
export function sshArgs(config) {
  const s = config.worker.ssh;
  return ['-o','BatchMode=yes','-o','IdentitiesOnly=yes','-o','StrictHostKeyChecking=yes','-o','ControlMaster=no','-i',s.identity,`${s.user}@${s.host}`];
}
export function identity(input_sha256, component_sha256, worker_node) {
  assert.match(input_sha256, hex); assert.match(component_sha256, hex); assert.match(worker_node, portable);
  const job_identity = { schema:'mct-first-real-app/job-v1', input_sha256, component_sha256, target: { ...TARGET }, worker_node };
  const job_id = hash(JSON.stringify(job_identity));
  return { job_identity, job_id, call_id:job_id, payload_json:JSON.stringify([JSON.stringify({sha256:input_sha256})]) };
}
function validateJob(job) { same(job, identity(job.job_identity.input_sha256, job.job_identity.component_sha256, job.job_identity.worker_node)); }
function syncDir(root) { const fd = openSync(root, 'r'); try { fsyncSync(fd); } finally { closeSync(fd); } }
function publish(root, name, value) {
  privateDir(root, true); const data = Buffer.from(JSON.stringify(value) + '\n'); assert.ok(data.length <= 65536);
  const dest = join(root,name), temp = join(root, `.${name}.${randomUUID()}`); let fd;
  try {
    fd = openSync(temp, 'wx', 0o600); writeFileSync(fd,data); fsyncSync(fd); closeSync(fd); fd = undefined;
    try { linkSync(temp,dest); } catch (e) { if (e.code !== 'EEXIST') throw e; same(file(dest,65536,true),data); }
    // Remove the temporary link before validating nlink and syncing the directory.
    unlinkSync(temp); syncDir(root); same(file(dest,65536,true),data);
  } finally { if (fd !== undefined) closeSync(fd); try { unlinkSync(temp); } catch (e) { if (e.code !== 'ENOENT') throw e; } }
}
export function saveRequest(root, job) { validateJob(job); publish(root, `${job.job_id}.request.json`, job); }
export async function withJobLock(root, job, fn) {
  validateJob(job); privateDir(root,true); const path = join(root, `${job.job_id}.lock`);
  const child = spawn('/usr/bin/shlock', ['-p', String(process.pid), '-f', path], {stdio:'ignore'});
  const code = await new Promise((res,rej) => { child.once('error',rej); child.once('close',res); }); assert.equal(code,0,'shlock unavailable or contended');
  try { return await fn(); } finally {
    try { const s = lstatSync(path); if (s.isFile() && !s.isSymbolicLink() && s.uid === process.getuid() && readFileSync(path,'utf8').trim() === String(process.pid)) unlinkSync(path); } catch (e) { if (e.code !== 'ENOENT') throw e; }
  }
}
function sortedUnique(a, min = 0) { assert.ok(Array.isArray(a) && a.length >= min); for (let i=0;i<a.length;i++) { assert.ok(typeof a[i] === 'string' && a[i].length > 0); if (i) assert.ok(a[i-1] < a[i]); } }
export function validateReport(r, job) {
  keys(r,['schema','sha256','package_count','registry_count','git_count','local_count','duplicates','selected']);
  assert.equal(r.schema,'mct-repo-report/v1'); assert.equal(r.sha256,job.job_identity.input_sha256);
  for (const k of ['package_count','registry_count','git_count','local_count']) assert.ok(Number.isSafeInteger(r[k]) && r[k] >= 0);
  assert.ok(r.package_count > 0 && r.package_count === r.registry_count + r.git_count + r.local_count);
  assert.ok(r.duplicates && typeof r.duplicates === 'object' && !Array.isArray(r.duplicates));
  for (const [name,versions] of Object.entries(r.duplicates)) { assert.ok(name.length); sortedUnique(versions,2); }
  keys(r.selected,['iroh','rusqlite','wasmtime','wasmtime-wasi']); for (const a of Object.values(r.selected)) sortedUnique(a);
  assert.ok(Buffer.byteLength(JSON.stringify(r)) <= 32768); return r;
}
function route(r,job) { assert.equal(r?.runtime_kind,'remote_peer'); assert.equal(r?.node_id,job.job_identity.worker_node); }
function inline(s) { assert.ok(typeof s === 'string' && /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(s)); const b = Buffer.from(s,'base64'); assert.ok(b.length <= 32768 && b.toString('base64') === s); return b; }
export function validateFirst(reply, job) {
  validateJob(job); assert.equal(reply?.outcome,'completed'); route(reply.route_taken,job);
  const decoded = JSON.parse(inline(reply.inline_result_payload_base64).toString('utf8')); same(decoded,reply.decoded_result);
  keys(decoded,['results']); assert.ok(Array.isArray(decoded.results) && decoded.results.length === 1); keys(decoded.results[0],['ok']);
  const text = decoded.results[0].ok; assert.equal(typeof text,'string'); assert.ok(Buffer.byteLength(text) <= 32768);
  return validateReport(JSON.parse(text),job);
}
export function validateReplay(reply, first, job) {
  assert.equal(reply?.outcome,'completed'); assert.equal(reply.protocol_reason,'idempotency_replay_completed'); route(reply.route_taken,job); same(reply.route_taken,first.route_taken); assert.equal(reply.inline_result_payload_base64,first.inline_result_payload_base64); inline(reply.inline_result_payload_base64);
}
export function validateWorkerSummary(s, job, cached = false) {
  assert.equal(s?.call_id,job.call_id); assert.equal(s.runtime_started,1); assert.ok(Number.isSafeInteger(s.runtime_completed) && (cached ? s.runtime_completed >= 1 : s.runtime_completed === 1)); assert.equal(s.runtime_failed,0);
}
export function makeResult(job, first, replay, summary, artifact_id, created_at = new Date().toISOString()) {
  const report = validateFirst(first,job); validateReplay(replay,first,job); validateWorkerSummary(summary,job);
  assert.ok(typeof artifact_id === 'string' && artifact_id.length > 0); assert.ok(typeof created_at === 'string' && Number.isFinite(Date.parse(created_at)));
  return { job, report, report_sha256:hash(JSON.stringify(report)), first_inline_base64:first.inline_result_payload_base64, route:first.route_taken, protocol_replay_reason:replay.protocol_reason, worker_summary:summary, artifact_id, created_at };
}
function validateResult(r,job) {
  keys(r,['job','report','report_sha256','first_inline_base64','route','protocol_replay_reason','worker_summary','artifact_id','created_at']); same(r.job,job);
  validateReport(r.report,job); assert.equal(r.report_sha256,hash(JSON.stringify(r.report))); route(r.route,job); assert.equal(r.protocol_replay_reason,'idempotency_replay_completed'); validateWorkerSummary(r.worker_summary,job,true);
  assert.ok(typeof r.artifact_id === 'string' && r.artifact_id.length > 0); assert.ok(typeof r.created_at === 'string' && Number.isFinite(Date.parse(r.created_at)));
  const decoded = JSON.parse(inline(r.first_inline_base64).toString('utf8')); keys(decoded,['results']); assert.ok(Array.isArray(decoded.results) && decoded.results.length === 1); keys(decoded.results[0],['ok']); assert.equal(typeof decoded.results[0].ok,'string'); assert.ok(Buffer.byteLength(decoded.results[0].ok) <= 32768); same(JSON.parse(decoded.results[0].ok),r.report);
}
export function saveResult(root,job,result) { validateJob(job); validateResult(result,job); publish(root,`${job.job_id}.result.json`,result); }
export function loadResult(root,job) {
  validateJob(job); privateDir(root,true); let bytes;
  try { bytes = file(join(root,`${job.job_id}.result.json`),65536,true); } catch (e) { if (e.code === 'ENOENT') return null; throw e; }
  const result = JSON.parse(bytes); validateResult(result,job); return result;
}
export async function getOrRun(root,job,startTransport) {
  return withJobLock(root,job,async () => { const cached = loadResult(root,job); if (cached) return {result:cached,source:'durable_cache'}; saveRequest(root,job); const result = await startTransport(job); saveResult(root,job,result); return {result,source:'executed'}; });
}

export class HelperClient {
  constructor(command,args = [],{spawnImpl = spawn, timeout = 150000} = {}) { this.command=command; this.args=args; this.spawnImpl=spawnImpl; this.timeout=timeout; this.stderr=''; this.buffer=''; this.pending=null; this.child=null; this.failure=null; }
  start() {
    assert.ok(!this.child); const c = this.child = this.spawnImpl(this.command,this.args,{stdio:['pipe','pipe','pipe']});
    this.exit = new Promise(resolve => { c.once('error',e => { this.fail(e); resolve({error:e}); }); c.once('close',(code,signal) => { if (code !== 0 || signal || this.pending || !this.stopping) this.fail(new Error(`helper exited: ${code ?? signal}`)); resolve({code,signal}); }); });
    c.stderr.on('data',b => { this.stderr = (this.stderr + b.toString()).slice(-65536); });
    c.stdout.on('data',b => { this.buffer += b.toString(); if (Buffer.byteLength(this.buffer) > 4*1024*1024) return this.fail(new Error('helper response too large')); const i=this.buffer.indexOf('\n'); if (i < 0) return; if (!this.pending || this.buffer.indexOf('\n',i+1) >= 0 || i !== this.buffer.length-1) return this.fail(new Error('unexpected helper output')); const line=this.buffer.slice(0,i); this.buffer=''; try { const v=JSON.parse(line); assert.equal(typeof v.ok,'boolean'); if (v.ok) { assert.ok(Object.hasOwn(v,'value')); this.finish(null,v.value); } else { assert.ok(Object.hasOwn(v,'error')); this.finish(new Error(typeof v.error === 'string' ? v.error : JSON.stringify(v.error))); } } catch(e) { this.fail(e); } });
    c.stdin.on('error',e => this.fail(e)); return this;
  }
  finish(e,v) { const p=this.pending; if (!p) return; this.pending=null; clearTimeout(p.timer); e ? p.reject(e) : p.resolve(v); }
  fail(e) { this.failure ??= e; this.finish(e); if (this.child && !this.child.killed) this.child.kill(); }
  request(value,{timeout=this.timeout} = {}) {
    assert.ok(this.child && !this.failure && !this.pending); const line=JSON.stringify(value)+'\n'; assert.ok(Buffer.byteLength(line) <= 4*1024*1024);
    return new Promise((resolve,reject) => { const timer=setTimeout(() => this.fail(new Error('helper request timeout')),timeout); this.pending={resolve,reject,timer}; this.child.stdin.write(line); });
  }
  async stop(stopRequest = {op:'stop'}) {
    assert.ok(this.child && !this.pending); this.stopping = true; let error; try { if (!this.failure && !this.child.stdin.destroyed) { const reply = await this.request(stopRequest); if (stopRequest.op === 'stop') { assert.equal(reply?.stopped,true); assert.equal(reply.escalated,false); assert.equal(reply.recovery_required,false); } } } catch(e) { error=e; }
    this.child.stdin.end(); let timer; const outcome = await Promise.race([this.exit,new Promise(resolve => { timer=setTimeout(() => resolve(null),this.timeout); })]); clearTimeout(timer);
    if (!outcome) { this.child.kill(); throw new Error('helper exit timeout'); } if (error) throw error; if (this.failure || outcome.error || outcome.code !== 0 || outcome.signal) throw this.failure ?? outcome.error ?? new Error('uncertain helper exit');
  }
}

export async function freshRun(config, job, bytes, {HelperClientClass=HelperClient, sleep=ms => new Promise(r => setTimeout(r,ms))} = {}) {
  validateJob(job);
  const {input, component, manifest} = bytes;
  assert.equal(hash(input),job.job_identity.input_sha256); assert.equal(hash(component),job.job_identity.component_sha256);
  const manifestSha = hash(manifest), componentSha = hash(component), inputSha = hash(input);
  const studio = { ...config.studio, client:new HelperClientClass(config.studio.helper,[]) };
  const worker = { ...config.worker, client:new HelperClientClass('/usr/bin/ssh',[...sshArgs(config),config.worker.helper]) };
  const hosts = [studio,worker], started = [];
  const req = (h,v) => h.client.request(v);
  const control = (h,path,body) => req(h,{op:'control',method:'POST',path,body});
  const paths = h => ({expected_config_path:join(h.root,'config.json'),expected_children_dir:join(h.root,'children'),expected_state_path:join(h.root,'state.sqlite')});
  async function start(h) {
    const r = await req(h,{op:'start'}); assert.ok(typeof r.endpoint_id === 'string' && r.endpoint_id.length); assert.equal(r.ticket?.endpoint_id,r.endpoint_id); assert.match(r.binary_sha256,hex);
    if (h.endpoint) { assert.equal(r.endpoint_id,h.endpoint); assert.equal(r.binary_sha256,h.binarySha); }
    else { h.endpoint=r.endpoint_id; h.binarySha=r.binary_sha256; }
    assert.equal(r.binary_sha256,h === studio ? hash(readFileSync(studio.binary)) : worker.binary_sha256); h.ticket=r.ticket;
  }
  async function stop(h) { const r=await req(h,{op:'stop'}); assert.equal(r?.stopped,true); assert.equal(r.escalated,false); assert.equal(r.recovery_required,false); }
  let result, failure;
  try {
    for (const h of hosts) { h.client.start(); started.push(h); }
    for (const h of hosts) await req(h,{op:'open',root:h.root,data_root:h.data_root,binary:h.binary,node:h.node,vision:config.vision,bind:h.bind});
    for (const h of hosts) await start(h);
    assert.notEqual(studio.endpoint,worker.endpoint);
    for (const [relative_path,data] of [[`snapshots/${inputSha}/Cargo.lock`,input],[`artifacts/${componentSha}/mct-repo-report.wasm`,component],[`artifacts/${componentSha}/child.toml`,manifest]]) {
      const sha256=hash(data), r=await req(worker,{op:'put',relative_path,sha256,content_base64:data.toString('base64')});
      assert.equal(r.path,relative_path); assert.equal(r.sha256,sha256); assert.equal(r.size_bytes,data.length); assert.equal(typeof r.reused,'boolean');
    }
    const expires_at=new Date(Date.now()+86400000).toISOString();
    for (const [receiver,caller] of [[studio,worker],[worker,studio]]) await control(receiver,'/peers/add',{expected_config_path:paths(receiver).expected_config_path,peer_node_id:caller.node,binding_id:`${receiver.node}-admits-${caller.node}`,endpoint_id:caller.endpoint,vision_id:config.vision,ticket:caller.ticket,binding_signature_ref:null,policy_revision:1,expires_at});
    for (const [receiver,caller] of [[studio,worker],[worker,studio]]) {
      const peer=(await req(receiver,{op:'peers'}))?.[caller.node];
      assert.match(peer?.binding_signature_ref ?? '',/^mct-ed25519-binding-v1:.+/); assert.equal(peer.binding_id,`${receiver.node}-admits-${caller.node}`); assert.equal(peer.endpoint_id,caller.endpoint); assert.equal(peer.vision_id,config.vision); assert.equal(peer.binding_state,'admitted'); assert.equal(peer.policy_revision,1); assert.ok(Date.parse(peer.expires_at)>Date.now());
      await control(caller,'/peers/proof',{expected_config_path:paths(caller).expected_config_path,peer_node_id:receiver.node,binding_id:peer.binding_id,policy_revision:peer.policy_revision,signature_ref:peer.binding_signature_ref,expires_at:peer.expires_at});
    }
    const stage=await control(worker,'/artifacts/stage',{source_root:join(worker.data_root,'artifacts',componentSha),manifest_path:'child.toml',component_path:'mct-repo-report.wasm',claimed_child_name:CHILD.name,claimed_artifact_version:CHILD.version,expected_digest:null,expected_manifest_digest:`sha256:${manifestSha}`,standing_source_authority_id:null,claimed_publisher:null,require_source_sidecars:false,children_dir:join(worker.root,'children'),state_path:join(worker.root,'state.sqlite')});
    assert.equal(stage.verification_outcome,'verified'); assert.match(stage.artifact_id ?? '',/^sha256:[a-f0-9]{64}$/);
    const approved=await control(worker,'/children/approve',{...paths(worker),expected_artifact_id:stage.artifact_id,child_name:CHILD.name,strict_integrity:true});
    assert.equal(approved.approval_state,'approved'); assert.equal(approved.assignment_state,'active'); assert.equal(approved.approval_artifact_id,stage.artifact_id);
    const root=join(worker.data_root,'snapshots',inputSha);
    const grant=await control(worker,'/watch/supporting-grant',{caller_node_id:studio.node,mutation_id:`first-real-app-directory:${job.job_id}`,...paths(worker),child_name:CHILD.name,expires_at,grant:{kind:'directory_read',canonical_root:root}});
    assert.equal(grant.grants?.length,1); const g=grant.grants[0]; assert.equal(g.grant_state,'active'); assert.equal(g.subject?.caller_node_id,studio.node); assert.equal(g.subject?.child_name,CHILD.name); assert.equal(g.scope?.resource_id,`file://${root}`); same(g.scope?.allowed_actions,['read-content']);
    await stop(studio); await stop(worker); await start(worker); await start(studio);
    const call={op:'call',job_id:job.job_id,payload_json:job.payload_json,target:TARGET}; let first;
    const deadline=Date.now()+120000;
    for (let attempts=0;;attempts++) { first=await req(studio,call); if (first?.outcome !== 'denied' || first.protocol_reason !== 'no_route') break; assert.ok(attempts<239 && Date.now()<deadline,'no_route deadline exceeded'); await sleep(500); }
    validateFirst(first,job); const replay=await req(studio,call); validateReplay(replay,first,job);
    await stop(worker); await stop(studio); const summary=await req(worker,{op:'ledger_call_summary',call_id:job.call_id}); validateWorkerSummary(summary,job);
    result=makeResult(job,first,replay,summary,stage.artifact_id);
  } catch (e) { failure=e; }
  const errors=[]; for (const h of started) { try { await h.client.stop(); } catch (e) { errors.push(e); } }
  if (errors.length) throw new AggregateError([...(failure ? [failure] : []),...errors],'helper cleanup uncertain');
  if (failure) throw failure;
  return result;
}
