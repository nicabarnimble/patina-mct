import test from 'node:test';
import assert from 'node:assert/strict';
import {generateKeyPairSync, sign} from 'node:crypto';
import {execFile} from 'node:child_process';
import {once} from 'node:events';
import {chmod, mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {createServer as createHttpsServer, request as httpsRequest} from 'node:https';
import {createServer as createNetServer} from 'node:net';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {promisify} from 'node:util';
import {
  ApprovalSessions,
  LoginTransactions,
  validateApprovalRequest,
  run,
  verifyIdToken,
} from './pocketid-approval-gate.mjs';

const issuer = 'https://id.example.test';
const clientId = 'mct-approval';
const nonce = 'nonce-1';
const subject = 'operator-subject-1';
const now = 1_800_000_000;
const {privateKey, publicKey} = generateKeyPairSync('rsa', {modulusLength: 2048});
const jwk = publicKey.export({format: 'jwk'});
jwk.kid = 'root-1';
jwk.alg = 'RS256';
jwk.use = 'sig';

const b64 = value => Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)).toString('base64url');
function token(overrides = {}, key = privateKey) {
  const header = b64({alg: 'RS256', kid: 'root-1', typ: 'JWT'});
  const payload = b64({
    iss: issuer, aud: clientId, sub: subject, nonce,
    iat: now - 5, exp: now + 300, ...overrides,
  });
  const input = `${header}.${payload}`;
  return `${input}.${sign('RSA-SHA256', Buffer.from(input), key).toString('base64url')}`;
}
const options = {
  issuer, clientId, nonce, allowedSubjects: new Set([subject]),
  jwks: {keys: [jwk]}, now,
};

test('valid Pocket ID token gates page access but creates no MCT authority', () => {
  const claims = verifyIdToken(token(), options);
  assert.equal(claims.sub, subject);
  assert.deepEqual(Object.keys(claims).sort(), ['aud', 'exp', 'iat', 'iss', 'nonce', 'sub']);
});

test('wrong issuer, audience, authorized party, nonce, signature, expiry, and subject are refused', () => {
  for (const [name, candidate, pattern] of [
    ['issuer', token({iss: 'https://wrong.example.test'}), /issuer/],
    ['audience', token({aud: 'other-client'}), /audience/],
    ['authorized party', token({aud: [clientId, 'other'], azp: 'other'}), /authorized party/],
    ['nonce', token({nonce: 'wrong'}), /nonce/],
    ['expiry', token({exp: now - 31}), /expired/],
    ['subject', token({sub: 'not-allowed'}), /subject/],
  ]) assert.throws(() => verifyIdToken(candidate, options), pattern, name);
  const other = generateKeyPairSync('rsa', {modulusLength: 2048}).privateKey;
  assert.throws(() => verifyIdToken(token({}, other), options), /signature/);
});

test('OIDC state is one-use and replay is refused', () => {
  const transactions = new LoginTransactions({now: () => 1000, random: (() => {
    let value = 0;
    return () => `random-${++value}`;
  })()});
  const pending = transactions.create();
  assert.equal(transactions.consume(pending.id, pending.state).nonce, pending.nonce);
  assert.throws(() => transactions.consume(pending.id, pending.state), /login transaction/);
  const expired = transactions.create();
  assert.throws(() => transactions.consume(expired.id, expired.state, 2000), /expired/);
  const bounded = new LoginTransactions({now: () => 1000, random: (() => { let value = 0; return () => `bounded-${++value}`; })(), limit: 1});
  bounded.create();
  assert.throws(() => bounded.create(), /too many/);
});

test('login alone cannot create a queue session or mutate without CSRF', () => {
  const sessions = new ApprovalSessions({now: () => 1000, random: (() => {
    let value = 0;
    return () => `session-${++value}`;
  })()});
  const created = sessions.create(subject);
  const session = sessions.get(created.id);
  assert.equal(session.subject, subject);
  assert.throws(() => validateApprovalRequest({
    method: 'POST', path: '/api/session', origin: 'https://approval.example.test',
    expectedOrigin: 'https://approval.example.test', csrf: '', session, body: {},
  }), /CSRF/);
  assert.throws(() => validateApprovalRequest({
    method: 'POST', path: '/api/session', origin: 'https://approval.example.test',
    expectedOrigin: 'https://approval.example.test', csrf: session.csrf, session, body: {},
  }), /one-use queue capability/);
  assert.doesNotThrow(() => validateApprovalRequest({
    method: 'POST', path: '/api/session', origin: 'https://approval.example.test',
    expectedOrigin: 'https://approval.example.test', csrf: session.csrf, session,
    body: {token: 'independent-fragment-capability'},
  }));
  assert.throws(() => validateApprovalRequest({
    method: 'POST', path: '/api/next', origin: 'https://approval.example.test',
    expectedOrigin: 'https://approval.example.test', csrf: session.csrf, session, body: {},
  }), /independent queue session/);
  assert.doesNotThrow(() => validateApprovalRequest({
    method: 'POST', path: '/api/next', origin: 'https://approval.example.test',
    expectedOrigin: 'https://approval.example.test', csrf: session.csrf, session,
    queueSession: true, body: {},
  }));
  assert.throws(() => validateApprovalRequest({
    method: 'POST', path: '/api/next', origin: 'https://evil.example.test',
    expectedOrigin: 'https://approval.example.test', csrf: session.csrf, session, body: {},
  }), /origin/);
});

const exec = promisify(execFile);
async function freePort() {
  const server = createNetServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = server.address().port;
  server.close();
  await once(server, 'close');
  return port;
}
function httpsCall(port, host, path, {method = 'GET', cookie = '', origin, csrf, body} = {}) {
  return new Promise((resolve, reject) => {
    const data = body === undefined ? null : Buffer.from(JSON.stringify(body));
    const request = httpsRequest({hostname: '127.0.0.1', port, path, method, rejectUnauthorized: false,
      headers: {Host: host, ...(cookie ? {Cookie: cookie} : {}), ...(origin ? {Origin: origin} : {}),
        ...(csrf ? {'X-MCT-CSRF': csrf} : {}), ...(data ? {'Content-Type': 'application/json', 'Content-Length': data.length} : {})}}, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({status: response.statusCode, headers: response.headers,
        body: Buffer.concat(chunks).toString()}));
    });
    request.on('error', reject);
    request.end(data);
  });
}
const cookieValue = (headers, name) => (headers['set-cookie'] || [])
  .map(value => value.split(';', 1)[0]).find(value => value.startsWith(`${name}=`));

test('full gate login keeps OIDC session separate from one-use queue capability', {timeout: 20000}, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'mct-pocketid-gate-'));
  const cert = join(directory, 'cert.pem');
  const key = join(directory, 'key.pem');
  const secret = join(directory, 'client-secret');
  const subjects = join(directory, 'subjects.json');
  let gate, upstream;
  try {
    await exec('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key,
      '-out', cert, '-days', '1', '-subj', '/CN=approval.example.test',
      '-addext', 'subjectAltName=DNS:approval.example.test']);
    await writeFile(secret, 'test-client-secret\n', {mode: 0o600});
    await writeFile(subjects, JSON.stringify([subject]), {mode: 0o600});
    await chmod(key, 0o600);
    const gatePort = await freePort();
    const upstreamPort = await freePort();
    const approvalOrigin = `https://approval.example.test:${gatePort}`;
    let upstreamCalls = 0;
    upstream = createHttpsServer({cert: await readFile(cert), key: await readFile(key)}, (request, response) => {
      upstreamCalls++;
      const isSession = request.url === '/api/session';
      response.writeHead(200, {'Content-Type': 'application/json',
        ...(isSession ? {'Set-Cookie': '__Host-mct=queue-session; Path=/; Secure; HttpOnly; SameSite=Strict'} : {})});
      response.end(JSON.stringify(isSession ? {page_id: 'page-1'} : {phase: 'waiting'}));
    });
    upstream.listen(upstreamPort, '127.0.0.1');
    await once(upstream, 'listening');
    let callbackNonce = '';
    const fetcher = async (url, options = {}) => {
      const href = String(url);
      if (href.endsWith('/.well-known/openid-configuration')) return new Response(JSON.stringify({
        issuer, authorization_endpoint: `${issuer}/authorize`, token_endpoint: `${issuer}/token`,
        jwks_uri: `${issuer}/jwks`, id_token_signing_alg_values_supported: ['RS256'],
      }), {status: 200});
      if (href === `${issuer}/token`) {
        assert.match(options.headers.Authorization, /^Basic /);
        const current = Math.floor(Date.now() / 1000);
        return new Response(JSON.stringify({id_token: token({nonce: callbackNonce, iat: current - 1, exp: current + 300})}), {status: 200});
      }
      if (href === `${issuer}/jwks`) return new Response(JSON.stringify({keys: [jwk]}), {status: 200});
      throw new Error(`unexpected OIDC URL ${href}`);
    };
    const env = {
      MCT_APPROVAL_ORIGIN: approvalOrigin, MCT_APPROVAL_LISTEN: `127.0.0.1:${gatePort}`,
      MCT_APPROVAL_UPSTREAM: `https://127.0.0.1:${upstreamPort}`,
      MCT_APPROVAL_UPSTREAM_CA_FILE: cert, MCT_APPROVAL_TLS_CERT_FILE: cert,
      MCT_APPROVAL_TLS_KEY_FILE: key, MCT_APPROVAL_SESSION_SECONDS: '300',
      MCT_OIDC_ISSUER: issuer, MCT_OIDC_CLIENT_ID: clientId,
      MCT_OIDC_CLIENT_SECRET_FILE: secret, MCT_OIDC_ALLOWED_SUBJECTS_FILE: subjects,
      MCT_OIDC_TOKEN_AUTH_METHOD: 'client_secret_basic',
    };
    gate = await run(env, {fetcher});
    if (!gate.listening) await once(gate, 'listening');
    const host = `approval.example.test:${gatePort}`;
    assert.equal((await httpsCall(gatePort, host, '/')).status, 302);
    const login = await httpsCall(gatePort, host, '/login');
    assert.equal(login.status, 302);
    const stateCookie = cookieValue(login.headers, '__Host-mct-oidc-state');
    const authorization = new URL(login.headers.location);
    callbackNonce = authorization.searchParams.get('nonce');
    const callback = await httpsCall(gatePort, host,
      `/oidc/callback?code=code-1&state=${encodeURIComponent(authorization.searchParams.get('state'))}`,
      {cookie: stateCookie});
    assert.equal(callback.status, 302, callback.body);
    const sessionCookie = cookieValue(callback.headers, '__Host-mct-oidc-session');
    const authenticated = await httpsCall(gatePort, host, '/api/auth', {cookie: sessionCookie});
    assert.equal(authenticated.status, 200);
    const identity = JSON.parse(authenticated.body);
    assert.equal(identity.subject, subject);

    const noQueue = await httpsCall(gatePort, host, '/api/next', {method: 'POST', cookie: sessionCookie,
      origin: approvalOrigin, csrf: identity.csrf, body: {}});
    assert.equal(noQueue.status, 400);
    assert.equal(upstreamCalls, 0);
    const noCapability = await httpsCall(gatePort, host, '/api/session', {method: 'POST', cookie: sessionCookie,
      origin: approvalOrigin, csrf: identity.csrf, body: {}});
    assert.equal(noCapability.status, 400);
    assert.equal(upstreamCalls, 0);
    const exchange = await httpsCall(gatePort, host, '/api/session', {method: 'POST', cookie: sessionCookie,
      origin: approvalOrigin, csrf: identity.csrf, body: {token: 'independent-fragment-capability'}});
    assert.equal(exchange.status, 200, exchange.body);
    assert.equal(upstreamCalls, 1);
    const queueCookie = cookieValue(exchange.headers, '__Host-mct');
    const next = await httpsCall(gatePort, host, '/api/next', {method: 'POST',
      cookie: `${sessionCookie}; ${queueCookie}`, origin: approvalOrigin, csrf: identity.csrf, body: {}});
    assert.equal(next.status, 200, next.body);
    assert.equal(upstreamCalls, 2);
    const wrongOrigin = await httpsCall(gatePort, host, '/api/next', {method: 'POST',
      cookie: `${sessionCookie}; ${queueCookie}`, origin: 'https://evil.example.test', csrf: identity.csrf, body: {}});
    assert.equal(wrongOrigin.status, 400);
    assert.equal(upstreamCalls, 2);
  } finally {
    if (gate) { gate.close(); await once(gate, 'close'); }
    if (upstream) { upstream.close(); await once(upstream, 'close'); }
    await rm(directory, {recursive: true, force: true});
  }
});
