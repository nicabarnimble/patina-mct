import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, chmodSync, rmSync, readFileSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { identity, loadConfig, sshArgs, saveRequest, saveResult, loadResult, validateFirst, validateReplay, validateWorkerSummary, makeResult, getOrRun, HelperClient, freshRun, hash } from './client.mjs';
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
    for (const name of ['input','mct-repo-report.wasm','child.toml','key']) writeFileSync(join(root,name),'x',{mode:0o600});
    const side=(node,port) => ({node,root:`${root}/${node}`,data_root:`${root}/${node}-data`,binary:`${root}/bin`,helper:`${root}/helper`,bind:`10.1.2.3:${port}`});
    const c={version:1,vision:'vision',input:join(root,'input'),component:join(root,'mct-repo-report.wasm'),manifest:join(root,'child.toml'),jobs_root:join(root,'jobs'),studio:side('studio',1000),worker:{...side('worker',1001),binary_sha256:'a'.repeat(64),ssh:{host:'host.local',user:'user',identity:join(root,'key')}}};
    const p=join(root,'config'); const put = v => writeFileSync(p,JSON.stringify(v),{mode:0o600}); put(c); assert.deepEqual(loadConfig(p),c);
    assert.deepEqual(sshArgs(c),['-o','BatchMode=yes','-o','IdentitiesOnly=yes','-o','StrictHostKeyChecking=yes','-o','ControlMaster=no','-i',c.worker.ssh.identity,'user@host.local']);
    put({...c,extra:1}); assert.throws(()=>loadConfig(p)); put({...c,worker:{...c.worker,extra:1}}); assert.throws(()=>loadConfig(p)); put({...c,input:'/tmp/x'}); assert.throws(()=>loadConfig(p)); put({...c,studio:{...c.studio,bind:'8.8.8.8:80'}}); assert.throws(()=>loadConfig(p)); symlinkSync(c.input,join(root,'link')); put({...c,input:join(root,'link')}); assert.throws(()=>loadConfig(p)); put(c); chmodSync(p,0o644); assert.throws(()=>loadConfig(p)); chmodSync(p,0o600); chmodSync(c.worker.ssh.identity,0o644); assert.throws(()=>loadConfig(p));
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('reply, replay and summary validation fail closed',() => {
  const {first,replay,summary}=fixtures(); assert.deepEqual(validateFirst(first,job),report); validateReplay(replay,first,job); validateWorkerSummary(summary,job);
  for (const bad of [{...first,outcome:'failed'},{...first,route_taken:{runtime_kind:'remote_peer',node_id:'other'}},{...first,decoded_result:{results:[{err:'x'}]}},{...first,inline_result_payload_base64:'!'}]) assert.throws(()=>validateFirst(bad,job));
  for (const r of [{...report,sha256:'c'.repeat(64)},{...report,schema:'wrong'},{...report,package_count:4},{...report,extra:1},{...report,duplicates:{foo:['2','1']}},{...report,selected:{...report.selected,iroh:['1','1']}}]) assert.throws(()=>validateFirst(fixtures(r).first,job));
  assert.throws(()=>validateReplay({...replay,protocol_reason:'other'},first,job)); assert.throws(()=>validateReplay({...replay,inline_result_payload_base64:'e30='},first,job)); assert.throws(()=>validateWorkerSummary({...summary,runtime_started:2},job)); assert.throws(()=>validateWorkerSummary({...summary,runtime_completed:0},job)); assert.throws(()=>validateWorkerSummary({...summary,runtime_failed:1},job));
});
test('durable publish, corruption and fresh cache avoid transport',async () => {
  const root=dir(); try {
    const {first,replay,summary}=fixtures(); const result=makeResult(job,first,replay,summary,'artifact');
    saveRequest(root,job); saveRequest(root,job); saveResult(root,job,result); saveResult(root,job,result); assert.deepEqual(loadResult(root,job),result);
    assert.deepEqual(await getOrRun(root,job,()=>{throw new Error('transport spawned');}),{result,source:'durable_cache'});
    const p=join(root,`${job.job_id}.result.json`); const obj=JSON.parse(readFileSync(p)); obj.report.package_count=99; writeFileSync(p,JSON.stringify(obj)); assert.throws(()=>loadResult(root,job)); assert.throws(()=>saveResult(root,job,result));
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('fresh result is published and identifies execution source',async () => {
  const root=dir(); try {
    const {first,replay,summary}=fixtures(); const result=makeResult(job,first,replay,summary,'artifact');
    assert.deepEqual(await getOrRun(root,job,async () => result),{result,source:'executed'});
    assert.deepEqual(loadResult(root,job),result);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('request is durable before transport and failure leaves it in place',async () => {
  const root=dir(); try {
    await assert.rejects(getOrRun(root,job,() => { assert.deepEqual(JSON.parse(readFileSync(join(root,`${job.job_id}.request.json`))),job); throw new Error('offline'); }),/offline/);
    assert.deepEqual(JSON.parse(readFileSync(join(root,`${job.job_id}.request.json`))),job);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('freshRun exchanges signed records and validates the execution; failures clean up',async () => {
  const root=dir(); try {
    const bytes={input:Buffer.from('input'),component:Buffer.from('wasm'),manifest:Buffer.from('manifest')};
    const j=identity(hash(bytes.input),hash(bytes.component),'worker');
    const r={...report,sha256:hash(bytes.input)}; const decoded_result={results:[{ok:JSON.stringify(r)}]};
    const first={outcome:'completed',route_taken:{runtime_kind:'remote_peer',node_id:'worker'},decoded_result,inline_result_payload_base64:Buffer.from(JSON.stringify(decoded_result)).toString('base64')};
    const binary=join(root,'bin'); writeFileSync(binary,'daemon');
    const side=node=>({node,root:`${root}/${node}`,data_root:`${root}/${node}-data`,binary,helper:`${root}/helper`,bind:'10.0.0.1:1234'});
    const c={vision:'vision',studio:side('studio'),worker:{...side('worker'),binary_sha256:hash('daemon'),ssh:{user:'user',host:'host',identity:'/key'}}};
    let events=[], peers={}, behavior='', calls=0;
    const authority={mother_node_id:'worker',authority_epoch:'epoch',generation:2,source_authority_observation_id:'observation'};
    class Fake {
      constructor(cmd) { this.node=cmd==='/usr/bin/ssh'?'worker':'studio'; }
      start() { events.push([this.node,'helper-start']); }
      async stop() { events.push([this.node,'helper-stop']); }
      async request(q) {
        events.push([this.node,q.op,q.path,q.body]); const side=c[this.node];
        if(q.op==='open') return {};
        if(q.op==='start') { this.starts=(this.starts??0)+1; const endpoint_id=this.node+(behavior==='endpoint' && this.starts>1?'changed':''); return {endpoint_id,ticket:{endpoint_id},binary_sha256:hash('daemon')}; }
        if(q.op==='stop') return {stopped:true,escalated:false,recovery_required:false};
        if(q.op==='put') return {path:q.relative_path,sha256:q.sha256,size_bytes:Buffer.from(q.content_base64,'base64').length,reused:false};
        if(q.op==='peers') return peers[this.node];
        if(q.op==='control') {
          if(q.path==='/peers/add') { const b=q.body; peers[this.node]={[b.peer_node_id]:{...b,binding_state:'admitted',binding_signature_ref:behavior==='signature'?'bad':'mct-ed25519-binding-v1:signed'}}; return {}; }
          if(q.path==='/peers/proof') { assert.equal(q.body.signature_ref,'mct-ed25519-binding-v1:signed'); return {}; }
          if(q.path==='/artifacts/stage') return {verification_outcome:behavior==='stage'?'rejected':'verified',artifact_id:`sha256:${'f'.repeat(64)}`};
          if(q.path==='/children/approve') return {approval_state:'approved',assignment_state:'active',approval_artifact_id:q.body.expected_artifact_id};
          if(q.path==='/watch/supporting-grant') return {grants:behavior==='grant'?[]:[{grant_state:'active',subject:{caller_node_id:'studio',child_name:'mct-repo-report'},scope:{resource_id:`file://${q.body.grant.canonical_root}`,allowed_actions:['read-content']}}],mutation_result:{status:'committed',mutation_id:`first-real-app-directory:${j.job_id}`,grants_authority:authority}};
          if(q.path==='/receiver-authority') return authority;
        }
        if(q.op==='call') { calls++; if (behavior==='denied') return {outcome:'denied',protocol_reason:'other'}; if (behavior==='retry' && calls===1) return {outcome:'denied',protocol_reason:'no_route'}; if (behavior==='bounded') return {outcome:'denied',protocol_reason:'no_route'}; return calls===1 || (behavior==='retry' && calls===2) ? first : {...first,protocol_reason:behavior==='replay'?'wrong':'idempotency_replay_completed'}; }
        if(q.op==='ledger_call_summary') return {call_id:j.call_id,runtime_started:behavior==='count'?2:1,runtime_completed:1,runtime_failed:0};
        throw Error(`unexpected ${q.op}`);
      }
    }
    async function run(mode) { events=[];peers={};calls=0;behavior=mode; return freshRun(c,j,bytes,{HelperClientClass:Fake,sleep:async()=>{}}); }
    assert.deepEqual((await run('')).report,r); assert.equal(calls,2); assert.equal(events.filter(e=>e[0]==='studio'&&e[1]==='start').length,3); assert.equal(events.filter(e=>e[0]==='worker'&&e[1]==='start').length,3);
    const proof=events.filter(e=>e[2]==='/peers/proof'); assert.equal(proof.length,2);
    const stage=events.find(e=>e[2]==='/artifacts/stage')[3]; assert.equal(stage.source_root,join(c.worker.data_root,'artifacts',hash(bytes.component))); assert.equal(stage.expected_digest,null); assert.equal(stage.expected_manifest_digest,null);
    const grant=events.find(e=>e[2]==='/watch/supporting-grant')[3]; assert.equal(grant.grant.canonical_root,join(c.worker.data_root,'snapshots',hash(bytes.input)));
    assert.deepEqual(events.slice(-2).map(e=>e[1]),['helper-stop','helper-stop']);
    await run('retry'); assert.equal(calls,3);
    for (const mode of ['signature','endpoint','stage','grant','replay','count','denied','bounded']) { await assert.rejects(run(mode)); assert.deepEqual(events.slice(-2).map(e=>e[1]),['helper-stop','helper-stop']); if(mode==='denied') assert.equal(calls,1); if(mode==='bounded') assert.equal(calls,240); }
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('HelperClient uses line protocol and waits for clean exit',async () => {
  const source=`const rl=require('readline').createInterface({input:process.stdin});rl.on('line',s=>{const v=JSON.parse(s);console.log(JSON.stringify({ok:true,value:v.op==='stop'?{stopped:true,escalated:false,recovery_required:false}:v}));if(v.op==='stop')rl.close()})`;
  const h=new HelperClient(process.execPath,['-e',source],{timeout:2000}); h.start(); assert.deepEqual(await h.request({op:'echo'}),{op:'echo'}); await h.stop();
});
