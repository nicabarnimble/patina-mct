import test from 'node:test';
import assert from 'node:assert/strict';
import {execFile, spawn} from 'node:child_process';
import {createHash, generateKeyPairSync, sign} from 'node:crypto';
import {access, chmod, mkdir, mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {createServer as createHttpsServer} from 'node:https';
import {createServer as createNetServer} from 'node:net';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {promisify} from 'node:util';

const SOURCE_ROOT = resolve(process.env.MCT_SOURCE_ROOT || join(import.meta.dirname, '..'));
const {run: runGate} = await import(pathToFileURL(join(SOURCE_ROOT, 'scripts/pocketid-approval-gate.mjs')));
const exec = promisify(execFile);
const sleep = milliseconds => new Promise(resolvePromise => setTimeout(resolvePromise, milliseconds));

async function eventually(task, timeout = 15000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    try { return await task(); } catch (error) { last = error; await sleep(50); }
  }
  throw last;
}

async function stop(child, group = false) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  if (group) process.kill(-child.pid, 'SIGTERM');
  else child.kill('SIGTERM');
  await eventually(async () => assert(child.exitCode !== null || child.signalCode !== null), 10000)
    .catch(() => group ? process.kill(-child.pid, 'SIGKILL') : child.kill('SIGKILL'));
}

async function freePort() {
  const server = createNetServer();
  server.listen(0, '127.0.0.1');
  await new Promise(resolvePromise => server.once('listening', resolvePromise));
  const port = server.address().port;
  await new Promise(resolvePromise => server.close(resolvePromise));
  return port;
}

class CDP {
  constructor(socket) {
    this.socket = socket;
    this.next = 0;
    this.pending = new Map();
    socket.addEventListener('message', event => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const waiter = this.pending.get(message.id);
      this.pending.delete(message.id);
      if (!waiter) return;
      clearTimeout(waiter.timer);
      message.error ? waiter.reject(new Error(JSON.stringify(message.error))) : waiter.resolve(message.result);
    });
  }
  call(method, params = {}, sessionId) {
    return new Promise((resolvePromise, reject) => {
      const id = ++this.next;
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP timeout: ${method}`));
      }, 15000);
      this.pending.set(id, {resolve: resolvePromise, reject, timer});
      this.socket.send(JSON.stringify({id, method, params, sessionId}));
    });
  }
  close() {
    for (const waiter of this.pending.values()) {
      clearTimeout(waiter.timer);
      waiter.reject(new Error('CDP closed'));
    }
    this.pending.clear();
    this.socket.close();
  }
}

async function certificate(directory) {
  const cert = join(directory, 'cert.pem');
  const key = join(directory, 'key.pem');
  await exec('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key,
    '-out', cert, '-days', '1', '-subj', '/CN=approval.example.test',
    '-addext', 'subjectAltName=DNS:approval.example.test,DNS:id.example.test']);
  await chmod(key, 0o600);
  const {stdout: publicKey} = await exec('openssl', ['x509', '-in', cert, '-pubkey', '-noout']);
  const converted = spawn('openssl', ['pkey', '-pubin', '-outform', 'DER'], {stdio: ['pipe', 'pipe', 'ignore']});
  const chunks = [];
  converted.stdout.on('data', data => chunks.push(data));
  converted.stdin.end(publicKey);
  await new Promise((resolvePromise, reject) => converted.on('exit', code => code === 0
    ? resolvePromise() : reject(new Error('SPKI conversion failed'))));
  return {cert, key, spki: createHash('sha256').update(Buffer.concat(chunks)).digest('base64')};
}

const base64url = value => Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)).toString('base64url');

function idToken(privateKey, claims) {
  const header = base64url({alg: 'RS256', kid: 'deployment-test', typ: 'JWT'});
  const payload = base64url(claims);
  const input = `${header}.${payload}`;
  return `${input}.${sign('RSA-SHA256', Buffer.from(input), privateKey).toString('base64url')}`;
}

async function queueContract(origin, field) {
  const directory = await mkdtemp(join(tmpdir(), 'mct-pocketid-contract-'));
  const session = join(directory, 'session');
  await chmod(directory, 0o700);
  await mkdir(session, {mode: 0o700});
  try {
    const probe = `
import json
import sys
sys.path.insert(0, sys.argv[1])
from passkey_queue import Broker
broker = Broker(sys.argv[2], sys.argv[3])
value = broker.rp_id if sys.argv[4] == "rp_id" else broker.cli({"op": "open"})["url"]
print(json.dumps({sys.argv[4]: value}))
`;
    const result = await exec('python3', ['-c', probe, join(SOURCE_ROOT, 'scripts'), session, origin, field],
      {cwd: SOURCE_ROOT});
    return JSON.parse(result.stdout);
  } finally {
    await rm(directory, {recursive: true, force: true});
  }
}

test('hosted queue derives the WebAuthn RP ID from the configured origin', async () => {
  const contract = await queueContract('https://approval.example.test:8443', 'rp_id');
  assert.equal(contract.rp_id, 'approval.example.test');
});

test('one-use capability URL forces a post-login document load', async () => {
  const contract = await queueContract('https://mct.localhost:8443', 'url');
  const url = new URL(contract.url);
  assert.equal(url.pathname, '/approve/');
  assert.match(url.hash, /^#[A-Za-z0-9_-]{32,}$/);
});

test('hosted non-localhost login-first ceremony reaches Mother through a headless URL handoff', {timeout: 180000}, async t => {
  const directory = await mkdtemp(join(tmpdir(), 'mct-pocketid-deployment-'));
  const session = join(directory, 'session');
  const runDirectory = join(directory, 'run');
  await mkdir(session, {mode: 0o700});
  const {cert, key, spki} = await certificate(directory);
  const gatePort = await freePort();
  const queuePort = await freePort();
  const issuerPort = await freePort();
  const origin = `https://approval.example.test:${gatePort}`;
  const issuer = `https://id.example.test:${issuerPort}`;
  const clientId = 'mct-deployment-test';
  const subject = 'operator-deployment-test';
  const secret = join(directory, 'client-secret');
  const subjects = join(directory, 'subjects.json');
  await writeFile(secret, 'disposable-test-secret\n', {mode: 0o600});
  await writeFile(subjects, JSON.stringify([subject]), {mode: 0o600});

  const {privateKey, publicKey} = generateKeyPairSync('rsa', {modulusLength: 2048});
  const jwk = publicKey.export({format: 'jwk'});
  Object.assign(jwk, {kid: 'deployment-test', alg: 'RS256', use: 'sig'});
  let callbackNonce = '';
  let issuerServer, gate, queue, chrome, demo, cdp;
  let queueErrors = '';
  let demoOutput = '';
  let demoErrors = '';
  try {
    issuerServer = createHttpsServer({cert: await readFile(cert), key: await readFile(key)}, (request, response) => {
      const url = new URL(request.url, issuer);
      if (url.pathname !== '/authorize') {
        response.writeHead(404).end();
        return;
      }
      callbackNonce = url.searchParams.get('nonce');
      const callback = new URL(url.searchParams.get('redirect_uri'));
      callback.searchParams.set('code', 'deployment-code');
      callback.searchParams.set('state', url.searchParams.get('state'));
      response.writeHead(302, {Location: callback.href}).end();
    });
    issuerServer.listen(issuerPort, '127.0.0.1');
    await new Promise(resolvePromise => issuerServer.once('listening', resolvePromise));

    queue = spawn('python3', [join(SOURCE_ROOT, 'scripts/serve-passkey-tap.py'), '--session', session,
      '--origin', origin, '--listen', `127.0.0.1:${queuePort}`, '--cert', cert, '--key', key],
    {cwd: SOURCE_ROOT});
    queue.stderr.on('data', data => { queueErrors += data; });
    await eventually(async () => {
      if (queue.exitCode !== null) throw new Error(queueErrors || `queue exited ${queue.exitCode}`);
      await access(join(session, 'control.sock'));
    });

    const fetcher = async (target, options = {}) => {
      const href = String(target);
      if (href.endsWith('/.well-known/openid-configuration')) return new Response(JSON.stringify({
        issuer, authorization_endpoint: `${issuer}/authorize`, token_endpoint: `${issuer}/token`,
        jwks_uri: `${issuer}/jwks`, id_token_signing_alg_values_supported: ['RS256'],
      }), {status: 200});
      if (href === `${issuer}/token`) {
        assert.equal(options.method, 'POST');
        assert(callbackNonce);
        const current = Math.floor(Date.now() / 1000);
        return new Response(JSON.stringify({id_token: idToken(privateKey, {
          iss: issuer, aud: clientId, sub: subject, nonce: callbackNonce,
          iat: current - 1, exp: current + 300,
        })}), {status: 200});
      }
      if (href === `${issuer}/jwks`) return new Response(JSON.stringify({keys: [jwk]}), {status: 200});
      throw new Error(`unexpected OIDC URL ${href}`);
    };
    gate = await runGate({
      MCT_APPROVAL_ORIGIN: origin,
      MCT_APPROVAL_LISTEN: `127.0.0.1:${gatePort}`,
      MCT_APPROVAL_UPSTREAM: `https://127.0.0.1:${queuePort}`,
      MCT_APPROVAL_UPSTREAM_CA_FILE: cert,
      MCT_APPROVAL_TLS_CERT_FILE: cert,
      MCT_APPROVAL_TLS_KEY_FILE: key,
      MCT_APPROVAL_SESSION_SECONDS: '300',
      MCT_OIDC_ISSUER: issuer,
      MCT_OIDC_CLIENT_ID: clientId,
      MCT_OIDC_CLIENT_SECRET_FILE: secret,
      MCT_OIDC_ALLOWED_SUBJECTS_FILE: subjects,
      MCT_OIDC_TOKEN_AUTH_METHOD: 'client_secret_basic',
    }, {fetcher});
    if (!gate.listening) await new Promise(resolvePromise => gate.once('listening', resolvePromise));

    const chromeBin = process.env.CHROME_BIN || (process.platform === 'darwin'
      ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : 'google-chrome');
    const profile = join(directory, 'chrome');
    chrome = spawn(chromeBin, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`,
      '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--password-store=basic',
      '--host-resolver-rules=MAP approval.example.test 127.0.0.1, MAP id.example.test 127.0.0.1',
      `--ignore-certificate-errors-spki-list=${spki}`, 'about:blank'], {stdio: 'ignore'});
    const [debugPort, socketPath] = await eventually(async () =>
      (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).trim().split('\n'));
    const socket = new WebSocket(`ws://127.0.0.1:${debugPort}${socketPath}`);
    await new Promise((resolvePromise, reject) => {
      socket.addEventListener('open', resolvePromise, {once: true});
      socket.addEventListener('error', reject, {once: true});
    });
    cdp = new CDP(socket);
    const {targetId} = await cdp.call('Target.createTarget', {url: 'about:blank'});
    const {sessionId} = await cdp.call('Target.attachToTarget', {targetId, flatten: true});
    const page = (method, params = {}) => cdp.call(method, params, sessionId);
    await page('Page.enable');
    await page('WebAuthn.enable', {enableUI: false});
    await page('WebAuthn.addVirtualAuthenticator', {options: {protocol: 'ctap2', transport: 'internal',
      hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true}});
    await page('Page.addScriptToEvaluateOnNewDocument', {source: `
      window.deploymentMetrics = {gets: 0, creates: 0, gestures: [], clicks: 0, reviews: []};
      for (const method of ['get', 'create']) {
        const original = navigator.credentials[method].bind(navigator.credentials);
        navigator.credentials[method] = (...args) => {
          deploymentMetrics[method === 'get' ? 'gets' : 'creates']++;
          deploymentMetrics.gestures.push(navigator.userActivation.isActive);
          return original(...args);
        };
      }
      document.addEventListener('click', event => {
        if (event.target.id === 'approve') {
          deploymentMetrics.clicks++;
          deploymentMetrics.reviews.push(document.getElementById('review')?.textContent || '');
        }
      });
    `});

    await page('Page.navigate', {url: `${origin}/login`});
    await page('Page.bringToFront');
    await eventually(async () => {
      const result = await page('Runtime.evaluate', {expression: `JSON.stringify({
        origin: location.origin,
        operator: document.getElementById('operator')?.textContent,
        state: document.getElementById('state')?.textContent,
        hash: location.hash
      })`, returnByValue: true});
      const state = JSON.parse(result.result.value);
      assert.equal(state.origin, origin);
      assert.equal(state.operator, subject);
      assert.equal(state.hash, '');
      assert.match(state.state, /Open a fresh authenticated page through the CLI/);
    });

    const opened = await exec('python3', [join(SOURCE_ROOT, 'scripts/passkey-approve.py'),
      '--session', session, 'open', '--print-url'],
    {cwd: SOURCE_ROOT, env: {...process.env, BROWSER: '/definitely-no-local-browser'}});
    const approvalUrl = opened.stdout.trim();
    assert.equal(opened.stderr, '');
    assert.equal(new URL(approvalUrl).origin, origin);
    assert.equal(new URL(approvalUrl).pathname, '/approve/');
    assert.match(new URL(approvalUrl).hash, /^#[A-Za-z0-9_-]{32,}$/);
    await page('Page.navigate', {url: approvalUrl});
    await page('Page.bringToFront');

    const binary = resolve(SOURCE_ROOT, process.env.MCT_BINARY || 'target/debug/mct-daemon');
    demo = spawn('python3', [join(SOURCE_ROOT, 'scripts/passkey-approval-demo.py'), '--session', session,
      '--directory', runDirectory, '--binary', binary], {cwd: SOURCE_ROOT, detached: true});
    demo.stdout.on('data', data => { demoOutput += data; });
    demo.stderr.on('data', data => { demoErrors += data; });
    const finished = new Promise(resolvePromise => demo.once('exit', resolvePromise));
    await eventually(async () => {
      if (demo.exitCode !== null && demo.exitCode !== 0) throw new Error(demoErrors || demoOutput);
      const button = await page('Runtime.evaluate', {expression: `(() => {
        const element = document.getElementById('approve');
        if (!element || element.hidden) return null;
        const box = element.getBoundingClientRect();
        return {x: box.x + box.width / 2, y: box.y + box.height / 2};
      })()`, returnByValue: true});
      if (button.result.value) {
        await page('Input.dispatchMouseEvent', {type: 'mousePressed', button: 'left', clickCount: 1, ...button.result.value});
        await page('Input.dispatchMouseEvent', {type: 'mouseReleased', button: 'left', clickCount: 1, ...button.result.value});
      }
      const pageState = await page('Runtime.evaluate', {
        expression: 'document.getElementById("state")?.textContent', returnByValue: true,
      });
      assert.match(demoOutput, /PASS full ceremony/,
        `page says ${pageState.result.value}; demo stderr ${demoErrors}`);
    }, 90000);
    assert.equal(await finished, 0, demoErrors);

    const root = JSON.parse(await readFile(join(runDirectory, 'registered-root.json'), 'utf8'));
    assert.equal(root.origin, origin);
    assert.equal(root.rp_id, 'approval.example.test');
    const browser = await page('Runtime.evaluate', {expression: 'JSON.stringify(deploymentMetrics)', returnByValue: true});
    const metrics = JSON.parse(browser.result.value);
    assert.equal(metrics.creates, 1);
    assert.equal(metrics.gets, 3);
    assert.equal(metrics.clicks, 4);
    assert.deepEqual(metrics.gestures, [true, true, true, true]);
    assert.match(demoOutput, /PASS bootstrap committed/);
    assert.match(demoOutput, /PASS issue committed/);
    assert.match(demoOutput, /PASS revoke committed/);
    assert.equal(JSON.parse(await readFile(join(runDirectory, 'day-status.reply.json'), 'utf8')).status, 'completed');
    assert.equal(JSON.parse(await readFile(join(runDirectory, 'revoked-status.reply.json'), 'utf8')).status, 'rejected');
    t.diagnostic('hosted login-first flow used a printed fragment URL, non-localhost RP, four explicit clicks, and three Mother-verified root assertions');
  } finally {
    if (cdp) cdp.close();
    await stop(demo, true);
    await stop(chrome);
    if (gate) await new Promise(resolvePromise => gate.close(resolvePromise));
    await stop(queue);
    if (issuerServer) await new Promise(resolvePromise => issuerServer.close(resolvePromise));
    await rm(directory, {recursive: true, force: true});
  }
});
