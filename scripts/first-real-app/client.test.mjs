import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, chmodSync, rmSync, readFileSync, symlinkSync, linkSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { identity, loadConfig, validateKnownHosts, sshArgs, saveRequest, saveResult, loadResult, validateFirst, validateReplay, validateWorkerSummary, validateStudioSummary, makeResult, getOrRun, revocationIdentity, makeRevocationReceipt, makeRevocationIntent, makeRevocationBaseline, saveRevocationBaseline, saveRevocationReceipt, loadRevocationReceipt, loadRevocationIntent, loadRevocationBaseline, getOrRunRevocation, validateRevocationReply, validateExecutorAttestation, HelperClient, freshRun, revocationRun, verifyRemoteArtifacts, validateLumeInspection, validateSmokeRuns, revocationEnabled, hash } from './client.mjs';
const dir = () => mkdtempSync(join(homedir(), '.mct-client-test-'));
const job = identity('a'.repeat(64),'b'.repeat(64),'worker');
const report = {schema:'mct-repo-report/v1',sha256:'a'.repeat(64),package_count:2,registry_count:1,git_count:0,local_count:1,duplicates:{foo:['1','2']},selected:{iroh:[],rusqlite:[],wasmtime:['1'],'wasmtime-wasi':[]}};
const attestation={vm_name:'nic73-worker',machine_identifier_sha256:'c'.repeat(64),hardware_uuid_sha256:'d'.repeat(64),nat_ip:'10.1.2.4'};
const ed25519Key='AAAAC3NzaC1lZDI1NTE5AAAAIAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEB';
const knownHostsLine=(host=attestation.nat_ip,key=ed25519Key)=>`${host} ssh-ed25519 ${key}\n`;
function fixtures(r = report) {
  const decoded_result={results:[{ok:JSON.stringify(r)}]}; const first={outcome:'completed',route_taken:{runtime_kind:'remote_peer',node_id:'worker'},decoded_result,inline_result_payload_base64:Buffer.from(JSON.stringify(decoded_result)).toString('base64')};
  const replay={...first,protocol_reason:'idempotency_replay_completed'};
  const summary={call_id:job.call_id,runtime_started:1,runtime_completed:1,runtime_failed:0,toy_allowed:1,toy_denied:0,execution_call_id:'receiver-call'};
  const studioSummary={call_id:job.call_id,runtime_started:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0};
  const preSummary={...studioSummary}; return {first,replay,summary,studioSummary,preSummary};
}
function resultFixture() {
  const {first,replay,summary,studioSummary,preSummary}=fixtures();
  return makeResult(job,first,replay,summary,studioSummary,preSummary,'artifact',{studio:'studio-endpoint',worker:'worker-endpoint'},{studio_binary_sha256:'1'.repeat(64),studio_helper_sha256:'2'.repeat(64),worker_binary_sha256:'3'.repeat(64),worker_helper_sha256:'4'.repeat(64)},attestation);
}
test('identity remains exactly the merged v1 workload identity',() => {
  assert.notEqual(job.job_id,identity('c'.repeat(64),'b'.repeat(64),'worker').job_id); assert.notEqual(job.job_id,identity('a'.repeat(64),'c'.repeat(64),'worker').job_id); assert.notEqual(job.job_id,identity('a'.repeat(64),'b'.repeat(64),'other').job_id);
  const lume={vm_name:'vm',machine_identifier_sha256:'c'.repeat(64),hardware_uuid_sha256:'d'.repeat(64),nat_ip:'10.0.0.2'}; assert.equal(identity('a'.repeat(64),'b'.repeat(64),'worker',lume).job_id,job.job_id); assert.equal(job.job_identity.schema,'mct-first-real-app/job-v1');
  assert.equal(job.call_id,job.job_id); assert.deepEqual(JSON.parse(job.payload_json),[JSON.stringify({sha256:'a'.repeat(64)})]);
});
test('config validates paths, permissions, schema and ssh options',() => {
  const root=dir(); try {
    for (const name of ['input','mct-repo-report.wasm','child.toml','key']) writeFileSync(join(root,name),'x',{mode:0o600});
    writeFileSync(join(root,'known_hosts'),`# independently verified\n${knownHostsLine()}`,{mode:0o600});
    const side=(node,port) => ({node,root:`${root}/${node}`,data_root:`${root}/${node}-data`,binary:`${root}/bin`,helper:`${root}/helper`,bind:`10.1.2.3:${port}`});
    const c={version:1,vision:'vision',input:join(root,'input'),component:join(root,'mct-repo-report.wasm'),manifest:join(root,'child.toml'),jobs_root:join(root,'jobs'),studio:side('studio',1000),worker:{...side('worker',1001),binary_sha256:'a'.repeat(64),ssh:{host:'host.local',user:'user',identity:join(root,'key')}}};
    const v2={...c,version:2,worker:{...c.worker,bind:'10.1.2.4:1001',helper_sha256:'b'.repeat(64),ssh:{...c.worker.ssh,host:'10.1.2.4',known_hosts:join(root,'known_hosts')},lume:attestation}};
    const p=join(root,'config'); const put = v => writeFileSync(p,JSON.stringify(v),{mode:0o600}); put(c); assert.deepEqual(loadConfig(p),c); put(v2); assert.deepEqual(loadConfig(p),v2);
    assert.deepEqual(sshArgs(c),['-o','BatchMode=yes','-o','IdentitiesOnly=yes','-o','StrictHostKeyChecking=yes','-o','ControlMaster=no','-i',c.worker.ssh.identity,'user@host.local']);
    assert.deepEqual(sshArgs(v2),['-F','/dev/null','-p','22','-o','BatchMode=yes','-o','IdentitiesOnly=yes','-o','StrictHostKeyChecking=yes','-o',`UserKnownHostsFile=${v2.worker.ssh.known_hosts}`,'-o','GlobalKnownHostsFile=/dev/null','-o','ControlMaster=no','-o','ClearAllForwardings=yes','-i',v2.worker.ssh.identity,'user@10.1.2.4']);
    put({...c,extra:1}); assert.throws(()=>loadConfig(p)); put({...c,worker:{...c.worker,extra:1}}); assert.throws(()=>loadConfig(p)); put({...c,input:'/tmp/x'}); assert.throws(()=>loadConfig(p)); put({...c,studio:{...c.studio,bind:'8.8.8.8:80'}}); assert.throws(()=>loadConfig(p));
    put({...v2,worker:{...v2.worker,helper_sha256:undefined}}); assert.throws(()=>loadConfig(p)); put({...v2,worker:{...v2.worker,lume:[v2.worker.lume]}}); assert.throws(()=>loadConfig(p)); put({...v2,worker:{...v2.worker,bind:'10.1.2.5:1001'}}); assert.throws(()=>loadConfig(p)); put({...v2,worker:{...v2.worker,ssh:{...v2.worker.ssh,host:'10.1.2.5'}}}); assert.throws(()=>loadConfig(p));put({...v2,worker:{...v2.worker,ssh:{host:'10.1.2.4',user:'user',identity:join(root,'key')}}});assert.throws(()=>loadConfig(p));chmodSync(join(root,'known_hosts'),0o644);put(v2);assert.throws(()=>loadConfig(p));chmodSync(join(root,'known_hosts'),0o600);linkSync(join(root,'known_hosts'),join(root,'known_hosts.link'));assert.throws(()=>loadConfig(p));unlinkSync(join(root,'known_hosts.link'));
    symlinkSync(c.input,join(root,'link')); put({...c,input:join(root,'link')}); assert.throws(()=>loadConfig(p)); put(c); chmodSync(p,0o644); assert.throws(()=>loadConfig(p)); chmodSync(p,0o600); chmodSync(c.worker.ssh.identity,0o644); assert.throws(()=>loadConfig(p));
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('config-v2 known_hosts is one exact unmarked ed25519 NAT-IP pin',() => {
  assert.deepEqual(validateKnownHosts(Buffer.from(`# comment\n\n${knownHostsLine()}`),attestation.nat_ip),{host:attestation.nat_ip,key_type:'ssh-ed25519',key:ed25519Key});
  const invalid=[
    '', '# comments only\n', `${knownHostsLine()}${knownHostsLine()}`, knownHostsLine('*'), knownHostsLine('10.1.2.5'), knownHostsLine('[10.1.2.4]:22'),
    `|1|hash|hash ssh-ed25519 ${ed25519Key}\n`, `@cert-authority ${attestation.nat_ip} ssh-ed25519 ${ed25519Key}\n`, `${attestation.nat_ip},10.1.2.5 ssh-ed25519 ${ed25519Key}\n`,
    `${attestation.nat_ip} ssh-rsa ${ed25519Key}\n`, `${attestation.nat_ip} ssh-ed25519 ${ed25519Key}=\n`, `${attestation.nat_ip} ssh-ed25519 eA==\n`, `${knownHostsLine().trim()} trailing-comment\n`
  ];
  for(const value of invalid)assert.throws(()=>validateKnownHosts(Buffer.from(value),attestation.nat_ip),value);
});
test('config-v2 SSH ignores hostile Host defaults and fixes port 22',() => {
  const root=dir(); try {
    const hostile=join(root,'ssh_config'); writeFileSync(hostile,'Host *\n  HostName 203.0.113.9\n  Port 2200\n  ProxyCommand /usr/bin/false\n  LocalCommand /usr/bin/false\n  PermitLocalCommand yes\n');
    const hostileEffective=execFileSync('/usr/bin/ssh',['-G','-T','-F',hostile,'user@10.1.2.4'],{encoding:'utf8'}); assert.match(hostileEffective,/^hostname 203\.0\.113\.9$/m); assert.match(hostileEffective,/^port 2200$/m); assert.match(hostileEffective,/^proxycommand \/usr\/bin\/false$/m);
    writeFileSync(join(root,'key'),'key'); const c={version:2,worker:{ssh:{host:'10.1.2.4',user:'user',identity:join(root,'key'),known_hosts:join(root,'known_hosts')}}}; const args=sshArgs(c),destination=args.pop();
    const effective=execFileSync('/usr/bin/ssh',['-G','-T',...args,destination],{encoding:'utf8'}); assert.match(effective,/^hostname 10\.1\.2\.4$/m); assert.match(effective,/^port 22$/m); assert.match(effective,/^permitlocalcommand no$/m); assert.doesNotMatch(effective,/^proxycommand /m); assert.doesNotMatch(effective,/^localcommand /m);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('Lume attestation requires NAT and v1 never enables revocation',() => {
  const machine='machine-id', expected={vm_name:'vm',nat_ip:'10.0.0.2',machine_identifier_sha256:hash(machine)};
  assert.equal(validateLumeInspection({name:'vm',ip:'10.0.0.2',networkMode:'nat',machineIdentifier:machine},expected).name,'vm');
  assert.throws(()=>validateLumeInspection({name:'vm',ip:'10.0.0.2',networkMode:'bridged',machineIdentifier:machine},expected),/must be nat/);
  assert.equal(revocationEnabled({version:1}),false); assert.equal(revocationEnabled({version:2}),true);assert.deepEqual(validateExecutorAttestation({version:2,worker:{lume:attestation}},attestation),attestation);assert.throws(()=>validateExecutorAttestation({version:2,worker:{lume:attestation}},{...attestation,nat_ip:'10.1.2.5'}));
});
test('remote daemon and helper metadata are trusted before independent hashing',async () => {
  const c={version:2,worker:{binary:'/private/var/mct/bin/mct-daemon',helper:'/private/var/mct/bin/first_real_app_host',binary_sha256:'a'.repeat(64),helper_sha256:'b'.repeat(64),ssh:{host:'10.0.0.2',user:'worker',identity:'/private/var/mct/key',known_hosts:'/private/var/mct/known_hosts'}}}; const invocations=[];
  const outputs=[`501\nRegular File|501|1|700\nRegular File|501|1|700\n`,`${'a'.repeat(64)}  ${c.worker.binary}\n${'b'.repeat(64)}  ${c.worker.helper}\n`];
  const digests=await verifyRemoteArtifacts(c,{runImpl:async(command,args)=>{invocations.push({command,args});return {stdout:outputs.shift(),stderr:''};}});
  assert.deepEqual(digests,{binary_sha256:'a'.repeat(64),helper_sha256:'b'.repeat(64)});assert.equal(invocations.length,2);for(const invocation of invocations){assert.equal(invocation.command,'/usr/bin/ssh');assert.deepEqual(invocation.args.slice(0,4),['-F','/dev/null','-p','22']);assert.ok(invocation.args.includes('ClearAllForwardings=yes'));assert.ok(invocation.args.includes(`UserKnownHostsFile=${c.worker.ssh.known_hosts}`));}assert.match(invocations[0].args.at(-1),/stat -f/);assert.match(invocations[1].args.at(-1),/shasum -a 256/);
  for(const bad of ['Symbolic Link|501|1|700','Regular File|502|1|700','Regular File|501|2|700','Regular File|501|1|600']){let calls=0;await assert.rejects(verifyRemoteArtifacts(c,{runImpl:async()=>{calls++;return {stdout:`501\n${bad}\nRegular File|501|1|700\n`,stderr:''};}}));assert.equal(calls,1,'hash command must not run after unsafe metadata');}
  let calls=0;await assert.rejects(verifyRemoteArtifacts(c,{runImpl:async()=>({stdout:++calls===1?'501\nRegular File|501|1|700\nRegular File|501|1|700\n':`${'a'.repeat(64)}  ${c.worker.binary}\n${'c'.repeat(64)}  ${c.worker.helper}\n`,stderr:''})}),/helper digest mismatch/);
});
test('reply, replay and summary validation fail closed',() => {
  const {first,replay,summary,studioSummary}=fixtures(); assert.deepEqual(validateFirst(first,job),report); validateReplay(replay,first,job); validateWorkerSummary(summary,job); validateWorkerSummary({...summary,runtime_completed:2},job);const recomputed={...summary,runtime_started:2,runtime_completed:2,toy_allowed:2,execution_call_ids:['old','receiver-call']};delete recomputed.execution_call_id;validateWorkerSummary(recomputed,job,{...summary,execution_call_id:'old'},'recovered_recomputed'); validateStudioSummary(studioSummary,job);
  for (const bad of [{...first,outcome:'failed'},{...first,protocol_reason:'idempotency_replay_completed'},{...first,route_taken:{runtime_kind:'remote_peer',node_id:'other'}},{...first,decoded_result:{results:[{err:'x'}]}},{...first,inline_result_payload_base64:'!'}]) assert.throws(()=>validateFirst(bad,job));
  for (const r of [{...report,sha256:'c'.repeat(64)},{...report,schema:'wrong'},{...report,package_count:4},{...report,extra:1},{...report,duplicates:{foo:['2','1']}},{...report,selected:{...report.selected,iroh:['1','1']}}]) assert.throws(()=>validateFirst(fixtures(r).first,job));
  assert.throws(()=>validateReplay({...replay,protocol_reason:'other'},first,job)); assert.throws(()=>validateReplay({...replay,inline_result_payload_base64:'e30='},first,job)); assert.throws(()=>validateWorkerSummary({...summary,runtime_started:2},job)); assert.throws(()=>validateWorkerSummary({...summary,runtime_completed:0},job)); assert.throws(()=>validateWorkerSummary({...summary,runtime_failed:1},job)); assert.throws(()=>validateWorkerSummary({...summary,toy_allowed:0},job)); assert.throws(()=>validateWorkerSummary({...summary,toy_denied:1},job)); assert.throws(()=>validateWorkerSummary({...summary,execution_call_id:''},job)); assert.throws(()=>validateStudioSummary({...studioSummary,runtime_started:1},job));
});
test('durable publish, corruption and fresh cache avoid transport',async () => {
  const root=dir(); try {
    const result=resultFixture();
    saveRequest(root,job); saveRequest(root,job); saveResult(root,job,result); saveResult(root,job,result); assert.deepEqual(loadResult(root,job),result);
    assert.deepEqual(await getOrRun(root,job,()=>{throw new Error('transport spawned');}),{result,source:'durable_cache'});
    const p=join(root,`${job.job_id}.result.json`); const obj=JSON.parse(readFileSync(p)); obj.report.package_count=99; writeFileSync(p,JSON.stringify(obj)); assert.throws(()=>loadResult(root,job)); assert.throws(()=>saveResult(root,job,result));
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('previously merged version-1 result records remain loadable and immutable during upgrade',async () => {
  const root=dir(); try { const {first,replay,summary,studioSummary}=fixtures(); const legacy={job,report,report_sha256:hash(JSON.stringify(report)),first_inline_base64:first.inline_result_payload_base64,route:first.route_taken,protocol_replay_reason:replay.protocol_reason,worker_summary:{call_id:job.call_id,runtime_started:1,runtime_completed:2,runtime_failed:0},artifact_id:'legacy-artifact',created_at:new Date().toISOString()}; saveRequest(root,job); const path=join(root,`${job.job_id}.result.json`);writeFileSync(path,JSON.stringify(legacy)+'\n',{mode:0o600});const original=readFileSync(path);assert.deepEqual(loadResult(root,job),legacy);
    const upgraded=makeResult(job,replay,replay,summary,studioSummary,summary,'artifact',{studio:'studio-endpoint',worker:'worker-endpoint'},{studio_binary_sha256:'1'.repeat(64),studio_helper_sha256:'2'.repeat(64),worker_binary_sha256:'3'.repeat(64),worker_helper_sha256:'4'.repeat(64)},attestation,'recovered_replay');const outcome=await getOrRun(root,job,async(_job,state)=>{assert.equal(state.recovery,true);return upgraded;},{requireCurrent:true});assert.deepEqual(outcome,{result:upgraded,source:'executed'});assert.deepEqual(readFileSync(path),original);assert.deepEqual(loadResult(root,job),upgraded);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('fresh result is published and identifies execution source',async () => {
  const root=dir(); try {
    const result=resultFixture();
    assert.deepEqual(await getOrRun(root,job,async () => result),{result,source:'executed'});
    assert.deepEqual(loadResult(root,job),result);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('durable revocation receipt is separate and loaded before transport',async () => {
  const root=dir(); try {
    const result=resultFixture(); const revocation=revocationIdentity(job,'studio','worker');const intent=makeRevocationIntent(job,result,revocation);const baseline=makeRevocationBaseline(job,result,intent,result.endpoints.worker,{ledger_sequence:7,ledger_head_hash:'e'.repeat(64)});
    const reply={outcome:'denied',protocol_reason:'binding_revoked',route_taken:{runtime_kind:'remote_peer',node_id:'worker'},submitted_call_id:revocation.probe_call_id};
    const zero=id=>({call_id:id,runtime_started:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0});
    const ledger={call_id:revocation.probe_call_id,binding_id:revocation.binding_id,peer_node_id:'studio',baseline_sequence:7,baseline_head_hash:'e'.repeat(64),revocation_count:1,rejection_count:1,revocation_sequence:10,rejection_sequence:11,hello_reason:'BindingRevoked',runtime_started:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0};
    validateRevocationReply(reply,job,revocation); const receipt=makeRevocationReceipt(job,result,revocation,reply,zero(revocation.probe_call_id),zero(revocation.probe_call_id),ledger,baseline);assert.deepEqual(receipt.executor_attestation,attestation);
    const reasonless={...reply,protocol_reason:null,route_taken:undefined};validateRevocationReply(reasonless,job,revocation);assert.equal(makeRevocationReceipt(job,result,revocation,reasonless,zero(revocation.probe_call_id),zero(revocation.probe_call_id),ledger,baseline).protocol_reason,null);assert.throws(()=>validateRevocationReply({...reply,protocol_reason:'no_route'},job,revocation));
    saveResult(root,job,result); saveRevocationReceipt(root,job,receipt,revocation); assert.deepEqual(loadRevocationReceipt(root,job,result,revocation),receipt);
    const secondJob=await getOrRun(root,job,()=>{throw new Error('job helper or SSH spawned');}); assert.equal(secondJob.source,'durable_cache');
    assert.deepEqual(await getOrRunRevocation(root,job,secondJob.result,revocation,()=>{throw new Error('revocation helper or SSH spawned');}),{receipt,source:'durable_cache'});
    const p=join(root,`${job.job_id}.revocation.json`); const damaged={...receipt,probe_call_id:job.call_id}; writeFileSync(p,JSON.stringify(damaged)); assert.throws(()=>loadRevocationReceipt(root,job,result,revocation));
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('changed Studio node rejects a cached revocation receipt before transport',async () => {
  const root=dir(); try {
    const result=resultFixture(),oldRevocation=revocationIdentity(job,'studio-old','worker'),currentRevocation=revocationIdentity(job,'studio-current','worker');
    const intent=makeRevocationIntent(job,result,oldRevocation),baseline=makeRevocationBaseline(job,result,intent,result.endpoints.worker,{ledger_sequence:7,ledger_head_hash:'e'.repeat(64)});
    const zero=id=>({call_id:id,runtime_started:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0});
    const reply={outcome:'denied',protocol_reason:'binding_revoked',route_taken:{runtime_kind:'remote_peer',node_id:'worker'},submitted_call_id:oldRevocation.probe_call_id};
    const ledger={call_id:oldRevocation.probe_call_id,binding_id:oldRevocation.binding_id,peer_node_id:oldRevocation.studio_node,baseline_sequence:7,baseline_head_hash:'e'.repeat(64),revocation_count:1,rejection_count:1,revocation_sequence:10,rejection_sequence:11,hello_reason:'BindingRevoked',runtime_started:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0};
    const receipt=makeRevocationReceipt(job,result,oldRevocation,reply,zero(oldRevocation.probe_call_id),zero(oldRevocation.probe_call_id),ledger,baseline);saveResult(root,job,result);saveRevocationReceipt(root,job,receipt,oldRevocation);
    let transported=false;await assert.rejects(getOrRunRevocation(root,job,result,currentRevocation,()=>{transported=true;}),/revocation studio_node mismatch/);assert.equal(transported,false);assert.throws(()=>loadRevocationReceipt(root,job,result,currentRevocation),/revocation studio_node mismatch/);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('changed Studio node rejects interrupted intent and baseline before transport',async () => {
  const root=dir(); try {
    const result=resultFixture(),oldRevocation=revocationIdentity(job,'studio-old','worker'),currentRevocation=revocationIdentity(job,'studio-current','worker');saveResult(root,job,result);
    await assert.rejects(getOrRunRevocation(root,job,result,oldRevocation,()=>{throw new Error('interrupted after intent');}),/interrupted after intent/);
    const oldIntent=loadRevocationIntent(root,job,result,oldRevocation);assert.throws(()=>loadRevocationIntent(root,job,result,currentRevocation),/revocation studio_node mismatch/);
    const oldBaseline=makeRevocationBaseline(job,result,oldIntent,result.endpoints.worker,{ledger_sequence:7,ledger_head_hash:'e'.repeat(64)});saveRevocationBaseline(root,job,result,oldIntent,oldBaseline,oldRevocation);
    const currentIntent=makeRevocationIntent(job,result,currentRevocation);assert.throws(()=>loadRevocationBaseline(root,job,result,currentIntent,currentRevocation),/revocation studio_node mismatch/);
    let transported=false;await assert.rejects(getOrRunRevocation(root,job,result,currentRevocation,()=>{transported=true;}),/revocation studio_node mismatch/);assert.equal(transported,false);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('revocation intent and post-endpoint ledger baseline are separately durable before mutation',async () => {
  const root=dir(); try { const result=resultFixture(),revocation=revocationIdentity(job,'studio','worker'); saveResult(root,job,result);
    await assert.rejects(getOrRunRevocation(root,job,result,revocation,(_job,_result,{intent,baseline,recovery,checkpointBaseline})=>{assert.equal(recovery,false);assert.equal(baseline,null);assert.deepEqual(loadRevocationIntent(root,job,result,revocation),intent);const saved=checkpointBaseline(makeRevocationBaseline(job,result,intent,result.endpoints.worker,{ledger_sequence:7,ledger_head_hash:'e'.repeat(64)}));assert.deepEqual(loadRevocationBaseline(root,job,result,intent,revocation),saved);throw new Error('interrupted after baseline before mutation');}),/interrupted/);
    const intent=loadRevocationIntent(root,job,result,revocation),baseline=loadRevocationBaseline(root,job,result,intent,revocation);assert.deepEqual(intent.executor_attestation,attestation);assert.deepEqual(baseline.intent,intent);assert.equal(baseline.worker_endpoint,result.endpoints.worker);assert.equal(baseline.ledger_sequence,7);assert.equal(baseline.ledger_head_hash,'e'.repeat(64));
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('request is durable before transport and failure leaves it in place',async () => {
  const root=dir(); try {
    await assert.rejects(getOrRun(root,job,() => { assert.deepEqual(JSON.parse(readFileSync(join(root,`${job.job_id}.request.json`))),job); throw new Error('offline'); }),/offline/);
    assert.deepEqual(JSON.parse(readFileSync(join(root,`${job.job_id}.request.json`))),job);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('request without result enters explicit recovery and checkpoints the reacquired reply',async () => {
  const root=dir(); try { const {replay,summary,studioSummary}=fixtures(); saveRequest(root,job); let stateSeen;
    const recovered=makeResult(job,replay,replay,summary,studioSummary,summary,'artifact',{studio:'studio-endpoint',worker:'worker-endpoint'},{studio_binary_sha256:'1'.repeat(64),studio_helper_sha256:'2'.repeat(64),worker_binary_sha256:'3'.repeat(64),worker_helper_sha256:'4'.repeat(64)},attestation,'recovered_replay');
    const outcome=await getOrRun(root,job,async(submitted,state)=>{assert.equal(submitted.call_id,job.call_id);assert.equal(state.recovery,true);assert.equal(state.checkpoint,null);stateSeen=state.checkpointReply(replay);return recovered;});
    assert.equal(stateSeen.call_id,job.call_id);assert.equal(stateSeen.reply.protocol_reason,'idempotency_replay_completed');assert.deepEqual(outcome,{result:recovered,source:'executed'});
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('freshRun exchanges signed records and validates the execution; failures clean up',async () => {
  const root=dir(); try {
    const bytes={input:Buffer.from('input'),component:Buffer.from('wasm'),manifest:Buffer.from('manifest')};
    const j=identity(hash(bytes.input),hash(bytes.component),'worker');
    const r={...report,sha256:hash(bytes.input)}; const decoded_result={results:[{ok:JSON.stringify(r)}]};
    const first={outcome:'completed',route_taken:{runtime_kind:'remote_peer',node_id:'worker'},decoded_result,inline_result_payload_base64:Buffer.from(JSON.stringify(decoded_result)).toString('base64')};
    const binary=join(root,'bin'),helper=join(root,'helper'); writeFileSync(binary,'daemon'); writeFileSync(helper,'helper');
    const side=node=>({node,root:`${root}/${node}`,data_root:`${root}/${node}-data`,binary,helper,bind:'10.0.0.1:1234'});
    const c={version:2,vision:'vision',studio:side('studio'),worker:{...side('worker'),binary_sha256:hash('daemon'),helper_sha256:hash('helper'),ssh:{user:'user',host:attestation.nat_ip,identity:'/key',known_hosts:'/known_hosts'},lume:attestation}};
    let events=[], peers={}, behavior='', calls=0;
    const authority={mother_node_id:'worker',authority_epoch:'epoch',generation:2,source_authority_observation_id:'observation'};
    class Fake {
      constructor(cmd) { this.node=cmd==='/usr/bin/ssh'?'worker':'studio'; }
      start() { events.push([this.node,'helper-start']); }
      async stop() { events.push([this.node,'helper-stop']); }
      async request(q) {
        events.push([this.node,q.op,q.path,q.body]); const side=c[this.node];
        if(q.op==='open') return {helper_sha256:hash('helper')};
        if(q.op==='start') { this.starts=(this.starts??0)+1; const endpoint_id=this.node+(behavior==='endpoint' && this.starts>1?'changed':''); return {endpoint_id,ticket:{endpoint_id},binary_sha256:hash('daemon'),helper_sha256:hash('helper')}; }
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
        if(q.op==='call') { calls++; if (behavior==='denied') return {outcome:'denied',protocol_reason:'other'}; if (behavior==='retry' && calls===1) return {outcome:'denied',protocol_reason:'no_route'}; if (behavior==='bounded') return {outcome:'denied',protocol_reason:'no_route'}; if(behavior==='recover')return {...first,protocol_reason:'idempotency_replay_completed'}; return calls===1 || (behavior==='retry' && calls===2) || (behavior==='recover-recomputed'&&calls===1) ? first : {...first,protocol_reason:behavior==='replay'?'wrong':'idempotency_replay_completed'}; }
        if(q.op==='ledger_call_summary') {
          if(calls===0) { if(behavior==='recover')return {call_id:j.call_id,runtime_started:1,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0,execution_call_id:'receiver-call',execution_call_ids:['receiver-call']};if(behavior==='recover-recomputed')return {call_id:j.call_id,runtime_started:1,runtime_completed:1,runtime_failed:0,toy_allowed:1,toy_denied:0,execution_call_id:'old-call',execution_call_ids:['old-call']}; return {call_id:j.call_id,runtime_started:behavior==='stale'?1:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0}; }
          if(this.node==='studio') return {call_id:j.call_id,runtime_started:behavior==='studio-exec'?1:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0};
          if(behavior==='recover-recomputed')return {call_id:j.call_id,runtime_started:2,runtime_completed:2,runtime_failed:0,toy_allowed:2,toy_denied:0,execution_call_ids:['new-call','old-call']};
          return {call_id:j.call_id,runtime_started:behavior==='count'?2:1,runtime_completed:behavior==='multi'||behavior==='recover'?2:1,runtime_failed:0,toy_allowed:behavior==='toy'?0:1,toy_denied:behavior==='toy-deny'?1:0,execution_call_id:'receiver-call',execution_call_ids:['receiver-call']};
        }
        throw Error(`unexpected ${q.op}`);
      }
    }
    async function run(mode,options={}) { events=[];peers={};calls=0;behavior=mode; return freshRun(c,j,bytes,{HelperClientClass:Fake,sleep:async()=>{},remoteHasher:async()=>({binary_sha256:hash('daemon'),helper_sha256:hash('helper')}),...options}); }
    assert.deepEqual((await run('')).report,r); assert.equal(calls,2); assert.equal(events.filter(e=>e[0]==='studio'&&e[1]==='start').length,4); assert.equal(events.filter(e=>e[0]==='worker'&&e[1]==='start').length,4);
    events=[];peers={};calls=0;behavior='';await freshRun(c,j,bytes,{HelperClientClass:Fake,sleep:async()=>{},remoteHasher:async()=>{events.push(['remote-hash']);return {binary_sha256:hash('daemon'),helper_sha256:hash('helper')};}});assert.deepEqual(events[0],['remote-hash']);assert.ok(events.findIndex(e=>e[1]==='helper-start')>0);
    const proof=events.filter(e=>e[2]==='/peers/proof'); assert.equal(proof.length,2);
    const stage=events.find(e=>e[2]==='/artifacts/stage')[3]; assert.equal(stage.source_root,join(c.worker.data_root,'artifacts',hash(bytes.component))); assert.equal(stage.expected_digest,null); assert.equal(stage.expected_manifest_digest,null);
    const grant=events.find(e=>e[2]==='/watch/supporting-grant')[3]; assert.equal(grant.grant.canonical_root,join(c.worker.data_root,'snapshots',hash(bytes.input)));
    assert.deepEqual(events.slice(-2).map(e=>e[1]),['helper-stop','helper-stop']);
    await run('retry'); assert.equal(calls,3);assert.equal((await run('multi')).worker_summary.runtime_completed,2);
    let checkpointed;const recovered=await run('recover',{recovery:true,checkpointReply:reply=>(checkpointed={reply})});assert.equal(recovered.execution_mode,'recovered_replay');assert.equal(calls,1);assert.equal(checkpointed.reply.protocol_reason,'idempotency_replay_completed');const recomputed=await run('recover-recomputed',{recovery:true});assert.equal(recomputed.execution_mode,'recovered_recomputed');assert.equal(recomputed.pre_execution_summary.runtime_started,1);assert.equal(recomputed.worker_summary.runtime_started,2);assert.equal(calls,2);
    for (const mode of ['signature','endpoint','stage','grant','replay','count','toy','toy-deny','studio-exec','stale','denied','bounded']) { await assert.rejects(run(mode)); assert.deepEqual(events.slice(-2).map(e=>e[1]),['helper-stop','helper-stop']); if(mode==='denied') assert.equal(calls,1); if(mode==='bounded') assert.equal(calls,240); if(mode==='stale') assert.equal(calls,1); }
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('revocationRun publishes a scoped baseline and recovers append-before-config revokes',async () => {
  const root=dir(); try {
    const binary=join(root,'bin'),helper=join(root,'helper'); writeFileSync(binary,'daemon'); writeFileSync(helper,'helper');
    const side=node=>({node,root:`${root}/${node}`,data_root:`${root}/${node}-data`,binary,helper,bind:'10.0.0.1:1234'});
    const c={version:2,vision:'vision',studio:side('studio'),worker:{...side('worker'),binary_sha256:hash('daemon'),helper_sha256:hash('helper'),ssh:{user:'user',host:attestation.nat_ip,identity:'/key',known_hosts:'/known_hosts'},lume:attestation}};
    const {first,replay,summary,studioSummary,preSummary}=fixtures(); const result=makeResult(job,first,replay,summary,studioSummary,preSummary,'artifact',{studio:'studio-endpoint',worker:'worker-endpoint'},{studio_binary_sha256:hash('daemon'),studio_helper_sha256:hash('helper'),worker_binary_sha256:hash('daemon'),worker_helper_sha256:hash('helper')},attestation);
    const revocation=revocationIdentity(job,'studio','worker'); let events=[],bindingState='admitted',revocationCount=1,rejections=1;
    class Fake {
      constructor(cmd){this.node=cmd==='/usr/bin/ssh'?'worker':'studio';}
      start(){events.push([this.node,'helper-start']);}
      async stop(){events.push([this.node,'helper-stop']);}
      async request(q){ events.push([this.node,q.op,q.path,q.body]);
        if(q.op==='open') return {helper_sha256:hash('helper')};
        if(q.op==='start') { const endpoint_id=result.endpoints[this.node]; return {endpoint_id,ticket:{endpoint_id},binary_sha256:hash('daemon'),helper_sha256:hash('helper')}; }
        if(q.op==='stop') return {stopped:true,escalated:false,recovery_required:false};
        if(q.op==='ledger_head'){assert.equal(q.baseline_sequence,q.baseline_head_hash?7:0);return {ledger_sequence:q.baseline_sequence||7,ledger_head_hash:q.baseline_head_hash??'e'.repeat(64)};}
        if(q.op==='peers') return {studio:{binding_id:revocation.binding_id,binding_state:bindingState}};
        if(q.op==='control') { assert.equal(this.node,'worker'); assert.equal(q.path,'/peers/revoke'); assert.deepEqual(q.body,{expected_config_path:join(c.worker.root,'config.json'),peer_node_id:'studio'}); bindingState='revoked'; return {}; }
        if(q.op==='call') return {outcome:'denied',protocol_reason:'binding_revoked',route_taken:{runtime_kind:'remote_peer',node_id:'worker'},submitted_call_id:revocation.probe_call_id};
        if(q.op==='ledger_call_summary') return {call_id:revocation.probe_call_id,runtime_started:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0};
        if(q.op==='ledger_revocation_summary') {assert.equal(q.after_sequence,7);assert.equal(q.baseline_head_hash,'e'.repeat(64));return {call_id:revocation.probe_call_id,binding_id:revocation.binding_id,peer_node_id:'studio',baseline_sequence:7,baseline_head_hash:'e'.repeat(64),revocation_count:revocationCount,rejection_count:rejections,revocation_sequence:revocationCount===2?20:19,rejection_sequence:21,hello_reason:'BindingRevoked',runtime_started:0,runtime_completed:0,runtime_failed:0,toy_allowed:0,toy_denied:0};}
        throw Error(`unexpected ${q.op}`);
      }
    }
    const remoteHasher=async()=>({binary_sha256:hash('daemon'),helper_sha256:hash('helper')}),intent=makeRevocationIntent(job,result,revocation);let baseline;
    const receipt=await revocationRun(c,job,result,{HelperClientClass:Fake,remoteHasher,intent,checkpointBaseline:value=>(baseline=value)}); assert.equal(receipt.probe_call_id,revocation.probe_call_id); assert.equal(receipt.ledger_summary.revocation_count,1);assert.deepEqual(receipt.executor_attestation,attestation);assert.equal(baseline.ledger_sequence,7);assert.deepEqual(events.slice(-2).map(e=>e[1]),['helper-stop','helper-stop']);assert.ok(events.findIndex(e=>e[1]==='ledger_head')<events.findIndex(e=>e[2]==='/peers/revoke'));
    assert.equal(events.filter(e=>e[2]==='/peers/revoke').length,1); assert.equal(events.filter(e=>e[1]==='call').length,1);
    events=[];bindingState='admitted';revocationCount=2;rejections=1;const recovered=await revocationRun(c,job,result,{HelperClientClass:Fake,remoteHasher,intent,baseline,recovery:true});assert.equal(recovered.ledger_summary.revocation_count,2);assert.equal(recovered.ledger_summary.revocation_sequence,20);assert.equal(events.filter(e=>e[2]==='/peers/revoke').length,1);assert.equal(events.filter(e=>e[1]==='call').length,1);
    let remoteChecks=0;const changedStudio={...c,studio:{...c.studio,node:'studio-current'}};await assert.rejects(revocationRun(changedStudio,job,result,{HelperClientClass:Fake,remoteHasher:async()=>{remoteChecks++;return remoteHasher();},intent,baseline,recovery:true}),/revocation studio_node mismatch/);assert.equal(remoteChecks,0);
  } finally { rmSync(root,{recursive:true,force:true}); }
});
test('smoke validation binds inspected executor evidence and keeps fresh proof count one',() => {
  const base={durable_job_id:'job',execution_mode:'fresh',executor_attestation:attestation,report_sha256:'a'.repeat(64),worker_execution_count:1,report:{ok:true},revocation:{outcome:'denied',revocation_count:1}};
  const cached={...base,source:'durable_cache',durable_sources:{job:'durable_cache',revocation:'durable_cache'}};assert.deepEqual(validateSmokeRuns(cached,cached,attestation),{first_process:cached,second_process:cached});
  assert.throws(()=>validateSmokeRuns({...base,source:'executed',durable_sources:{job:'executed',revocation:'executed'}},{...cached,source:'executed'},attestation));assert.throws(()=>validateSmokeRuns(cached,{...cached,executor_attestation:{...attestation,nat_ip:'10.1.2.5'}},attestation));const recomputed={...cached,execution_mode:'recovered_recomputed',worker_execution_count:2};assert.throws(()=>validateSmokeRuns(recomputed,recomputed,attestation),/2 !== 1/);
});
test('HelperClient uses line protocol and waits for clean exit',async () => {
  const source=`const rl=require('readline').createInterface({input:process.stdin});rl.on('line',s=>{const v=JSON.parse(s);console.log(JSON.stringify({ok:true,value:v.op==='stop'?{stopped:true,escalated:false,recovery_required:false}:v}));if(v.op==='stop')rl.close()})`;
  const h=new HelperClient(process.execPath,['-e',source],{timeout:2000}); h.start(); assert.deepEqual(await h.request({op:'echo'}),{op:'echo'}); await h.stop();
});
