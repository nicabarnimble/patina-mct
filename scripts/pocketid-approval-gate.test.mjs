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
import {ApprovalSessions, LinkSessions, LoginTransactions, run, verifyIdToken} from './pocketid-approval-gate.mjs';

const issuer = 'https://id.example.test';
const clientId = 'mct-approval';
const nonce = 'nonce-1';
const subject = 'operator-subject-1';
const now = 1_800_000_000;
const {privateKey, publicKey} = generateKeyPairSync('rsa', {modulusLength: 2048});
const jwk = publicKey.export({format: 'jwk'});
Object.assign(jwk, {kid: 'root-1', alg: 'RS256', use: 'sig'});
const b64 = value => Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)).toString('base64url');
function token(overrides = {}, key = privateKey) {
  const header = b64({alg: 'RS256', kid: 'root-1', typ: 'JWT'});
  const payload = b64({iss: issuer, aud: clientId, sub: subject, nonce, iat: now - 5, exp: now + 300, ...overrides});
  const input = `${header}.${payload}`;
  return `${input}.${sign('RSA-SHA256', Buffer.from(input), key).toString('base64url')}`;
}
const options = {issuer, clientId, nonce, allowedSubjects: new Set([subject]), jwks: {keys: [jwk]}, now};

test('valid Pocket ID token authenticates identity but creates no MCT authority', () => {
  const claims = verifyIdToken(token(), options);
  assert.equal(claims.sub, subject);
  assert.equal('authority' in claims, false);
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

test('one-use login state remains bound to the HttpOnly link session', () => {
  let value = 0;
  const source = () => `random-${++value}`;
  const links = new LinkSessions({now: () => 1000, random: source});
  const transactions = new LoginTransactions({now: () => 1000, random: source});
  const link = links.create('capability-token');
  const pending = transactions.create(link.id);
  assert.equal(transactions.consume(pending.id, pending.state).linkId, link.id);
  assert.throws(() => transactions.consume(pending.id, pending.state), /login transaction/);
  assert.equal(links.get(link.id).token, 'capability-token');
});

test('Pocket ID session alone has no queue session, passkey proof, or Mother authority', () => {
  let value = 0;
  const sessions = new ApprovalSessions({now: () => 1000, random: () => `session-${++value}`});
  const session = sessions.get(sessions.create('nicabar').id);
  assert.equal(session.subject, 'nicabar');
  assert.equal(session.queueCookie, undefined);
  assert.equal(session.pageId, undefined);
  assert.equal(session.proof, undefined);
  assert.equal(session.authority, undefined);
});

const exec = promisify(execFile);
async function freePort() {
  const server = createNetServer(); server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const port = server.address().port; server.close(); await once(server, 'close'); return port;
}
function httpsCall(port, host, path, {method = 'GET', cookie = '', origin, csrf, body} = {}) {
  return new Promise((resolve, reject) => {
    const data = body === undefined ? null : Buffer.from(JSON.stringify(body));
    const request = httpsRequest({hostname: '127.0.0.1', port, path, method, rejectUnauthorized: false,
      headers: {Host: host, ...(cookie ? {Cookie: cookie} : {}), ...(origin ? {Origin: origin} : {}),
        ...(csrf ? {'X-MCT-CSRF': csrf} : {}), ...(data ? {'Content-Type': 'application/json', 'Content-Length': data.length} : {})}}, response => {
      const chunks = []; response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({status: response.statusCode, headers: response.headers, body: Buffer.concat(chunks).toString()}));
    });
    request.on('error', reject); request.end(data);
  });
}
const namedCookie = (headers, name) => (headers['set-cookie'] || []).find(value => value.startsWith(`${name}=`));
const cookiePair = value => value?.split(';', 1)[0];

test('direct QR scan survives OIDC and reveals details only after the bound callback', {timeout: 20000}, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'mct-pocketid-gate-'));
  const cert = join(directory, 'cert.pem'); const key = join(directory, 'key.pem');
  const secret = join(directory, 'client-secret'); const subjects = join(directory, 'subjects.json');
  const proofPrivate = join(directory, 'proof-private.pem'); const wikiPublic = join(directory, 'wiki-public.pem');
  const proofKeys = generateKeyPairSync('ed25519'); const wikiKeys = generateKeyPairSync('ed25519');
  let gate, upstream;
  try {
    await exec('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', cert, '-days', '1',
      '-subj', '/CN=approval.example.test', '-addext', 'subjectAltName=DNS:approval.example.test']);
    await writeFile(secret, 'test-client-secret\n', {mode: 0o600});
    await writeFile(subjects, JSON.stringify([subject]), {mode: 0o600});
    await writeFile(proofPrivate, proofKeys.privateKey.export({format: 'pem', type: 'pkcs8'}), {mode: 0o600});
    await writeFile(wikiPublic, wikiKeys.publicKey.export({format: 'pem', type: 'spki'}), {mode: 0o600});
    await chmod(key, 0o600);
    const gatePort = await freePort(); const upstreamPort = await freePort();
    const approvalOrigin = `https://approval.example.test:${gatePort}`;
    const capability = 'abcdefghijklmnopqrstuv';
    let claims = 0;
    upstream = createHttpsServer({cert: await readFile(cert), key: await readFile(key)}, async (request, response) => {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      const parsed = chunks.length ? JSON.parse(Buffer.concat(chunks)) : {};
      response.setHeader('Content-Type', 'application/json');
      if (request.url === '/api/inspect') {
        assert.equal(parsed.token, capability);
        response.end(JSON.stringify({code: 'K7-QP', kind: 'grant', phase: 'waiting', deadline: Math.floor(Date.now()/1000)+600, consumed: false, asker: 'Unraid Grok'}));
      } else if (request.url === '/api/session') {
        claims++; assert.deepEqual(parsed, {token: capability, subject: 'nicabar'});
        response.setHeader('Set-Cookie', '__Host-mct-queue=queue-session; Path=/; Secure; HttpOnly; SameSite=Strict');
        response.end(JSON.stringify({page_id: 'page-1'}));
      } else if (request.url === '/api/state') {
        assert.match(request.headers.cookie, /queue-session/); assert.equal(request.headers['x-mct-page'], 'page-1');
        response.end(JSON.stringify({id: 'job', display_id: 'display', code: 'K7-QP', kind: 'grant', phase: 'opened',
          created_at: Date.now()/1000, deadline: Date.now()/1000+600, remaining_seconds: 600, consumed: false,
          request: {kind: 'grant', asker: 'Unraid Grok', mother: 'unraid-mother', permissions: [], grant_ends_at: 'tomorrow'},
          prepared: null, subject: 'nicabar', nonce: 'nonce', revision: 1}));
      } else { response.statusCode = 404; response.end('{}'); }
    });
    upstream.listen(upstreamPort, '127.0.0.1'); await once(upstream, 'listening');
    let callbackNonce = '';
    const fetcher = async (url, options = {}) => {
      const href = String(url);
      if (href.endsWith('/.well-known/openid-configuration')) return new Response(JSON.stringify({issuer,
        authorization_endpoint: `${issuer}/authorize`, token_endpoint: `${issuer}/token`, jwks_uri: `${issuer}/jwks`, id_token_signing_alg_values_supported: ['RS256']}), {status: 200});
      if (href === `${issuer}/token`) {
        assert.match(options.headers.Authorization, /^Basic /);
        const current = Math.floor(Date.now()/1000);
        return new Response(JSON.stringify({id_token: token({nonce: callbackNonce, iat: current-1, exp: current+300})}), {status: 200});
      }
      if (href === `${issuer}/jwks`) return new Response(JSON.stringify({keys: [jwk]}), {status: 200});
      throw new Error(`unexpected OIDC URL ${href}`);
    };
    gate = await run({MCT_APPROVAL_ORIGIN: approvalOrigin, MCT_APPROVAL_TEST_HOST: '1', MCT_APPROVAL_LISTEN: `127.0.0.1:${gatePort}`,
      MCT_APPROVAL_UPSTREAM: `https://127.0.0.1:${upstreamPort}`, MCT_APPROVAL_UPSTREAM_CA_FILE: cert,
      MCT_APPROVAL_TLS_CERT_FILE: cert, MCT_APPROVAL_TLS_KEY_FILE: key, MCT_APPROVAL_SESSION_SECONDS: '300',
      MCT_OIDC_ISSUER: issuer, MCT_OIDC_CLIENT_ID: clientId, MCT_OIDC_CLIENT_SECRET_FILE: secret,
      MCT_OIDC_ALLOWED_SUBJECTS_FILE: subjects, MCT_OIDC_TOKEN_AUTH_METHOD: 'client_secret_basic',
      MCT_APPROVAL_PROOF_SIGNING_KEY_FILE: proofPrivate, MCT_APPROVAL_WIKI_VERIFY_KEY_FILE: wikiPublic,
      MCT_APPROVAL_WIKI_PROOF_URL: 'https://wiki.nicabar.org/library/_mct-proof'}, {fetcher});
    if (!gate.listening) await once(gate, 'listening');
    const host = `approval.example.test:${gatePort}`;
    const scanned = await httpsCall(gatePort, host, `/a/${capability}`);
    assert.equal(scanned.status, 200); assert.doesNotMatch(scanned.body, /unraid-mother/); assert.match(scanned.body, /A bot is asking for your approval/);
    const linkHeader = namedCookie(scanned.headers, '__Host-mct-link');
    assert.match(linkHeader, /HttpOnly/); assert.match(linkHeader, /SameSite=Lax/);
    const linkCookie = cookiePair(linkHeader);
    const login = await httpsCall(gatePort, host, `/a/${capability}/login`, {cookie: linkCookie});
    const stateCookie = cookiePair(namedCookie(login.headers, '__Host-mct-oidc-state'));
    const authorization = new URL(login.headers.location); callbackNonce = authorization.searchParams.get('nonce');
    const callback = await httpsCall(gatePort, host, `/oidc/callback?code=code-1&state=${encodeURIComponent(authorization.searchParams.get('state'))}`,
      {cookie: `${linkCookie}; ${stateCookie}`});
    assert.equal(callback.status, 302, callback.body); assert.equal(callback.headers.location, `/a/${capability}`); assert.equal(claims, 1);
    const sessionHeader = namedCookie(callback.headers, '__Host-mct-session');
    assert.match(sessionHeader, /HttpOnly/); assert.match(sessionHeader, /SameSite=Lax/);
    const returned = await httpsCall(gatePort, host, `/a/${capability}`, {cookie: `${linkCookie}; ${cookiePair(sessionHeader)}`});
    assert.equal(returned.status, 200); assert.match(returned.body, /You’re signed in/); assert.equal(claims, 1);
  } finally {
    if (gate) { gate.close(); await once(gate, 'close'); }
    if (upstream) { upstream.close(); await once(upstream, 'close'); }
    await rm(directory, {recursive: true, force: true});
  }
});
