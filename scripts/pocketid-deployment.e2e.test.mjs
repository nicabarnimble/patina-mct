import test from 'node:test';
import assert from 'node:assert/strict';
import {execFile, spawn} from 'node:child_process';
import {createHash, generateKeyPairSync, sign} from 'node:crypto';
import {access, chmod, mkdir, mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {createServer as createHttpsServer, request as httpsRequest} from 'node:https';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {promisify} from 'node:util';

const SOURCE_ROOT = resolve(process.env.MCT_SOURCE_ROOT || join(import.meta.dirname, '..'));
const {run: runGate} = await import(pathToFileURL(join(SOURCE_ROOT, 'scripts/pocketid-approval-gate.mjs')));
const {signEnvelope, verifyEnvelope} = await import(pathToFileURL(join(SOURCE_ROOT, 'scripts/mct-approval-protocol.mjs')));
const exec = promisify(execFile);
const sleep = milliseconds => new Promise(resolvePromise => setTimeout(resolvePromise, milliseconds));
async function eventually(task, timeout = 15000) {
  const deadline = Date.now() + timeout; let last;
  while (Date.now() < deadline) { try { return await task(); } catch (error) { last = error; await sleep(50); } }
  throw last;
}
async function stop(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  child.kill('SIGTERM');
  await eventually(async () => assert(child.exitCode !== null || child.signalCode !== null), 10000).catch(() => child.kill('SIGKILL'));
}
function httpsStatus(port, host, path) {
  return new Promise((resolvePromise, reject) => {
    const request = httpsRequest({hostname: '127.0.0.1', port, path, rejectUnauthorized: false, headers: {Host: host}}, response => {
      response.resume(); response.on('end', () => resolvePromise(response.statusCode));
    });
    request.on('error', reject); request.end();
  });
}
class CDP {
  constructor(socket) {
    this.socket = socket; this.next = 0; this.pending = new Map();
    socket.addEventListener('message', event => {
      const message = JSON.parse(event.data); if (!message.id) return;
      const waiter = this.pending.get(message.id); this.pending.delete(message.id); if (!waiter) return;
      clearTimeout(waiter.timer); message.error ? waiter.reject(new Error(JSON.stringify(message.error))) : waiter.resolve(message.result);
    });
  }
  call(method, params = {}, sessionId) {
    return new Promise((resolvePromise, reject) => {
      const id = ++this.next; const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
      this.pending.set(id, {resolve: resolvePromise, reject, timer}); this.socket.send(JSON.stringify({id, method, params, sessionId}));
    });
  }
  close() { this.socket.close(); }
}
async function certificate(directory) {
  const cert = join(directory, 'cert.pem'); const key = join(directory, 'key.pem');
  await exec('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', cert, '-days', '1',
    '-subj', '/CN=mct-approval.nicabar.org', '-addext', 'subjectAltName=DNS:mct-approval.nicabar.org,DNS:id.nicabar.org,DNS:wiki.nicabar.org']);
  await chmod(key, 0o600);
  const {stdout: publicKey} = await exec('openssl', ['x509', '-in', cert, '-pubkey', '-noout']);
  const converted = spawn('openssl', ['pkey', '-pubin', '-outform', 'DER'], {stdio: ['pipe', 'pipe', 'ignore']});
  const chunks = []; converted.stdout.on('data', data => chunks.push(data)); converted.stdin.end(publicKey);
  await new Promise((resolvePromise, reject) => converted.on('exit', code => code === 0 ? resolvePromise() : reject(new Error('SPKI conversion failed'))));
  return {cert, key, spki: createHash('sha256').update(Buffer.concat(chunks)).digest('base64')};
}
const b64 = value => Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)).toString('base64url');
function idToken(privateKey, claims) {
  const header = b64({alg: 'RS256', kid: 'hosted-test', typ: 'JWT'}); const payload = b64(claims); const input = `${header}.${payload}`;
  return `${input}.${sign('RSA-SHA256', Buffer.from(input), privateKey).toString('base64url')}`;
}
function processOutput(child) {
  let stdout = '', stderr = '';
  child.stdout.on('data', data => { stdout += data; }); child.stderr.on('data', data => { stderr += data; });
  return {get stdout() { return stdout; }, get stderr() { return stderr; }};
}
async function exitCode(child) {
  if (child.exitCode !== null) return child.exitCode;
  return new Promise(resolvePromise => child.once('exit', resolvePromise));
}
async function click(page, selector) {
  const point = await eventually(async () => {
    const result = await page('Runtime.evaluate', {expression: `(() => { const e=document.querySelector(${JSON.stringify(selector)}); if(!e)return null; e.scrollIntoView({block:'center'}); const b=e.getBoundingClientRect(); return {x:b.x+b.width/2,y:b.y+b.height/2}; })()`, returnByValue: true});
    assert(result.result.value); return result.result.value;
  });
  await page('Input.dispatchMouseEvent', {type: 'mousePressed', button: 'left', clickCount: 1, ...point});
  await page('Input.dispatchMouseEvent', {type: 'mouseReleased', button: 'left', clickCount: 1, ...point});
}
async function waitForDocumentReady(page, expectedOrigin) {
  await eventually(async () => {
    const evaluated = await page('Runtime.evaluate', {
      expression: '({readyState:document.readyState,origin:location.origin})', returnByValue: true,
    });
    assert.equal(evaluated.exceptionDetails, undefined, JSON.stringify(evaluated));
    assert.deepEqual(evaluated.result.value, {readyState: 'complete', origin: expectedOrigin});
  });
}
async function scanAndSignIn(page, url) {
  await page('Page.navigate', {url}); await page('Page.bringToFront');
  await eventually(async () => {
    const text = await page('Runtime.evaluate', {expression: 'document.body.innerText', returnByValue: true});
    assert.match(text.result.value, /A bot is asking/); assert.doesNotMatch(text.result.value, /unraid-mother|example-handbook/);
  });
  const href = await page('Runtime.evaluate', {expression: 'document.querySelector(".btn-signin").href', returnByValue: true});
  await page('Page.navigate', {url: href.result.value});
}

test('hosted non-localhost QR flows complete one Mother grant and one NIC-132 proof', {timeout: 180000}, async t => {
  const directory = await mkdtemp(join(tmpdir(), 'mct-qr-hosted-'));
  const session = join(directory, 'session'); const runDirectory = join(directory, 'run'); const mother = join(runDirectory, 'mother');
  await chmod(directory, 0o700);
  await mkdir(session, {mode: 0o700}); await mkdir(runDirectory, {mode: 0o700}); await mkdir(mother, {mode: 0o700});
  const {cert, key, spki} = await certificate(directory);
  let gatePort, queuePort, issuerPort, wikiPort;
  const origin = 'https://mct-approval.nicabar.org'; const issuer = 'https://id.nicabar.org';
  const wikiProofUrl = 'https://wiki.nicabar.org/library/_mct-proof';
  const clientId = 'mct-hosted-test'; const subject = 'operator-hosted-test';
  const secret = join(directory, 'client-secret'); const subjects = join(directory, 'subjects.json');
  const proofPrivateFile = join(directory, 'proof-private.pem'); const proofPublicFile = join(directory, 'proof-public.pem');
  const wikiPrivateFile = join(directory, 'wiki-private.pem'); const wikiPublicFile = join(directory, 'wiki-public.pem');
  await writeFile(secret, 'disposable-test-secret\n', {mode: 0o600}); await writeFile(subjects, JSON.stringify([subject]), {mode: 0o600});
  const approvalProofKeys = generateKeyPairSync('ed25519'); const wikiKeys = generateKeyPairSync('ed25519');
  await writeFile(proofPrivateFile, approvalProofKeys.privateKey.export({format: 'pem', type: 'pkcs8'}), {mode: 0o600});
  await writeFile(proofPublicFile, approvalProofKeys.publicKey.export({format: 'pem', type: 'spki'}), {mode: 0o600});
  await writeFile(wikiPrivateFile, wikiKeys.privateKey.export({format: 'pem', type: 'pkcs8'}), {mode: 0o600});
  await writeFile(wikiPublicFile, wikiKeys.publicKey.export({format: 'pem', type: 'spki'}), {mode: 0o600});
  const oidcKeys = generateKeyPairSync('rsa', {modulusLength: 2048}); const jwk = oidcKeys.publicKey.export({format: 'jwk'});
  Object.assign(jwk, {kid: 'hosted-test', alg: 'RS256', use: 'sig'});
  let callbackNonce = ''; let ticketSeen, ticketTokenSeen; let issuerServer, wikiServer, gate, queue, chrome, grant, proof, cdp;
  const consumedTickets = new Set();
  try {
    issuerServer = createHttpsServer({cert: await readFile(cert), key: await readFile(key)}, (request, response) => {
      const url = new URL(request.url, issuer); if (url.pathname !== '/authorize') return response.writeHead(404).end();
      callbackNonce = url.searchParams.get('nonce'); const callback = new URL(url.searchParams.get('redirect_uri'));
      callback.searchParams.set('code', 'hosted-code'); callback.searchParams.set('state', url.searchParams.get('state'));
      response.writeHead(302, {Location: callback.href}).end();
    });
    issuerServer.listen(0, '127.0.0.1'); await new Promise(resolvePromise => issuerServer.once('listening', resolvePromise));
    issuerPort = issuerServer.address().port;
    wikiServer = createHttpsServer({cert: await readFile(cert), key: await readFile(key)}, (request, response) => {
      const url = new URL(request.url, wikiProofUrl); if (url.pathname !== '/library/_mct-proof') return response.writeHead(404).end();
      ticketTokenSeen = url.searchParams.get('t');
      const ticket = verifyEnvelope(ticketTokenSeen, approvalProofKeys.publicKey); ticketSeen = ticket;
      assert.equal(ticket.kind, 'mct-access-proof-ticket/v1'); assert.equal(ticket.expected_subject, 'nicabar');
      assert.equal(ticket.required_group, 'wiki-admin'); assert.equal(ticket.path, '/library/example-handbook.pdf');
      const ticketKey = `${ticket.request_id}\0${ticket.nonce}`;
      if (consumedTickets.has(ticketKey)) return response.writeHead(410).end();
      consumedTickets.add(ticketKey);
      const now = Math.floor(Date.now()/1000);
      const result = signEnvelope({kind: 'mct-access-proof-result/v1', request_id: ticket.request_id, nonce: ticket.nonce,
        path: ticket.path, subject: 'nicabar', outcome: 'allowed', reason: 'ok', iat: now, exp: now+60}, wikiKeys.privateKey);
      const back = new URL(ticket.return_url); back.searchParams.set('r', result); response.writeHead(303, {Location: back.href}).end();
    });
    wikiServer.listen(0, '127.0.0.1'); await new Promise(resolvePromise => wikiServer.once('listening', resolvePromise));
    wikiPort = wikiServer.address().port;
    queue = spawn('python3', [join(SOURCE_ROOT, 'scripts/serve-passkey-tap.py'), '--session', session, '--origin', origin,
      '--listen', '127.0.0.1:0', '--cert', cert, '--key', key],
    {cwd: SOURCE_ROOT, env: {...process.env, PYTHONDONTWRITEBYTECODE: '1'}});
    const queueOutput = processOutput(queue);
    await eventually(async () => {
      if (queue.exitCode !== null) throw new Error(queueOutput.stderr || `queue exited ${queue.exitCode}`);
      const match = queueOutput.stdout.match(/Private approval queue listening at 127\.0\.0\.1:(\d+)/);
      assert(match, queueOutput.stdout); await access(join(session, 'control.sock')); queuePort = Number(match[1]);
    });
    const fetcher = async (target, options = {}) => {
      const href = String(target);
      if (href.endsWith('/.well-known/openid-configuration')) return new Response(JSON.stringify({issuer,
        authorization_endpoint: `${issuer}/authorize`, token_endpoint: `${issuer}/token`, jwks_uri: `${issuer}/jwks`, id_token_signing_alg_values_supported: ['RS256']}), {status: 200});
      if (href === `${issuer}/token`) {
        const current = Math.floor(Date.now()/1000); return new Response(JSON.stringify({id_token: idToken(oidcKeys.privateKey,
          {iss: issuer, aud: clientId, sub: subject, nonce: callbackNonce, iat: current-1, exp: current+300})}), {status: 200});
      }
      if (href === `${issuer}/jwks`) return new Response(JSON.stringify({keys: [jwk]}), {status: 200});
      throw new Error(`unexpected OIDC URL ${href}`);
    };
    gate = await runGate({MCT_APPROVAL_ORIGIN: origin, MCT_APPROVAL_LISTEN: '127.0.0.1:0',
      MCT_APPROVAL_UPSTREAM: `https://127.0.0.1:${queuePort}`, MCT_APPROVAL_UPSTREAM_CA_FILE: cert,
      MCT_APPROVAL_TLS_CERT_FILE: cert, MCT_APPROVAL_TLS_KEY_FILE: key, MCT_APPROVAL_SESSION_SECONDS: '300',
      MCT_OIDC_ISSUER: issuer, MCT_OIDC_CLIENT_ID: clientId, MCT_OIDC_CLIENT_SECRET_FILE: secret,
      MCT_OIDC_ALLOWED_SUBJECTS_FILE: subjects, MCT_OIDC_TOKEN_AUTH_METHOD: 'client_secret_basic',
      MCT_APPROVAL_PROOF_SIGNING_KEY_FILE: proofPrivateFile, MCT_APPROVAL_WIKI_VERIFY_KEY_FILE: wikiPublicFile,
      MCT_APPROVAL_WIKI_PROOF_URL: wikiProofUrl}, {fetcher});
    if (!gate.listening) await new Promise(resolvePromise => gate.once('listening', resolvePromise));
    gatePort = gate.address().port;
    const chromeBin = process.env.CHROME_BIN || (process.platform === 'darwin' ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : 'google-chrome');
    const profile = join(directory, 'chrome');
    chrome = spawn(chromeBin, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run',
      '--no-default-browser-check', '--disable-background-networking', '--password-store=basic',
      `--host-resolver-rules=MAP mct-approval.nicabar.org:443 127.0.0.1:${gatePort}, MAP id.nicabar.org:443 127.0.0.1:${issuerPort}, MAP wiki.nicabar.org:443 127.0.0.1:${wikiPort}`,
      `--ignore-certificate-errors-spki-list=${spki}`, 'about:blank'], {stdio: 'ignore'});
    const [debugPort, socketPath] = await eventually(async () => (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).trim().split('\n'));
    const socket = new WebSocket(`ws://127.0.0.1:${debugPort}${socketPath}`); await new Promise((ok, fail) => { socket.addEventListener('open', ok, {once:true}); socket.addEventListener('error', fail, {once:true}); });
    cdp = new CDP(socket); const {targetId} = await cdp.call('Target.createTarget', {url: `${origin}/tokens.css`});
    const {sessionId} = await cdp.call('Target.attachToTarget', {targetId, flatten: true}); const page = (method, params={}) => cdp.call(method, params, sessionId);
    await page('Page.enable');
    await waitForDocumentReady(page, origin);
    await page('WebAuthn.enable', {enableUI: false});
    await page('WebAuthn.addVirtualAuthenticator', {options: {protocol: 'ctap2', transport: 'internal', hasResidentKey: true,
      hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true}});
    const rootResult = await page('Runtime.evaluate', {expression: `(async()=>JSON.stringify(await (await import('/passkey-admin.mjs')).registerRoot('mct-approval.nicabar.org','nicabar')))()`, awaitPromise: true, returnByValue: true, userGesture: true});
    assert.equal(rootResult.exceptionDetails, undefined, JSON.stringify(rootResult));
    assert.equal(typeof rootResult.result.value, 'string', JSON.stringify(rootResult));
    const root = JSON.parse(rootResult.result.value); assert.equal(root.origin, origin); assert.equal(root.rp_id, 'mct-approval.nicabar.org');
    const pin = join(runDirectory, 'registered-root.json'); await writeFile(pin, JSON.stringify(root), {mode: 0o600});
    const binary = resolve(SOURCE_ROOT, process.env.MCT_BINARY || 'target/debug/mct-daemon');
    grant = spawn('python3', [join(SOURCE_ROOT, 'scripts/passkey-approve.py'), '--session', session, '--binary', binary, 'bootstrap',
      '--root', mother, '--mother', 'unraid-mother', '--ledger-id', 'ledger-hosted', '--registered-root', pin],
    {cwd: SOURCE_ROOT, env: {...process.env, PYTHONDONTWRITEBYTECODE:'1'}});
    const grantOutput = processOutput(grant);
    const grantUrl = await eventually(async () => { const match = grantOutput.stdout.match(/https:\/\/mct-approval\.nicabar\.org\/a\/[A-Za-z0-9_-]+/); assert(match, grantOutput.stderr + grantOutput.stdout); return match[0]; });
    await scanAndSignIn(page, grantUrl);
    await eventually(async () => { const found = await page('Runtime.evaluate', {expression: 'JSON.stringify({found:typeof document.querySelector("#approve")?.onclick === "function",url:location.href,text:document.body.innerText})', returnByValue: true}); const state=JSON.parse(found.result.value); assert.equal(state.found, true, JSON.stringify(state) + grantOutput.stderr + grantOutput.stdout); });
    await click(page, '#approve');
    await eventually(async () => { const state = await page('Runtime.evaluate', {expression: 'JSON.stringify({url:location.href,text:document.body.innerText})', returnByValue: true}); assert.notEqual(grant.exitCode, null, state.result.value + grantOutput.stderr + grantOutput.stdout); }, 60000);
    assert.equal(await exitCode(grant), 0, grantOutput.stderr + grantOutput.stdout);
    assert.match(grantOutput.stdout, /✓ Approved/);
    proof = spawn('python3', [join(SOURCE_ROOT, 'scripts/passkey-approve.py'), '--session', session, 'proof', '--title', 'The Example Handbook',
      '--path', '/library/example-handbook.pdf', '--why', 'Checking the library release works for you (NIC-132)'],
    {cwd: SOURCE_ROOT, env: {...process.env, PYTHONDONTWRITEBYTECODE:'1'}});
    const proofOutput = processOutput(proof);
    const proofUrl = await eventually(async () => { const match = proofOutput.stdout.match(/https:\/\/mct-approval\.nicabar\.org\/a\/[A-Za-z0-9_-]+/); assert(match, proofOutput.stderr + proofOutput.stdout); return match[0]; });
    await scanAndSignIn(page, proofUrl);
    await eventually(async () => { const found = await page('Runtime.evaluate', {expression: 'JSON.stringify({found:typeof document.querySelector("#check")?.onclick === "function",url:location.href,text:document.body.innerText})', returnByValue: true}); const state=JSON.parse(found.result.value); assert.equal(state.found, true, JSON.stringify(state) + proofOutput.stderr + proofOutput.stdout); });
    await click(page, '#check');
    await eventually(async () => { const state = await page('Runtime.evaluate', {expression: 'JSON.stringify({url:location.href,text:document.body.innerText})', returnByValue: true}); assert.notEqual(proof.exitCode, null, state.result.value + proofOutput.stderr + proofOutput.stdout); }, 60000);
    assert.equal(await exitCode(proof), 0, proofOutput.stderr + proofOutput.stdout);
    assert.match(proofOutput.stdout, /✓ Passed/); assert(ticketSeen); assert.equal(ticketSeen.required_group, 'wiki-admin');
    assert.equal(await httpsStatus(wikiPort, 'wiki.nicabar.org', `/library/_mct-proof?t=${ticketTokenSeen}`), 410);
    t.diagnostic('direct QR scan survived cross-origin OIDC, Mother verified the MCT passkey grant, and the wiki handler contract returned a signed pass');
  } finally {
    if (cdp) cdp.close(); await stop(grant); await stop(proof); await stop(chrome);
    if (gate) await new Promise(resolvePromise => gate.close(resolvePromise)); await stop(queue);
    if (issuerServer) await new Promise(resolvePromise => issuerServer.close(resolvePromise));
    if (wikiServer) await new Promise(resolvePromise => wikiServer.close(resolvePromise));
    await rm(directory, {recursive: true, force: true});
  }
});
