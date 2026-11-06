import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { constants, lstatSync, fstatSync, mkdirSync, openSync, closeSync, readFileSync, writeFileSync, fsyncSync, linkSync, unlinkSync } from 'node:fs';
import { dirname, join, resolve, isAbsolute, basename } from 'node:path';
import { spawn } from 'node:child_process';

export const TARGET = Object.freeze({ namespace: 'patina:repo-report', interface_name: 'report@0.1.0', function_name: 'inspect' });
export const CHILD = Object.freeze({ name: 'mct-repo-report', version: '0.2.0' });
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
  return `${a}.${b}.${c}.${d}`;
}
export function validateKnownHosts(bytes,nat_ip) {
  assert.ok(Buffer.isBuffer(bytes)); assert.equal(bind(`${nat_ip}:1`),nat_ip);
  const text=bytes.toString('utf8'); assert.deepEqual(Buffer.from(text),bytes); assert.match(text,/^[\x09\x0a\x0d\x20-\x7e]*$/);
  const entries=text.split(/\r?\n/).map(line=>line.trim()).filter(line=>line&&!line.startsWith('#')); assert.equal(entries.length,1,'known_hosts must contain exactly one noncomment entry');
  const entry=entries[0]; assert.ok(!entry.startsWith('@'),'known_hosts markers are forbidden'); const fields=entry.split(/\s+/); assert.equal(fields.length,3,'known_hosts entry must contain only host, key type, and key');
  assert.equal(fields[0],nat_ip,'known_hosts host must be the exact NAT IP for port 22'); assert.equal(fields[1],'ssh-ed25519','known_hosts key type must be ssh-ed25519');
  const encoded=fields[2]; assert.match(encoded,/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/); const key=Buffer.from(encoded,'base64'); assert.equal(key.toString('base64'),encoded,'known_hosts key must use canonical base64');
  assert.equal(key.length,51,'invalid ssh-ed25519 public key blob'); assert.equal(key.readUInt32BE(0),11); assert.equal(key.subarray(4,15).toString('ascii'),'ssh-ed25519'); assert.equal(key.readUInt32BE(15),32); assert.equal(key.length,19+key.readUInt32BE(15));
  return {host:nat_ip,key_type:'ssh-ed25519',key:encoded};
}
export function loadConfig(path) {
  safePath(path); privateDir(dirname(path)); const c = JSON.parse(file(path, 65536, true));
  keys(c, ['version','vision','input','component','manifest','jobs_root','studio','worker']); assert.ok(c.version === 1 || c.version === 2);
  assert.match(c.vision, portable);
  for (const p of [c.input,c.component,c.manifest]) file(p, 2 * 1024 * 1024);
  assert.equal(basename(c.component), 'mct-repo-report.wasm'); assert.equal(basename(c.manifest), 'child.toml');
  for (const [name, side] of [['studio',c.studio],['worker',c.worker]]) {
    const workerFields = c.version === 2 ? ['ssh','binary_sha256','helper_sha256','lume'] : ['ssh','binary_sha256'];
    keys(side, ['node','root','data_root','binary','helper','bind', ...(name === 'worker' ? workerFields : [])]);
    if (name === 'worker') { assert.match(side.binary_sha256, hex); if (c.version === 2) assert.match(side.helper_sha256, hex); }
    assert.match(side.node, portable); for (const p of [side.root,side.data_root,side.binary,side.helper]) { safePath(p); assert.match(p, /^\/[A-Za-z0-9_./-]+$/); }
    assert.ok(Buffer.byteLength(join(side.root,'control.sock')) < 104); bind(side.bind);
  }
  keys(c.worker.ssh, c.version === 2 ? ['host','user','identity','known_hosts'] : ['host','user','identity']); assert.match(c.worker.ssh.host, /^[A-Za-z0-9][A-Za-z0-9.-]*$/); assert.match(c.worker.ssh.user, portable); file(c.worker.ssh.identity, 1024 * 1024, true);
  if (c.version === 2) {
    keys(c.worker.lume,['vm_name','machine_identifier_sha256','hardware_uuid_sha256','nat_ip']); assert.match(c.worker.lume.vm_name,portable); assert.match(c.worker.lume.machine_identifier_sha256,hex); assert.match(c.worker.lume.hardware_uuid_sha256,hex);
    assert.match(c.worker.lume.nat_ip,/^\d{1,3}(?:\.\d{1,3}){3}$/); assert.equal(bind(`${c.worker.lume.nat_ip}:1`),c.worker.lume.nat_ip); assert.equal(bind(c.worker.bind),c.worker.lume.nat_ip); assert.equal(c.worker.ssh.host,c.worker.lume.nat_ip);
    const knownHosts=file(c.worker.ssh.known_hosts,1024*1024,true); assert.notEqual(c.worker.ssh.known_hosts,c.worker.ssh.identity); validateKnownHosts(knownHosts,c.worker.lume.nat_ip);
  }
  for (const k of ['node','root','data_root','bind']) assert.notEqual(c.studio[k], c.worker[k]);
  privateDir(c.jobs_root, true); return c;
}
export function sshArgs(config) {
  const s = config.worker.ssh;
  return [...(config.version===2?['-F','/dev/null','-p','22']:[]),'-o','BatchMode=yes','-o','IdentitiesOnly=yes','-o','StrictHostKeyChecking=yes',...(config.version===2?['-o',`UserKnownHostsFile=${s.known_hosts}`,'-o','GlobalKnownHostsFile=/dev/null']:[]),'-o','ControlMaster=no',...(config.version===2?['-o','ClearAllForwardings=yes']:[]),'-i',s.identity,`${s.user}@${s.host}`];
}
export function identity(input_sha256, component_sha256, worker_node) {
  assert.match(input_sha256, hex); assert.match(component_sha256, hex); assert.match(worker_node, portable);
  const job_identity = { schema:'mct-first-real-app/job-v1', input_sha256, component_sha256, target: { ...TARGET }, worker_node };
  const job_id = hash(JSON.stringify(job_identity));
  return { job_identity, job_id, call_id:job_id, payload_json:JSON.stringify([JSON.stringify({sha256:input_sha256})]) };
}
function validateJob(job) { const i=job?.job_identity; assert.equal(i?.schema,'mct-first-real-app/job-v1'); same(job, identity(i.input_sha256,i.component_sha256,i.worker_node)); }
export function revocationEnabled(config) { assert.ok(config && (config.version === 1 || config.version === 2)); return config.version === 2; }
export function validateExecutorAttestation(config,evidence) { assert.equal(config?.version,2,'executor attestation requires config version 2'); keys(evidence,['vm_name','machine_identifier_sha256','hardware_uuid_sha256','nat_ip']); same(evidence,config.worker.lume); return evidence; }
export function validateLumeInspection(raw, {vm_name,nat_ip,machine_identifier_sha256}) {
  assert.match(vm_name,portable); assert.equal(bind(`${nat_ip}:1`),nat_ip); assert.match(machine_identifier_sha256,hex);
  const rows=Array.isArray(raw)?raw:Array.isArray(raw?.vms)?raw.vms:[raw];
  const matches=rows.filter(v=>(v?.name??v?.vmName??v?.vm_name)===vm_name); assert.equal(matches.length,1,'expected exactly one named Lume VM');
  const vm=matches[0], actualIp=vm.ip??vm.ipAddress??vm.ip_address??vm.natIp??vm.nat_ip; assert.equal(actualIp,nat_ip,'Lume NAT IP mismatch');
  const mode=vm.networkMode??vm.network_mode??vm.network?.mode; assert.equal(typeof mode,'string','Lume network mode missing'); assert.equal(mode.toLowerCase(),'nat','Lume network mode must be nat');
  const machine=vm.machineIdentifier??vm.machine_identifier??vm.machineId??vm.machine_id; assert.equal(typeof machine,'string','Lume machine identifier missing'); assert.equal(hash(machine.trim()),machine_identifier_sha256,'Lume machine identifier mismatch');
  return vm;
}
export function validateSmokeRuns(first,second,executor_attestation) {
  assert.ok(first?.source==='executed'||first?.source==='durable_cache'); assert.equal(second?.source,'durable_cache'); assert.deepEqual(second.durable_sources,{job:'durable_cache',revocation:'durable_cache'});
  if(first.source==='durable_cache')assert.deepEqual(first.durable_sources,{job:'durable_cache',revocation:'durable_cache'});else for(const source of Object.values(first.durable_sources??{}))assert.ok(source==='executed'||source==='durable_cache');
  validateAttestation(executor_attestation);same(first.executor_attestation,executor_attestation);same(second.executor_attestation,executor_attestation);assert.equal(second.durable_job_id,first.durable_job_id);assert.equal(second.report_sha256,first.report_sha256);assert.equal(first.worker_execution_count,1);assert.equal(second.worker_execution_count,1);assert.deepEqual(second.report,first.report);assert.deepEqual(second.revocation,first.revocation);return {first_process:first,second_process:second};
}
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
export function loadRequest(root, job) {
  validateJob(job); privateDir(root,true); let bytes;
  try { bytes=file(join(root,`${job.job_id}.request.json`),65536,true); } catch(e) { if(e.code==='ENOENT') return null; throw e; }
  const request=JSON.parse(bytes); validateJob(request); same(request,job); return request;
}
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
  keys(r,['schema','sha256','package_count','registry_count','git_count','local_count','packages','duplicates','selected']);
  assert.equal(r.schema,'mct-repo-report/v1'); assert.equal(r.sha256,job.job_identity.input_sha256);
  for (const k of ['package_count','registry_count','git_count','local_count']) assert.ok(Number.isSafeInteger(r[k]) && r[k] >= 0);
  assert.ok(r.package_count > 0 && r.package_count === r.registry_count + r.git_count + r.local_count);
  assert.ok(r.packages && typeof r.packages === 'object' && !Array.isArray(r.packages));
  for (const [name,versions] of Object.entries(r.packages)) { assert.ok(name.length); sortedUnique(versions,1); }
  assert.ok(r.duplicates && typeof r.duplicates === 'object' && !Array.isArray(r.duplicates));
  for (const [name,versions] of Object.entries(r.duplicates)) { assert.ok(name.length); sortedUnique(versions,2); same(versions,r.packages[name]); }
  keys(r.selected,['iroh','rusqlite','wasmtime','wasmtime-wasi']); for (const a of Object.values(r.selected)) sortedUnique(a);
  assert.ok(Buffer.byteLength(JSON.stringify(r)) <= 32768); return r;
}
function route(r,job) { assert.equal(r?.runtime_kind,'remote_peer'); assert.equal(r?.node_id,job.job_identity.worker_node); }
function inline(s) { assert.ok(typeof s === 'string' && /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(s)); const b = Buffer.from(s,'base64'); assert.ok(b.length <= 32768 && b.toString('base64') === s); return b; }
function validateCompletedReply(reply, job, replay) {
  validateJob(job); assert.equal(reply?.outcome,'completed'); replay ? assert.equal(reply.protocol_reason,'idempotency_replay_completed') : assert.notEqual(reply.protocol_reason,'idempotency_replay_completed'); route(reply.route_taken,job);
  const decoded=JSON.parse(inline(reply.inline_result_payload_base64).toString('utf8')); same(decoded,reply.decoded_result);
  keys(decoded,['results']); assert.ok(Array.isArray(decoded.results)&&decoded.results.length===1); keys(decoded.results[0],['ok']);
  const text=decoded.results[0].ok; assert.equal(typeof text,'string'); assert.ok(Buffer.byteLength(text)<=32768); return validateReport(JSON.parse(text),job);
}
export function validateFirst(reply, job) { return validateCompletedReply(reply,job,false); }
export function validateReplay(reply, first, job) {
  validateCompletedReply(reply,job,true); same(reply.route_taken,first.route_taken); assert.equal(reply.inline_result_payload_base64,first.inline_result_payload_base64);
}
function summaryCounts(s,call_id) { assert.equal(s?.call_id,call_id); for(const key of ['runtime_started','runtime_completed','runtime_failed','toy_allowed','toy_denied']) assert.ok(Number.isSafeInteger(s[key])&&s[key]>=0,`invalid ${key}`); return s; }
function executionIds(s) { if(Array.isArray(s.execution_call_ids)){assert.ok(s.execution_call_ids.length>0);for(let i=0;i<s.execution_call_ids.length;i++){assert.ok(typeof s.execution_call_ids[i]==='string'&&s.execution_call_ids[i].length>0);if(i)assert.ok(s.execution_call_ids[i-1]<s.execution_call_ids[i]);}if(Object.hasOwn(s,'execution_call_id')){assert.equal(s.execution_call_ids.length,1);assert.equal(s.execution_call_id,s.execution_call_ids[0]);}return s.execution_call_ids;}assert.ok(typeof s.execution_call_id==='string'&&s.execution_call_id.length>0);return [s.execution_call_id]; }
function validateZeroSummary(s, call_id) { summaryCounts(s,call_id); for (const k of ['runtime_started','runtime_completed','runtime_failed','toy_allowed','toy_denied']) assert.equal(s[k],0); assert.ok(!Object.hasOwn(s,'execution_call_id')); assert.ok(!Object.hasOwn(s,'execution_call_ids')); }
function validatePriorExecutionSummary(s,job) { summaryCounts(s,job.call_id); assert.ok(s.runtime_started>=1); assert.equal(s.runtime_failed,0); assert.equal(s.toy_denied,0); assert.ok(executionIds(s).length>=1); }
function delta(final,pre,key){return final[key]-pre[key];}
export function validateWorkerSummary(s, job, pre_execution_summary={call_id:job.call_id,runtime_started:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0}, execution_mode='fresh') {
  summaryCounts(s,job.call_id); assert.ok(['fresh','recovered_fresh','recovered_replay','recovered_recomputed'].includes(execution_mode));
  if(execution_mode==='recovered_replay'){validatePriorExecutionSummary(pre_execution_summary,job);assert.equal(delta(s,pre_execution_summary,'runtime_started'),0);assert.ok(delta(s,pre_execution_summary,'runtime_completed')>=0);assert.ok(delta(s,pre_execution_summary,'toy_allowed')>=0);}
  else {if(execution_mode==='fresh'||execution_mode==='recovered_fresh')validateZeroSummary(pre_execution_summary,job.call_id);else validatePriorExecutionSummary(pre_execution_summary,job);assert.equal(delta(s,pre_execution_summary,'runtime_started'),1);assert.ok(delta(s,pre_execution_summary,'runtime_completed')>=1);assert.ok(delta(s,pre_execution_summary,'toy_allowed')>=1);}
  assert.ok(s.runtime_completed>=1);assert.equal(delta(s,pre_execution_summary,'runtime_failed'),0);assert.equal(delta(s,pre_execution_summary,'toy_denied'),0);assert.ok(s.toy_allowed>=1);assert.ok(executionIds(s).length>=1);
}
function validateLegacyWorkerSummary(s,job) { assert.equal(s?.call_id,job.call_id); assert.equal(s.runtime_started,1); assert.ok(Number.isSafeInteger(s.runtime_completed)&&s.runtime_completed>=1); assert.equal(s.runtime_failed,0); }
export function validateStudioSummary(s, job) { validateZeroSummary(s,job.call_id); }
function validateEndpoints(endpoints) { keys(endpoints,['studio','worker']); assert.ok(typeof endpoints.studio === 'string' && endpoints.studio.length > 0); assert.ok(typeof endpoints.worker === 'string' && endpoints.worker.length > 0); assert.notEqual(endpoints.studio,endpoints.worker); }
function validateDigests(digests) { keys(digests,['studio_binary_sha256','studio_helper_sha256','worker_binary_sha256','worker_helper_sha256']); for (const value of Object.values(digests)) assert.match(value,hex); }
function validateAttestation(evidence){keys(evidence,['vm_name','machine_identifier_sha256','hardware_uuid_sha256','nat_ip']);assert.match(evidence.vm_name,portable);assert.match(evidence.machine_identifier_sha256,hex);assert.match(evidence.hardware_uuid_sha256,hex);assert.equal(bind(`${evidence.nat_ip}:1`),evidence.nat_ip);}
export function makeResult(job, first, replay, worker_summary, studio_summary, pre_execution_summary, artifact_id, endpoints, digests, executor_attestation, execution_mode='fresh', created_at = new Date().toISOString()) {
  assert.ok(['fresh','recovered_fresh','recovered_replay','recovered_recomputed'].includes(execution_mode));
  const report=execution_mode==='recovered_replay' ? validateCompletedReply(first,job,true) : validateFirst(first,job);
  validateReplay(replay,first,job); validateWorkerSummary(worker_summary,job,pre_execution_summary,execution_mode); validateStudioSummary(studio_summary,job);
  validateEndpoints(endpoints); validateDigests(digests); validateAttestation(executor_attestation); assert.ok(typeof artifact_id==='string'&&artifact_id.length>0); assert.ok(typeof created_at==='string'&&Number.isFinite(Date.parse(created_at)));
  return {schema:'mct-first-real-app/result-v2',job,report,report_sha256:hash(JSON.stringify(report)),first_inline_base64:first.inline_result_payload_base64,route:first.route_taken,protocol_replay_reason:replay.protocol_reason,execution_mode,pre_execution_summary,worker_summary,studio_summary,endpoints,digests,executor_attestation:{...executor_attestation},artifact_id,created_at};
}
function makeLegacyResult(job,first,replay,worker_summary,artifact_id,created_at=new Date().toISOString()){const report=validateFirst(first,job);validateReplay(replay,first,job);validateLegacyWorkerSummary(worker_summary,job);assert.ok(typeof artifact_id==='string'&&artifact_id.length>0);return {job,report,report_sha256:hash(JSON.stringify(report)),first_inline_base64:first.inline_result_payload_base64,route:first.route_taken,protocol_replay_reason:replay.protocol_reason,worker_summary:{call_id:worker_summary.call_id,runtime_started:worker_summary.runtime_started,runtime_completed:worker_summary.runtime_completed,runtime_failed:worker_summary.runtime_failed},artifact_id,created_at};}
function validateInlineReport(r,job) { const decoded=JSON.parse(inline(r.first_inline_base64).toString('utf8')); keys(decoded,['results']); assert.ok(Array.isArray(decoded.results)&&decoded.results.length===1); keys(decoded.results[0],['ok']); assert.equal(typeof decoded.results[0].ok,'string'); assert.ok(Buffer.byteLength(decoded.results[0].ok)<=32768); same(JSON.parse(decoded.results[0].ok),r.report); }
function validateLegacyResult(r,job) {
  keys(r,['job','report','report_sha256','first_inline_base64','route','protocol_replay_reason','worker_summary','artifact_id','created_at']); same(r.job,job); validateReport(r.report,job); assert.equal(r.report_sha256,hash(JSON.stringify(r.report))); route(r.route,job); assert.equal(r.protocol_replay_reason,'idempotency_replay_completed'); validateLegacyWorkerSummary(r.worker_summary,job); assert.ok(typeof r.artifact_id==='string'&&r.artifact_id.length>0); assert.ok(typeof r.created_at==='string'&&Number.isFinite(Date.parse(r.created_at))); validateInlineReport(r,job);
}
function validateResult(r,job) {
  if(!Object.hasOwn(r,'schema')) return validateLegacyResult(r,job);
  keys(r,['schema','job','report','report_sha256','first_inline_base64','route','protocol_replay_reason','execution_mode','pre_execution_summary','worker_summary','studio_summary','endpoints','digests','executor_attestation','artifact_id','created_at']); assert.equal(r.schema,'mct-first-real-app/result-v2'); same(r.job,job); assert.ok(['fresh','recovered_fresh','recovered_replay','recovered_recomputed'].includes(r.execution_mode));
  validateReport(r.report,job); assert.equal(r.report_sha256,hash(JSON.stringify(r.report))); route(r.route,job); assert.equal(r.protocol_replay_reason,'idempotency_replay_completed'); validateWorkerSummary(r.worker_summary,job,r.pre_execution_summary,r.execution_mode); validateStudioSummary(r.studio_summary,job); validateEndpoints(r.endpoints); validateDigests(r.digests); validateAttestation(r.executor_attestation); assert.ok(typeof r.artifact_id==='string'&&r.artifact_id.length>0); assert.ok(typeof r.created_at==='string'&&Number.isFinite(Date.parse(r.created_at))); validateInlineReport(r,job);
}
function loadResultFile(root,job,name) { let bytes; try { bytes=file(join(root,name),65536,true); } catch(e) { if(e.code==='ENOENT')return null; throw e; } const result=JSON.parse(bytes); validateResult(result,job); return result; }
export function saveResult(root,job,result) { validateJob(job); validateResult(result,job); privateDir(root,true); const baseName=`${job.job_id}.result.json`,base=loadResultFile(root,job,baseName); const name=result.schema==='mct-first-real-app/result-v2'&&base&&!base.schema?`${job.job_id}.result-v2.json`:baseName; publish(root,name,result); }
export function loadResult(root,job) {
  validateJob(job); privateDir(root,true); return loadResultFile(root,job,`${job.job_id}.result-v2.json`)??loadResultFile(root,job,`${job.job_id}.result.json`);
}
function validateReplyCheckpoint(checkpoint,job) { keys(checkpoint,['schema','job_id','call_id','reply','created_at']); assert.equal(checkpoint.schema,'mct-first-real-app/reply-v1'); assert.equal(checkpoint.job_id,job.job_id); assert.equal(checkpoint.call_id,job.call_id); assert.ok(typeof checkpoint.created_at==='string'&&Number.isFinite(Date.parse(checkpoint.created_at))); validateCompletedReply(checkpoint.reply,job,checkpoint.reply.protocol_reason==='idempotency_replay_completed'); }
export function loadReplyCheckpoint(root,job) { validateJob(job); privateDir(root,true); let bytes; try { bytes=file(join(root,`${job.job_id}.reply.json`),65536,true); } catch(e) { if(e.code==='ENOENT') return null; throw e; } const checkpoint=JSON.parse(bytes); validateReplyCheckpoint(checkpoint,job); return checkpoint; }
export function checkpointReply(root,job,reply,created_at=new Date().toISOString()) {
  validateCompletedReply(reply,job,reply.protocol_reason==='idempotency_replay_completed'); const existing=loadReplyCheckpoint(root,job);
  if(existing){same(existing.reply.route_taken,reply.route_taken);assert.equal(existing.reply.inline_result_payload_base64,reply.inline_result_payload_base64);return existing;}
  const checkpoint={schema:'mct-first-real-app/reply-v1',job_id:job.job_id,call_id:job.call_id,reply,created_at}; validateReplyCheckpoint(checkpoint,job); publish(root,`${job.job_id}.reply.json`,checkpoint); return checkpoint;
}
export async function getOrRun(root,job,startTransport,{requireCurrent=false}={}) {
  return withJobLock(root,job,async()=>{const cached=loadResult(root,job);if(cached&&(!requireCurrent||cached.schema==='mct-first-real-app/result-v2'))return {result:cached,source:'durable_cache'};const request=loadRequest(root,job);const recovery=request!==null;if(!request)saveRequest(root,job);const result=await startTransport(job,{recovery,checkpoint:loadReplyCheckpoint(root,job),checkpointReply:reply=>checkpointReply(root,job,reply)});saveResult(root,job,result);return {result,source:'executed'};});
}
export function revocationIdentity(job, studio_node, worker_node) {
  validateJob(job); assert.match(studio_node,portable); assert.match(worker_node,portable); assert.notEqual(studio_node,worker_node);
  return {schema:'mct-first-real-app/revocation-v1',job_id:job.job_id,probe_call_id:`${job.job_id}:receiver-revoked`,binding_id:`${worker_node}-admits-${studio_node}`,studio_node,worker_node};
}
export function validateRevocationReply(reply,job,revocation) { assert.equal(reply?.outcome,'denied'); assert.ok(reply.protocol_reason == null || (typeof reply.protocol_reason === 'string' && reply.protocol_reason.length > 0)); assert.notEqual(reply.protocol_reason,'no_route'); assert.equal(reply.submitted_call_id,revocation.probe_call_id); }
function validateRevocationLedger(s,revocation,baseline) {
  assert.equal(s?.call_id,revocation.probe_call_id); assert.equal(s.binding_id,revocation.binding_id); assert.equal(s.peer_node_id,revocation.studio_node); assert.equal(s.baseline_sequence,baseline.ledger_sequence); assert.equal(s.baseline_head_hash,baseline.ledger_head_hash); assert.ok(Number.isSafeInteger(s.revocation_count)&&s.revocation_count>=1); assert.ok(Number.isSafeInteger(s.rejection_count)&&s.rejection_count>=1); assert.ok(Number.isSafeInteger(s.revocation_sequence)&&Number.isSafeInteger(s.rejection_sequence)&&s.revocation_sequence>s.baseline_sequence&&s.revocation_sequence<s.rejection_sequence); assert.equal(s.hello_reason,'BindingRevoked'); validateZeroSummary(s,revocation.probe_call_id);
}
function requireCurrentResult(result,job) { validateResult(result,job); assert.equal(result.schema,'mct-first-real-app/result-v2','NIC-73 requires a current evidence result'); }
function validateRevocationIdentity(job,revocation) { same(revocation,revocationIdentity(job,revocation.studio_node,revocation.worker_node)); return revocation; }
function validateRequestedRevocation(value,job,revocation) { validateRevocationIdentity(job,revocation); for(const field of ['studio_node','worker_node','binding_id','probe_call_id'])assert.equal(value?.[field],revocation[field],`revocation ${field} mismatch`); }
export function makeRevocationIntent(job,result,revocation,created_at=new Date().toISOString()) { requireCurrentResult(result,job); validateRevocationIdentity(job,revocation); assert.ok(typeof created_at==='string'&&Number.isFinite(Date.parse(created_at))); return {schema:'mct-first-real-app/revocation-request-v2',job_id:job.job_id,probe_call_id:revocation.probe_call_id,binding_id:revocation.binding_id,studio_node:revocation.studio_node,worker_node:revocation.worker_node,endpoints:{...result.endpoints},executor_attestation:{...result.executor_attestation},created_at}; }
function validateRevocationIntent(intent,job,result,revocation) { keys(intent,['schema','job_id','probe_call_id','binding_id','studio_node','worker_node','endpoints','executor_attestation','created_at']); assert.equal(intent.schema,'mct-first-real-app/revocation-request-v2'); assert.equal(intent.job_id,job.job_id); validateRequestedRevocation(intent,job,revocation); const expected=makeRevocationIntent(job,result,revocation,intent.created_at); same(intent,expected); }
export function saveRevocationIntent(root,job,result,intent,revocation) { validateRevocationIntent(intent,job,result,revocation); publish(root,`${job.job_id}.revocation-request.json`,intent); }
export function loadRevocationIntent(root,job,result,revocation) { requireCurrentResult(result,job); validateRevocationIdentity(job,revocation); privateDir(root,true); let bytes; try { bytes=file(join(root,`${job.job_id}.revocation-request.json`),65536,true); } catch(e) { if(e.code==='ENOENT') return null; throw e; } const intent=JSON.parse(bytes); validateRevocationIntent(intent,job,result,revocation); return intent; }
function validateLedgerHead(head){keys(head,['ledger_sequence','ledger_head_hash']);assert.ok(Number.isSafeInteger(head.ledger_sequence)&&head.ledger_sequence>=0);if(head.ledger_sequence===0)assert.equal(head.ledger_head_hash,null);else assert.match(head.ledger_head_hash,hex);return head;}
export function makeRevocationBaseline(job,result,intent,worker_endpoint,ledger_head,created_at=new Date().toISOString()){requireCurrentResult(result,job);validateRevocationIntent(intent,job,result,revocationIdentity(job,intent.studio_node,intent.worker_node));assert.equal(worker_endpoint,result.endpoints.worker);validateLedgerHead(ledger_head);assert.ok(typeof created_at==='string'&&Number.isFinite(Date.parse(created_at)));return {schema:'mct-first-real-app/revocation-baseline-v1',intent,worker_endpoint,ledger_sequence:ledger_head.ledger_sequence,ledger_head_hash:ledger_head.ledger_head_hash,executor_attestation:{...result.executor_attestation},created_at};}
function validateRevocationBaseline(baseline,job,result,intent,revocation){keys(baseline,['schema','intent','worker_endpoint','ledger_sequence','ledger_head_hash','executor_attestation','created_at']);assert.equal(baseline.schema,'mct-first-real-app/revocation-baseline-v1');validateRevocationIntent(intent,job,result,revocation);validateRevocationIntent(baseline.intent,job,result,revocation);const expected=makeRevocationBaseline(job,result,intent,baseline.worker_endpoint,{ledger_sequence:baseline.ledger_sequence,ledger_head_hash:baseline.ledger_head_hash},baseline.created_at);same(baseline,expected);}
export function saveRevocationBaseline(root,job,result,intent,baseline,revocation){validateRevocationBaseline(baseline,job,result,intent,revocation);publish(root,`${job.job_id}.revocation-baseline.json`,baseline);}
export function loadRevocationBaseline(root,job,result,intent,revocation){validateRevocationIntent(intent,job,result,revocation);privateDir(root,true);let bytes;try{bytes=file(join(root,`${job.job_id}.revocation-baseline.json`),65536,true);}catch(e){if(e.code==='ENOENT')return null;throw e;}const baseline=JSON.parse(bytes);validateRevocationBaseline(baseline,job,result,intent,revocation);return baseline;}
export function makeRevocationReceipt(job,result,revocation,reply,studio_summary,worker_summary,ledger_summary,baseline,created_at=new Date().toISOString()) {
  requireCurrentResult(result,job); validateRevocationIdentity(job,revocation); validateRevocationBaseline(baseline,job,result,baseline.intent,revocation); validateRevocationReply(reply,job,revocation); validateZeroSummary(studio_summary,revocation.probe_call_id); validateZeroSummary(worker_summary,revocation.probe_call_id); validateRevocationLedger(ledger_summary,revocation,baseline); assert.ok(typeof created_at === 'string' && Number.isFinite(Date.parse(created_at)));
  return {schema:'mct-first-real-app/revocation-receipt-v2',job_id:job.job_id,probe_call_id:revocation.probe_call_id,binding_id:revocation.binding_id,studio_node:revocation.studio_node,worker_node:revocation.worker_node,endpoints:{...result.endpoints},executor_attestation:{...result.executor_attestation},baseline,outcome:reply.outcome,protocol_reason:reply.protocol_reason??null,studio_summary,worker_summary,ledger_summary,created_at};
}
function validateRevocationReceipt(receipt,job,result,revocation) {
  requireCurrentResult(result,job); keys(receipt,['schema','job_id','probe_call_id','binding_id','studio_node','worker_node','endpoints','executor_attestation','baseline','outcome','protocol_reason','studio_summary','worker_summary','ledger_summary','created_at']); assert.equal(receipt.schema,'mct-first-real-app/revocation-receipt-v2'); assert.equal(receipt.job_id,job.job_id); validateRequestedRevocation(receipt,job,revocation); same(receipt.endpoints,result.endpoints);same(receipt.executor_attestation,result.executor_attestation);validateRevocationBaseline(receipt.baseline,job,result,receipt.baseline.intent,revocation); assert.equal(receipt.outcome,'denied'); assert.ok(receipt.protocol_reason == null || (typeof receipt.protocol_reason === 'string' && receipt.protocol_reason.length > 0)); assert.notEqual(receipt.protocol_reason,'no_route'); validateZeroSummary(receipt.studio_summary,revocation.probe_call_id); validateZeroSummary(receipt.worker_summary,revocation.probe_call_id); validateRevocationLedger(receipt.ledger_summary,revocation,receipt.baseline); assert.ok(typeof receipt.created_at === 'string' && Number.isFinite(Date.parse(receipt.created_at)));
}
export function saveRevocationReceipt(root,job,receipt,revocation) { validateJob(job); const result=loadResult(root,job); assert.ok(result,'durable job result required before revocation receipt'); validateRevocationReceipt(receipt,job,result,revocation); publish(root,`${job.job_id}.revocation.json`,receipt); }
export function loadRevocationReceipt(root,job,result,revocation) { validateJob(job); assert.ok(result,'durable job result required before revocation'); validateRevocationIdentity(job,revocation); privateDir(root,true); let bytes; try { bytes=file(join(root,`${job.job_id}.revocation.json`),65536,true); } catch(e) { if(e.code==='ENOENT') return null; throw e; } const receipt=JSON.parse(bytes); validateRevocationReceipt(receipt,job,result,revocation); return receipt; }
export async function getOrRunRevocation(root,job,result,revocation,startTransport) { return withJobLock(root,job,async()=>{validateRevocationIdentity(job,revocation);const cached=loadRevocationReceipt(root,job,result,revocation);if(cached)return {receipt:cached,source:'durable_cache'};let intent=loadRevocationIntent(root,job,result,revocation);const recovery=intent!==null;if(!intent){intent=makeRevocationIntent(job,result,revocation);saveRevocationIntent(root,job,result,intent,revocation);}let baseline=loadRevocationBaseline(root,job,result,intent,revocation);const receipt=await startTransport(job,result,{intent,baseline,recovery,checkpointBaseline:value=>{saveRevocationBaseline(root,job,result,intent,value,revocation);baseline=loadRevocationBaseline(root,job,result,intent,revocation);return baseline;}});saveRevocationReceipt(root,job,receipt,revocation);return {receipt,source:'executed'};}); }


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

function runCommand(command,args,{spawnImpl=spawn,timeout=30000,maxOutput=65536}={}) {
  return new Promise((resolve,reject)=>{let timer;const child=spawnImpl(command,args,{stdio:['ignore','pipe','pipe']});let stdout='',stderr='',done=false;const finish=(error,value)=>{if(done)return;done=true;clearTimeout(timer);error?reject(error):resolve(value);};const append=(current,bytes)=>{const next=current+bytes.toString();assert.ok(Buffer.byteLength(next)<=maxOutput,'command output too large');return next;};child.stdout.on('data',b=>{try{stdout=append(stdout,b);}catch(e){child.kill();finish(e);}});child.stderr.on('data',b=>{try{stderr=append(stderr,b);}catch(e){child.kill();finish(e);}});child.once('error',e=>finish(e));child.once('close',(code,signal)=>code===0&&!signal?finish(null,{stdout,stderr}):finish(new Error(`command failed: ${code??signal}: ${stderr.slice(-2048)}`)));timer=setTimeout(()=>{child.kill();finish(new Error('command timeout'));},timeout);});
}
export async function verifyRemoteArtifacts(config,{runImpl=runCommand}={}) {
  if(config.version!==2)return null;
  const paths=[config.worker.binary,config.worker.helper];for(const path of paths){safePath(path);assert.match(path,/^\/[A-Za-z0-9_./-]+$/);}
  const statCommand=`set -eu; export LC_ALL=C; /usr/bin/id -u; /usr/bin/stat -f '%HT|%u|%l|%Lp' ${paths.join(' ')}`;
  const statResult=await runImpl('/usr/bin/ssh',[...sshArgs(config),statCommand]);const lines=statResult.stdout.trim().split('\n');assert.equal(lines.length,3,'invalid remote executable metadata');const expectedUid=Number(lines[0]);assert.ok(Number.isSafeInteger(expectedUid)&&expectedUid>=0,'invalid remote uid');
  for(let i=0;i<paths.length;i++){const match=/^(Regular File|Symbolic Link)\|(\d+)\|(\d+)\|([0-7]{3,4})$/.exec(lines[i+1]);assert.ok(match,`invalid remote metadata for ${paths[i]}`);const mode=Number.parseInt(match[4],8);assert.equal(match[1],'Regular File',`remote path is not a nonsymlinked file: ${paths[i]}`);assert.equal(Number(match[2]),expectedUid,`remote path ownership mismatch: ${paths[i]}`);assert.equal(Number(match[3]),1,`remote path must have one hard link: ${paths[i]}`);assert.ok((mode&0o100)!==0&&(mode&0o6022)===0,`remote path mode is not trusted executable: ${paths[i]}`);}
  const command=`/usr/bin/shasum -a 256 ${paths.join(' ')}`;
  const {stdout}=await runImpl('/usr/bin/ssh',[...sshArgs(config),command]); const found=new Map();
  for(const line of stdout.trim().split('\n')){const match=/^([a-f0-9]{64})\s+(.+)$/.exec(line);assert.ok(match,`invalid remote digest output: ${line}`);assert.ok(!found.has(match[2]));found.set(match[2],match[1]);}
  assert.equal(found.size,2);assert.equal(found.get(config.worker.binary),config.worker.binary_sha256,'remote daemon digest mismatch');assert.equal(found.get(config.worker.helper),config.worker.helper_sha256,'remote helper digest mismatch');
  return {binary_sha256:found.get(config.worker.binary),helper_sha256:found.get(config.worker.helper)};
}
function helperHosts(config,HelperClientClass) {
  const studio={...config.studio,client:new HelperClientClass(config.studio.helper,[]),role:'studio'};
  const worker={...config.worker,client:new HelperClientClass('/usr/bin/ssh',[...sshArgs(config),config.worker.helper]),role:'worker'};
  return {studio,worker,hosts:[studio,worker]};
}
function expectedHelperDigest(config,h) { if(h.role==='studio') return hash(readFileSync(h.helper)); return config.version===2 ? h.helper_sha256 : null; }
async function closeHelpers(started,failure) { const errors=[]; for(const h of started) { try { await h.client.stop(); } catch(e) { errors.push(e); } } if(errors.length) throw new AggregateError([...(failure?[failure]:[]),...errors],'helper cleanup uncertain'); if(failure) throw failure; }
export async function freshRun(config, job, bytes, {HelperClientClass=HelperClient,sleep=ms=>new Promise(r=>setTimeout(r,ms)),remoteHasher=verifyRemoteArtifacts,recovery=false,checkpoint=null,checkpointReply:saveCheckpoint=()=>{}} = {}) {
  validateJob(job); const {input,component,manifest}=bytes; assert.equal(hash(input),job.job_identity.input_sha256); assert.equal(hash(component),job.job_identity.component_sha256);
  const remoteDigests=await remoteHasher(config); const componentSha=hash(component),inputSha=hash(input); const {studio,worker,hosts}=helperHosts(config,HelperClientClass),started=[];
  const req=(h,v)=>h.client.request(v); const control=(h,path,body)=>req(h,{op:'control',method:'POST',path,body}); const readControl=(h,path)=>req(h,{op:'control',method:'GET',path,body:null});
  const paths=h=>({expected_config_path:join(h.root,'config.json'),expected_children_dir:join(h.root,'children'),expected_state_path:join(h.root,'state.sqlite')});
  async function open(h) { const r=await req(h,{op:'open',root:h.root,data_root:h.data_root,binary:h.binary,node:h.node,vision:config.vision,bind:h.bind}); assert.match(r.helper_sha256,hex); const expected=expectedHelperDigest(config,h); if(expected) assert.equal(r.helper_sha256,expected); if(h.role==='worker'&&remoteDigests)assert.equal(r.helper_sha256,remoteDigests.helper_sha256); h.helperSha=r.helper_sha256; }
  async function start(h) { const r=await req(h,{op:'start'}); assert.ok(typeof r.endpoint_id==='string'&&r.endpoint_id.length); assert.equal(r.ticket?.endpoint_id,r.endpoint_id); assert.match(r.binary_sha256,hex); assert.equal(r.helper_sha256,h.helperSha); if(h.endpoint){assert.equal(r.endpoint_id,h.endpoint);assert.equal(r.binary_sha256,h.binarySha);}else{h.endpoint=r.endpoint_id;h.binarySha=r.binary_sha256;} assert.equal(r.binary_sha256,h===studio?hash(readFileSync(studio.binary)):worker.binary_sha256); h.ticket=r.ticket; }
  async function stop(h){const r=await req(h,{op:'stop'});assert.equal(r?.stopped,true);assert.equal(r.escalated,false);assert.equal(r.recovery_required,false);}
  let result,failure;
  try {
    for(const h of hosts){h.client.start();started.push(h);} for(const h of hosts) await open(h); for(const h of hosts) await start(h); assert.notEqual(studio.endpoint,worker.endpoint);
    for(const [relative_path,data] of [[`snapshots/${inputSha}/Cargo.lock`,input],[`artifacts/${componentSha}/mct-repo-report.wasm`,component],[`artifacts/${componentSha}/child.toml`,manifest]]) { const sha256=hash(data),r=await req(worker,{op:'put',relative_path,sha256,content_base64:data.toString('base64')}); assert.equal(r.path,relative_path);assert.equal(r.sha256,sha256);assert.equal(r.size_bytes,data.length);assert.equal(typeof r.reused,'boolean'); }
    const expires_at=new Date(Date.now()+86400000).toISOString();
    for(const [receiver,caller] of [[studio,worker],[worker,studio]]) await control(receiver,'/peers/add',{expected_config_path:paths(receiver).expected_config_path,peer_node_id:caller.node,binding_id:`${receiver.node}-admits-${caller.node}`,endpoint_id:caller.endpoint,vision_id:config.vision,ticket:caller.ticket,binding_signature_ref:null,policy_revision:1,expires_at});
    for(const [receiver,caller] of [[studio,worker],[worker,studio]]) { const peer=(await req(receiver,{op:'peers'}))?.[caller.node]; assert.match(peer?.binding_signature_ref??'',/^mct-ed25519-binding-v1:.+/);assert.equal(peer.binding_id,`${receiver.node}-admits-${caller.node}`);assert.equal(peer.endpoint_id,caller.endpoint);assert.equal(peer.vision_id,config.vision);assert.equal(peer.binding_state,'admitted');assert.equal(peer.policy_revision,1);assert.ok(Date.parse(peer.expires_at)>Date.now()); await control(caller,'/peers/proof',{expected_config_path:paths(caller).expected_config_path,peer_node_id:receiver.node,binding_id:peer.binding_id,policy_revision:peer.policy_revision,signature_ref:peer.binding_signature_ref,expires_at:peer.expires_at}); }
    const stage=await control(worker,'/artifacts/stage',{source_root:join(worker.data_root,'artifacts',componentSha),manifest_path:'child.toml',component_path:'mct-repo-report.wasm',claimed_child_name:CHILD.name,claimed_artifact_version:CHILD.version,expected_digest:null,expected_manifest_digest:null,standing_source_authority_id:null,claimed_publisher:null,require_source_sidecars:false,children_dir:join(worker.root,'children'),state_path:join(worker.root,'state.sqlite')}); assert.equal(stage.verification_outcome,'verified');assert.match(stage.artifact_id??'',/^sha256:[a-f0-9]{64}$/);
    const approved=await control(worker,'/children/approve',{...paths(worker),expected_artifact_id:stage.artifact_id,child_name:CHILD.name,strict_integrity:true}); assert.equal(approved.approval_state,'approved');assert.equal(approved.assignment_state,'active');assert.equal(approved.approval_artifact_id,stage.artifact_id);
    await stop(studio);await stop(worker);await start(worker);await start(studio);
    const root=join(worker.data_root,'snapshots',inputSha); const grant=await control(worker,'/watch/supporting-grant',{caller_node_id:studio.node,mutation_id:`first-real-app-directory:${job.job_id}`,...paths(worker),child_name:CHILD.name,expires_at,grant:{kind:'directory_read',canonical_root:root}}); assert.equal(grant.grants?.length,1); const g=grant.grants[0];assert.equal(g.grant_state,'active');assert.equal(g.subject?.caller_node_id,studio.node);assert.equal(g.subject?.child_name,CHILD.name);assert.equal(g.scope?.resource_id,`file://${root}`);same(g.scope?.allowed_actions,['read-content']);
    const mutation=grant.mutation_result;assert.equal(mutation?.status,'committed');assert.equal(mutation.mutation_id,`first-real-app-directory:${job.job_id}`);assert.ok(mutation.grants_authority&&typeof mutation.grants_authority==='object');
    await stop(studio);await stop(worker);await start(worker);await start(studio);
    const authorityDeadline=Date.now()+120000;for(;;){try{const current=await readControl(worker,'/receiver-authority');assert.equal(current?.mother_node_id,mutation.grants_authority.mother_node_id);assert.ok(Number.isSafeInteger(current?.generation)&&current.generation>=mutation.grants_authority.generation);assert.ok(typeof current.authority_epoch==='string'&&current.authority_epoch.length>0);assert.ok(typeof current.source_authority_observation_id==='string'&&current.source_authority_observation_id.length>0);break;}catch(error){assert.ok(Date.now()<authorityDeadline,'receiver authority readiness deadline exceeded');await sleep(500);}}
    await stop(worker);await stop(studio);const preExecution=await req(worker,{op:'ledger_call_summary',call_id:job.call_id});await start(worker);await start(studio);
    const call={op:'call',job_id:job.job_id,payload_json:job.payload_json,target:TARGET};let first;const deadline=Date.now()+120000;for(let attempts=0;;attempts++){first=await req(studio,call);if(first?.outcome!=='denied'||first.protocol_reason!=='no_route')break;assert.ok(attempts<239&&Date.now()<deadline,'no_route deadline exceeded');await sleep(500);}
    const savedCheckpoint=saveCheckpoint(first);if(savedCheckpoint)checkpoint=savedCheckpoint;let replay,executionMode;
    if(recovery){let prior=false;try{validatePriorExecutionSummary(preExecution,job);prior=true;}catch(error){validateZeroSummary(preExecution,job.call_id);}if(prior&&first.protocol_reason==='idempotency_replay_completed'){validateCompletedReply(first,job,true);replay=first;executionMode='recovered_replay';}else{validateFirst(first,job);replay=await req(studio,call);validateReplay(replay,first,job);executionMode=prior?'recovered_recomputed':'recovered_fresh';}}
    else{validateZeroSummary(preExecution,job.call_id);validateFirst(first,job);replay=await req(studio,call);validateReplay(replay,first,job);executionMode='fresh';}
    if(checkpoint){same(checkpoint.reply.route_taken,first.route_taken);assert.equal(checkpoint.reply.inline_result_payload_base64,first.inline_result_payload_base64);}await stop(worker);await stop(studio);const workerSummary=await req(worker,{op:'ledger_call_summary',call_id:job.call_id});const studioSummary=await req(studio,{op:'ledger_call_summary',call_id:job.call_id});validateWorkerSummary(workerSummary,job,preExecution,executionMode);validateStudioSummary(studioSummary,job);
    result=config.version===2?makeResult(job,first,replay,workerSummary,studioSummary,preExecution,stage.artifact_id,{studio:studio.endpoint,worker:worker.endpoint},{studio_binary_sha256:studio.binarySha,studio_helper_sha256:studio.helperSha,worker_binary_sha256:worker.binarySha,worker_helper_sha256:worker.helperSha},config.worker.lume,executionMode):makeLegacyResult(job,first,replay,workerSummary,stage.artifact_id);
  } catch(e){failure=e;} await closeHelpers(started,failure); return result;
}
export async function revocationRun(config,job,result,{HelperClientClass=HelperClient,remoteHasher=verifyRemoteArtifacts,intent,baseline=null,recovery=false,checkpointBaseline=()=>{throw new Error('durable revocation baseline publisher required');}}={}) {
  validateJob(job);requireCurrentResult(result,job);validateExecutorAttestation(config,result.executor_attestation);const revocation=revocationIdentity(job,config.studio.node,config.worker.node);assert.ok(intent,'durable revocation intent required');validateRevocationIntent(intent,job,result,revocation);if(baseline)validateRevocationBaseline(baseline,job,result,intent,revocation);const remoteDigests=await remoteHasher(config);const {studio,worker,hosts}=helperHosts(config,HelperClientClass),started=[];const req=(h,v)=>h.client.request(v);const control=(h,path,body)=>req(h,{op:'control',method:'POST',path,body});
  async function open(h){const r=await req(h,{op:'open',root:h.root,data_root:h.data_root,binary:h.binary,node:h.node,vision:config.vision,bind:h.bind});assert.match(r.helper_sha256,hex);assert.equal(r.helper_sha256,result.digests[`${h.role}_helper_sha256`]);if(h.role==='worker'&&remoteDigests)assert.equal(r.helper_sha256,remoteDigests.helper_sha256);h.helperSha=r.helper_sha256;}
  async function start(h){const r=await req(h,{op:'start'});assert.equal(r.endpoint_id,result.endpoints[h.role]);assert.equal(r.ticket?.endpoint_id,r.endpoint_id);assert.equal(r.binary_sha256,result.digests[`${h.role}_binary_sha256`]);assert.equal(r.helper_sha256,result.digests[`${h.role}_helper_sha256`]);h.endpoint=r.endpoint_id;}
  async function stop(h){const r=await req(h,{op:'stop'});assert.equal(r?.stopped,true);assert.equal(r.escalated,false);assert.equal(r.recovery_required,false);}
  let receipt,failure;
  try {for(const h of hosts){h.client.start();started.push(h);}for(const h of hosts)await open(h);for(const h of hosts)await start(h);assert.notEqual(studio.endpoint,worker.endpoint);await stop(worker);const head=await req(worker,{op:'ledger_head',baseline_sequence:baseline?.ledger_sequence??0,baseline_head_hash:baseline?.ledger_head_hash??null});validateLedgerHead(head);if(!baseline){baseline=checkpointBaseline(makeRevocationBaseline(job,result,intent,worker.endpoint,head));assert.ok(baseline,'durable revocation baseline publisher returned no evidence');validateRevocationBaseline(baseline,job,result,intent,revocation);}else assert.ok(head.ledger_sequence>=baseline.ledger_sequence);await start(worker);let peer=(await req(worker,{op:'peers'}))?.[studio.node];assert.equal(peer?.binding_id,revocation.binding_id);if(peer.binding_state==='admitted'){await control(worker,'/peers/revoke',{expected_config_path:join(worker.root,'config.json'),peer_node_id:studio.node});peer=(await req(worker,{op:'peers'}))?.[studio.node];}assert.equal(peer?.binding_state,'revoked');const reply=await req(studio,{op:'call',job_id:revocation.probe_call_id,payload_json:job.payload_json,target:TARGET});validateRevocationReply(reply,job,revocation);await stop(worker);await stop(studio);const workerSummary=await req(worker,{op:'ledger_call_summary',call_id:revocation.probe_call_id});const studioSummary=await req(studio,{op:'ledger_call_summary',call_id:revocation.probe_call_id});validateZeroSummary(workerSummary,revocation.probe_call_id);validateZeroSummary(studioSummary,revocation.probe_call_id);const ledgerSummary=await req(worker,{op:'ledger_revocation_summary',call_id:revocation.probe_call_id,binding_id:revocation.binding_id,peer_node_id:studio.node,after_sequence:baseline.ledger_sequence,baseline_head_hash:baseline.ledger_head_hash});if(!recovery)assert.equal(ledgerSummary.revocation_count,1,'fresh revocation must append exactly one observation');receipt=makeRevocationReceipt(job,result,revocation,reply,studioSummary,workerSummary,ledgerSummary,baseline);}catch(e){failure=e;}await closeHelpers(started,failure);return receipt;
}
