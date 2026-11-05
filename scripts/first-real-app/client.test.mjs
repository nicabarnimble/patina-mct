import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, chmodSync, rmSync, readFileSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { identity, loadConfig, sshArgs, saveRequest, saveResult, loadResult, validateFirst, validateReplay, validateWorkerSummary, makeResult, getOrRun, HelperClient } from './client.mjs';
const dir = () => mkdtempSync(join(homedir(), '.mct-client-test-'));
const job = identity('a'.repeat(64),'b'.repeat(64),'worker');
const report = {schema:'mct-repo-report/v1',sha256:'a'.repeat(64),package_count:2,registry_count:1,git_count:0,local_count:1,duplicates:{foo:['1','2']},selected:{iroh:[],rusqlite:[],wasmtime:['1'],'wasmtime-wasi':[]}};
function fixtures(r = report) {
  const decoded_result={results:[{ok:JSON.stringify(r)}]}; const first={outcome:'completed',route_taken:{runtime_kind:'remote_peer',node_id:'worker'},decoded_result,inline_result_payload_base64:Buffer.from(JSON.stringify(decoded_result)).toString('base64')};
  const replay={...first,protocol_reason:'idempotency_replay_completed'}; const summary={call_id:job.call_id,runtime_started:1,runtime_completed:1,runtime_failed:0}; return {first,replay,summary};
}
test('identity is immutable and changes with all inputs',() => {
  assert.notEqual(job.job_id,identity('c'.repeat(64),'b'.repeat(64),'worker').job_id); assert.notEqual(job.job_id,identity('a'.repeat(64),'c'.repeat(64),'worker').job_id); assert.notEqual(job.job_id,identity('a'.repeat(64),'b'.repeat(64),'other').job_id); assert.equal(job.call_id,job.job_id); assert.deepEqual(JSON.parse(job.payload_json),[JSON.stringify({sha256:'a'.repeat(64)})]);
});
test('config validates paths, permissions, schema and ssh options',() => {
  const root=dir(); try {
    for (const name of ['input','component','manifest','key']) writeFileSync(join(root,name),'x',{mode:0o600});
    const side=(node,port) => ({node,root:`${root}/${node}`,data_root:`${root}/${node}-data`,binary:`${root}/bin`,helper:`${root}/helper`,bind:`10.1.2.3:${port}`});
    const c={version:1,vision:'vision',input:join(root,'input'),component:join(root,'component'),manifest:join(root,'manifest'),jobs_root:join(root,'jobs'),studio:side('studio',1000),worker:{...side('worker',1001),ssh:{host:'host.local',user:'user',identity:join(root,'key')}}};
    const p=join(root,'config'); const put = v => writeFileSync(p,JSON.stringify(v),{mode:0o600}); put(c); assert.deepEqual(loadConfig(p),c);
    assert.deepEqual(sshArgs(c),['-o','BatchMode=yes','-o','IdentitiesOnly=yes','-o','StrictHostKeyChecking=yes','-o','ControlMaster=no','-i',c.worker.ssh.identity,'user@host.local']);
    put({...c,extra:1}); assert.throws(()=>loadConfig(p)); put({...c,worker:{...c.worker,extra:1}}); assert.throws(()=>loadConfig(p)); put({...c,input:'/tmp/x'}); assert.throws(()=>loadConfig(p)); put({...c,studio:{...c.studio,bind:'8.8.8.8:80'}}); assert.throws(()=>loadConfig(p)); symlinkSync(c.input,join(root,'link')); put({...c,input:join(root,'link')}); assert.throws(()=>loadConfig(p)); put(c); chmodSync(p,0o644); assert.throws(()=>loadConfig(p)); chmodSync(p,0o600); chmodSync(c.worker.ssh.identity,0o644); assert.throws(()=>loadConfig(p));
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('reply, replay and summary validation fail closed',() => {
  const {first,replay,summary}=fixtures(); assert.deepEqual(validateFirst(first,job),report); validateReplay(replay,first,job); validateWorkerSummary(summary,job);
  for (const bad of [{...first,outcome:'failed'},{...first,route_taken:{runtime_kind:'remote_peer',node_id:'other'}},{...first,decoded_result:{results:[{err:'x'}]}},{...first,inline_result_payload_base64:'!'}]) assert.throws(()=>validateFirst(bad,job));
  for (const r of [{...report,sha256:'c'.repeat(64)},{...report,schema:'wrong'},{...report,package_count:4},{...report,extra:1},{...report,duplicates:{foo:['2','1']}},{...report,selected:{...report.selected,iroh:['1','1']}}]) assert.throws(()=>validateFirst(fixtures(r).first,job));
  assert.throws(()=>validateReplay({...replay,protocol_reason:'other'},first,job)); assert.throws(()=>validateReplay({...replay,inline_result_payload_base64:'e30='},first,job)); assert.throws(()=>validateWorkerSummary({...summary,runtime_started:2},job)); assert.throws(()=>validateWorkerSummary({...summary,runtime_failed:1},job));
});
test('durable publish, corruption and fresh cache avoid transport',async () => {
  const root=dir(); try {
    const {first,replay,summary}=fixtures(); const result=makeResult(job,first,replay,summary,'artifact');
    saveRequest(root,job); saveRequest(root,job); saveResult(root,job,result); saveResult(root,job,result); assert.deepEqual(loadResult(root,job),result);
    assert.deepEqual(await getOrRun(root,job,()=>{throw new Error('transport spawned');}),result);
    const p=join(root,`${job.job_id}.result.json`); const obj=JSON.parse(readFileSync(p)); obj.report.package_count=99; writeFileSync(p,JSON.stringify(obj)); assert.throws(()=>loadResult(root,job)); assert.throws(()=>saveResult(root,job,result));
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('request is durable before transport and failure leaves it in place',async () => {
  const root=dir(); try {
    await assert.rejects(getOrRun(root,job,() => { assert.deepEqual(JSON.parse(readFileSync(join(root,`${job.job_id}.request.json`))),job); throw new Error('offline'); }),/offline/);
    assert.deepEqual(JSON.parse(readFileSync(join(root,`${job.job_id}.request.json`))),job);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('HelperClient uses line protocol and waits for clean exit',async () => {
  const source=`const rl=require('readline').createInterface({input:process.stdin});rl.on('line',s=>{const v=JSON.parse(s);console.log(JSON.stringify({ok:true,value:v}));if(v.op==='stop')rl.close()})`;
  const h=new HelperClient(process.execPath,['-e',source],{timeout:2000}); h.start(); assert.deepEqual(await h.request({op:'echo'}),{op:'echo'}); await h.stop();
});
