import test from 'node:test';
import assert from 'node:assert/strict';
import {spawn, execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdtemp, mkdir, readFile, readdir, access, stat} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {createServer} from 'node:net';
const exec = promisify(execFile);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function eventually(task, timeout = 15000) {
  const until = Date.now() + timeout;
  let last;
  while (Date.now() < until) {
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
  const server = createServer();
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}
class CDP {
  constructor(socket) {
    this.socket = socket; this.next = 0; this.pending = new Map(); this.fileChoosers = 0; this.downloads = 0;
    socket.addEventListener('message', event => {
      const msg = JSON.parse(event.data);
      if (msg.method === 'Page.fileChooserOpened') this.fileChoosers++;
      if (msg.method === 'Browser.downloadWillBegin') this.downloads++;
      if (msg.id) {
        const waiter = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (waiter) {
          clearTimeout(waiter.timer);
          msg.error ? waiter.reject(new Error(JSON.stringify(msg.error))) : waiter.resolve(msg.result);
        }
      }
    });
  }
  call(method, params = {}, sessionId) {
    return new Promise((resolve, reject) => {
      const id = ++this.next;
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error('CDP timeout: ' + method)); }, 15000);
      this.pending.set(id, {resolve, reject, timer});
      this.socket.send(JSON.stringify({id, method, params, sessionId}));
    });
  }
  close() {
    for (const waiter of this.pending.values()) { clearTimeout(waiter.timer); waiter.reject(new Error('CDP closed')); }
    this.pending.clear(); this.socket.close();
  }
}

for (const forcedFallback of [false, true]) {
test(forcedFallback ? 'Chrome ceremony with a simulated provider refusal and one Approve click per root action'
  : 'Chrome approvals-only genesis issue status revoke and fresh refusal across restart', {timeout: 120000}, async t => {
  const directory = await mkdtemp(join(tmpdir(), 'mct-approval-e2e-'));
  const session = join(directory, 'session');
  await mkdir(session, {mode: 0o700});
  const cert = join(directory, 'cert.pem'), key = join(directory, 'key.pem');
  await exec('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key,
    '-out', cert, '-days', '1', '-subj', '/CN=mct.localhost']);
  const {stdout: pub} = await exec('openssl', ['x509', '-in', cert, '-pubkey', '-noout']);
  const convert = spawn('openssl', ['pkey', '-pubin', '-outform', 'DER'], {stdio: ['pipe', 'pipe', 'ignore']});
  const chunks = []; convert.stdout.on('data', data => chunks.push(data)); convert.stdin.end(pub);
  await new Promise((resolve, reject) => convert.on('exit', code => code === 0 ? resolve() : reject(new Error('SPKI conversion failed'))));
  const spki = createHash('sha256').update(Buffer.concat(chunks)).digest('base64');
  const origin = 'https://mct.localhost:' + await freePort();
  const server = spawn('python3', ['scripts/serve-passkey-tap.py', '--session', session, '--origin', origin, '--cert', cert, '--key', key], {stdio: 'ignore'});
  let chrome, demo, cdp;
  try {
    await eventually(() => access(join(session, 'control.sock')));
    const chromeBin = process.env.CHROME_BIN || (process.platform === 'darwin'
      ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : 'google-chrome');
    const profile = join(directory, 'chrome');
    chrome = spawn(chromeBin, ['--headless=new', '--remote-debugging-port=0', '--user-data-dir=' + profile,
      '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--password-store=basic',
      '--host-resolver-rules=MAP mct.localhost 127.0.0.1', '--ignore-certificate-errors-spki-list=' + spki, 'about:blank'], {stdio: 'ignore'});
    chrome.on('error', () => {});
    const [port, socketPath] = await eventually(async () => (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).trim().split('\n'));
    const socket = new WebSocket('ws://127.0.0.1:' + port + socketPath);
    await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, {once: true}); socket.addEventListener('error', reject, {once: true}); });
    cdp = new CDP(socket);
    const version = await cdp.call('Browser.getVersion');
    const {targetId} = await cdp.call('Target.createTarget', {url: 'about:blank'});
    const {sessionId} = await cdp.call('Target.attachToTarget', {targetId, flatten: true});
    const page = (method, params = {}) => cdp.call(method, params, sessionId);
    await page('Page.enable');
    await page('Page.setInterceptFileChooserDialog', {enabled: true});
    await cdp.call('Browser.setDownloadBehavior', {behavior: 'deny', eventsEnabled: true});
    await page('WebAuthn.enable', {enableUI: false});
    await page('WebAuthn.addVirtualAuthenticator', {options: {protocol: 'ctap2', transport: 'internal',
      hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true}});
    await page('Page.addScriptToEvaluateOnNewDocument', {source: `
      window.ceremonyMetrics = {gets: 0, creates: 0, gestures: [], approveClicks: 0};
      const refusedChallenges = new Set();
      for (const method of ['get', 'create']) {
        const original = navigator.credentials[method].bind(navigator.credentials);
        navigator.credentials[method] = (...args) => {
          const challenge = String(args[0].publicKey.challenge);
          if (${forcedFallback} && method === 'get' && !refusedChallenges.has(challenge)) {
            refusedChallenges.add(challenge);
            return Promise.reject(new DOMException('simulated provider refusal', 'NotAllowedError'));
          }
          ceremonyMetrics[method === 'get' ? 'gets' : 'creates']++;
          ceremonyMetrics.gestures.push(navigator.userActivation.isActive);
          return original(...args);
        };
      }
      document.addEventListener('click', event => {
        if (event.target.id === 'approve') ceremonyMetrics.approveClicks++;
      });
    `});
    const runDirectory = join(directory, 'run');
    const binary = resolve(process.env.MCT_BINARY || 'target/debug/mct-daemon');
    demo = spawn('python3', ['scripts/passkey-approval-demo.py', '--session', session, '--directory', runDirectory, '--binary', binary], {detached: true});
    let output = '', errors = '';
    demo.stdout.on('data', data => { output += data; });
    demo.stderr.on('data', data => { errors += data; });
    const finished = new Promise(resolve => demo.on('exit', code => resolve(code)));
    await eventually(async () => assert.match(output, /WAIT register/));
    await sleep(500);
    assert.equal((await readdir(session)).filter(name => name.endsWith('.json')).length, 0,
      'no intent or registration before the page is present');
    const {stdout} = await exec('python3', ['-c',
      'import sys,json; sys.path.insert(0,"scripts"); from passkey_queue import rpc; print(json.dumps(rpc(sys.argv[1],{"op":"open"})))', session]);
    const openedAt = Date.now();
    await page('Page.navigate', {url: JSON.parse(stdout).url});
    await page('Page.bringToFront');
    await eventually(async () => {
      const result = await page('Runtime.evaluate', {expression: 'document.getElementById("state")?.textContent', returnByValue: true});
      if (demo.exitCode !== null && demo.exitCode !== 0) throw new Error(errors);
      if (forcedFallback) {
        const button = await page('Runtime.evaluate', {expression: `(() => {
          const b = document.getElementById('approve');
          if (!b || b.hidden) return null;
          const r = b.getBoundingClientRect(); return {x:r.x+r.width/2, y:r.y+r.height/2};
        })()`, returnByValue: true});
        if (button.result.value) {
          const point = button.result.value;
          await page('Input.dispatchMouseEvent', {type:'mousePressed', button:'left', clickCount:1, ...point});
          await page('Input.dispatchMouseEvent', {type:'mouseReleased', button:'left', clickCount:1, ...point});
        }
      }
      assert.match(output, /PASS full ceremony/, 'waiting for ceremony; page says ' + result.result.value);
    }, 75000);
    assert.equal(await finished, 0, errors);
    const {result} = await page('Runtime.evaluate', {expression: `JSON.stringify({
      ...ceremonyMetrics, files: document.querySelectorAll('input[type=file]').length,
      signButtons: [...document.querySelectorAll('button')].filter(b => /sign/i.test(b.textContent)).length,
      fragment: location.hash, cookie: document.cookie})`, returnByValue: true});
    const metrics = JSON.parse(result.value);
    assert.equal(metrics.gets, 3);
    assert.equal(metrics.creates, 1);
    assert.equal(metrics.approveClicks, forcedFallback ? 3 : 0);
    assert.deepEqual(metrics.gestures, [false, forcedFallback, forcedFallback, forcedFallback]);
    assert.equal(metrics.files, 0); assert.equal(metrics.signButtons, 0); assert.equal(cdp.fileChoosers, 0); assert.equal(cdp.downloads, 0);
    assert.equal(metrics.fragment, ''); assert.equal(metrics.cookie, '');
    const requests = [];
    for (const name of await readdir(session)) {
      if (/^[a-f0-9]{64}\.json$/.test(name)) {
        const data = JSON.parse(await readFile(join(session, name), 'utf8'));
        if (data.intent) requests.push(data);
      }
    }
    assert.equal(requests.length, 3);
    assert.deepEqual(requests.map(r => r.intent.action.action).sort(), ['bootstrap', 'issue_day_key', 'revoke_day_key']);
    for (const request of requests) assert(Date.parse(request.intent.issued_at) >= openedAt - 1000);
    const statusIntents = [];
    for (const [name, status] of [['day-status', 'completed'], ['revoked-status', 'rejected'], ['restarted-status', 'rejected']]) {
      assert.equal(JSON.parse(await readFile(join(runDirectory, name + '.reply.json'), 'utf8')).status, status);
      statusIntents.push(JSON.parse(await readFile(join(runDirectory, name + '.intent.json'), 'utf8')));
    }
    assert.equal(new Set(statusIntents.map(i => i.nonce)).size, 3);
    assert.notEqual(statusIntents[1].epoch, statusIntents[2].epoch);
    for (const folder of [session, runDirectory]) {
      for (const name of await readdir(folder)) {
        if (name.endsWith('.json') || name === 'resident.log') assert.equal((await stat(join(folder, name))).mode & 0o777, 0o600);
      }
    }
    t.diagnostic(version.product + ': 3 root assertions, ' + metrics.approveClicks + ' Approve clicks, 0 file selections; '
      + (forcedFallback ? 'simulated provider refusal, not a measured Chrome requirement' : 'no transient user activation'));
    t.diagnostic('Private virtual-authenticator artifacts retained at ' + directory);
  } finally {
    if (cdp) cdp.close();
    await stop(demo, true); await stop(chrome); await stop(server);
  }
});
}
