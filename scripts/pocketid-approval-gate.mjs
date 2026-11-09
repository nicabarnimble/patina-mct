#!/usr/bin/env node
/** Pocket ID authenticates page access. Only WebAuthn/Mother grants MCT authority. */
import {createHash, createPublicKey, randomBytes, timingSafeEqual, verify} from 'node:crypto';
import {lstat, readFile} from 'node:fs/promises';
import {request as httpsRequest, createServer} from 'node:https';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {dirname, join} from 'node:path';

const ROOT = dirname(fileURLToPath(import.meta.url));
const LIMIT = 96 * 1024;
const CLOCK_SKEW = 30;
const API_PATHS = new Set(['/api/session', '/api/next', '/api/complete', '/api/fail', '/api/renew', '/api/release']);
const random = () => randomBytes(32).toString('base64url');
const nowSeconds = () => Math.floor(Date.now() / 1000);

function equal(left, right) {
  const a = Buffer.from(String(left));
  const b = Buffer.from(String(right));
  return a.length === b.length && timingSafeEqual(a, b);
}
function jsonPart(value, label) {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) throw new Error(`invalid ${label}`);
  let parsed;
  try { parsed = JSON.parse(Buffer.from(value, 'base64url')); }
  catch { throw new Error(`invalid ${label}`); }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error(`invalid ${label}`);
  return parsed;
}

function validateTokenHeader(header, allowedAlgorithms) {
  if (!['RS256', 'ES256'].includes(header.alg) || !allowedAlgorithms.has(header.alg)) throw new Error('unsupported ID token algorithm');
  if (header.typ !== undefined && header.typ !== 'JWT') throw new Error('invalid ID token type');
  if (typeof header.kid !== 'string' || !header.kid) throw new Error('missing ID token key id');
}
function parseIdToken(token, allowedAlgorithms) {
  if (typeof token !== 'string' || token.length > 16 * 1024) throw new Error('invalid ID token');
  const parts = token.split('.');
  if (parts.length !== 3 || !/^[A-Za-z0-9_-]+$/.test(parts[2])) throw new Error('invalid ID token');
  const header = jsonPart(parts[0], 'ID token header');
  validateTokenHeader(header, allowedAlgorithms);
  return {header, claims: jsonPart(parts[1], 'ID token claims'), input: Buffer.from(`${parts[0]}.${parts[1]}`), signature: Buffer.from(parts[2], 'base64url')};
}
function matchingJwk(header, jwks) {
  const keys = Array.isArray(jwks?.keys) ? jwks.keys.filter(key => key?.kid === header.kid) : [];
  if (keys.length !== 1) throw new Error('ID token key is unavailable or ambiguous');
  return keys[0];
}
function validateJwk(header, jwk) {
  if (jwk.use !== undefined && jwk.use !== 'sig') throw new Error('ID token key is not for signatures');
  if (jwk.alg !== undefined && jwk.alg !== header.alg) throw new Error('ID token key algorithm mismatch');
  const expected = header.alg === 'RS256' ? 'RSA' : 'EC';
  if (jwk.kty !== expected) throw new Error('ID token key type mismatch');
}
function verificationKey(header, jwks) {
  const jwk = matchingJwk(header, jwks);
  validateJwk(header, jwk);
  return createPublicKey({key: jwk, format: 'jwk'});
}
function verifyTokenSignature(parsed, key) {
  const valid = parsed.header.alg === 'ES256'
    ? verify('sha256', parsed.input, {key, dsaEncoding: 'ieee-p1363'}, parsed.signature)
    : verify('RSA-SHA256', parsed.input, key, parsed.signature);
  if (!valid) throw new Error('invalid ID token signature');
}
function verifyAudience(claims, clientId) {
  const audience = typeof claims.aud === 'string' ? [claims.aud] : claims.aud;
  if (!Array.isArray(audience) || !audience.includes(clientId)) throw new Error('wrong ID token audience');
  if (audience.length > 1 && claims.azp !== clientId) throw new Error('wrong ID token authorized party');
}
function verifyTimes(claims, now) {
  if (!Number.isInteger(claims.exp) || claims.exp < now - CLOCK_SKEW) throw new Error('expired ID token');
  if (!Number.isInteger(claims.iat) || claims.iat > now + CLOCK_SKEW) throw new Error('future ID token issue time');
  if (claims.nbf !== undefined && (!Number.isInteger(claims.nbf) || claims.nbf > now + CLOCK_SKEW)) throw new Error('ID token not yet valid');
}
function verifyIdentity(claims, {issuer, nonce, allowedSubjects}) {
  if (claims.iss !== issuer) throw new Error('wrong ID token issuer');
  if (claims.nonce !== nonce) throw new Error('wrong ID token nonce');
  if (typeof claims.sub !== 'string' || !claims.sub || !allowedSubjects.has(claims.sub)) throw new Error('unauthorized ID token subject');
}
export function verifyIdToken(token, {issuer, clientId, nonce, allowedSubjects, jwks, now = nowSeconds(), allowedAlgorithms = new Set(['RS256', 'ES256'])}) {
  const parsed = parseIdToken(token, allowedAlgorithms);
  verifyTokenSignature(parsed, verificationKey(parsed.header, jwks));
  verifyIdentity(parsed.claims, {issuer, nonce, allowedSubjects});
  verifyAudience(parsed.claims, clientId);
  verifyTimes(parsed.claims, now);
  return Object.fromEntries(['iss', 'aud', 'azp', 'sub', 'nonce', 'iat', 'exp', 'nbf']
    .filter(name => parsed.claims[name] !== undefined).map(name => [name, parsed.claims[name]]));
}

export class LoginTransactions {
  constructor({now = nowSeconds, random: randomSource = random, ttl = 300, limit = 64} = {}) {
    this.now = now; this.random = randomSource; this.ttl = ttl; this.limit = limit; this.pending = new Map();
  }
  create() {
    const at = this.now();
    for (const [id, value] of this.pending) if (value.expires < at) this.pending.delete(id);
    if (this.pending.size >= this.limit) throw new Error('too many pending login transactions');
    const value = {id: this.random(), state: this.random(), nonce: this.random(), verifier: this.random(), expires: at + this.ttl};
    this.pending.set(value.id, value);
    return {...value};
  }
  consume(id, state, at = this.now()) {
    const value = this.pending.get(id);
    this.pending.delete(id);
    if (!value || !equal(value.state, state)) throw new Error('invalid or replayed login transaction');
    if (value.expires < at) throw new Error('expired login transaction');
    return {...value};
  }
}

export class ApprovalSessions {
  constructor({now = nowSeconds, random: randomSource = random, ttl = 900, limit = 64} = {}) {
    this.now = now; this.random = randomSource; this.ttl = ttl; this.limit = limit; this.sessions = new Map();
  }
  create(subject) {
    const at = this.now();
    for (const [id, value] of this.sessions) if (value.expires < at) this.sessions.delete(id);
    if (this.sessions.size >= this.limit) throw new Error('too many approval sessions');
    const value = {id: this.random(), subject, csrf: this.random(), expires: at + this.ttl};
    this.sessions.set(value.id, value);
    return {...value};
  }
  get(id, at = this.now()) {
    const value = this.sessions.get(id);
    if (!value || value.expires < at) { this.sessions.delete(id); return null; }
    return {...value};
  }
  delete(id) { this.sessions.delete(id); }
}

function validateBrowserPost({method, path, origin, expectedOrigin, csrf, session}) {
  if (!session) throw new Error('authenticated session required');
  if (method !== 'POST' || !API_PATHS.has(path)) throw new Error('approval route refused');
  if (origin !== expectedOrigin) throw new Error('wrong request origin');
  if (!equal(csrf, session.csrf)) throw new Error('invalid CSRF token');
}
function validateQueueCapability(path, body, queueSession) {
  if (path === '/api/session') {
    if (typeof body?.token !== 'string' || body.token.length < 16) throw new Error('one-use queue capability required');
    return;
  }
  if (!queueSession) throw new Error('independent queue session required');
}
export function validateApprovalRequest(request) {
  validateBrowserPost(request);
  validateQueueCapability(request.path, request.body, request.queueSession);
}

function parseUrl(value, label) {
  try { return new URL(value); } catch { throw new Error(`${label} must be a valid URL`); }
}
function strictHttps(value, label, {originOnly = false} = {}) {
  const url = parseUrl(value, label);
  const validAuthority = url.protocol === 'https:' && !url.username && !url.password && !url.hash;
  const validShape = !originOnly || (url.pathname === '/' && !url.search);
  if (!validAuthority || !validShape) throw new Error(`${label} must be an HTTPS ${originOnly ? 'origin' : 'URL'}`);
  return url;
}
function parseListen(value) {
  const url = new URL(`https://${value}`);
  if (!url.hostname || !url.port || url.username || url.password || url.pathname !== '/' || url.search || url.hash) {
    throw new Error('MCT_APPROVAL_LISTEN must be host:port');
  }
  return {host: url.hostname, port: Number(url.port)};
}
async function privateText(path, label) {
  const info = await lstat(path, {bigint: false});
  if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1 || (info.mode & 0o077) !== 0 || ![0, process.getuid?.()].includes(info.uid)) {
    throw new Error(`${label} must be a private regular single-link file`);
  }
  return (await readFile(path, 'utf8')).trim();
}
function cookies(header = '') {
  return Object.fromEntries(header.split(';').map(part => part.trim().split('='))
    .filter(parts => parts.length === 2 && parts[0]).map(([name, value]) => [name, value]));
}
function cookie(name, value, maxAge) {
  return `${name}=${value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=${maxAge}`;
}
function clearCookie(name) { return cookie(name, '', 0); }
function securityHeaders(response) {
  response.setHeader('Cache-Control', 'no-store');
  response.setHeader('X-Content-Type-Options', 'nosniff');
  response.setHeader('Referrer-Policy', 'no-referrer');
  response.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  response.setHeader('Content-Security-Policy', "default-src 'none'; script-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'");
}
function reply(response, status, body, contentType = 'application/json; charset=utf-8', headers = {}) {
  const data = Buffer.isBuffer(body) ? body : Buffer.from(typeof body === 'string' ? body : JSON.stringify(body));
  response.writeHead(status, {'Content-Type': contentType, 'Content-Length': data.length, ...headers});
  response.end(data);
}
async function body(request) {
  const declared = Number(request.headers['content-length']);
  if (!Number.isInteger(declared) || declared < 2 || declared > LIMIT || request.headers['transfer-encoding']) {
    throw new Error('invalid request body size');
  }
  const chunks = []; let size = 0;
  for await (const chunk of request) { size += chunk.length; if (size > LIMIT) throw new Error('request too large'); chunks.push(chunk); }
  if (size !== declared) throw new Error('incomplete request body');
  const raw = Buffer.concat(chunks);
  let parsed;
  try { parsed = JSON.parse(raw); } catch { throw new Error('invalid JSON'); }
  return {raw, parsed};
}

class Provider {
  constructor(config, fetcher = globalThis.fetch) {
    this.config = config; this.fetcher = fetcher; this.discoveryCache = null; this.jwksCache = null;
  }
  async fetchJson(url, options = {}) {
    const response = await this.fetcher(url, {...options, signal: AbortSignal.timeout(5000)});
    if (!response.ok) throw new Error(`OIDC endpoint returned ${response.status}`);
    const text = await response.text();
    if (text.length > LIMIT) throw new Error('OIDC response too large');
    return JSON.parse(text);
  }
  async discovery() {
    if (this.discoveryCache && this.discoveryCache.expires > Date.now()) return this.discoveryCache.value;
    const url = new URL(this.config.issuer.href.replace(/\/$/, '') + '/.well-known/openid-configuration');
    const value = await this.fetchJson(url);
    if (value.issuer !== this.config.issuer.href.replace(/\/$/, '')) throw new Error('OIDC discovery issuer mismatch');
    for (const name of ['authorization_endpoint', 'token_endpoint', 'jwks_uri']) {
      const endpoint = strictHttps(value[name], `OIDC ${name}`);
      if (endpoint.origin !== this.config.issuer.origin) throw new Error(`OIDC ${name} must share the issuer origin`);
    }
    if (!Array.isArray(value.id_token_signing_alg_values_supported) ||
        !value.id_token_signing_alg_values_supported.some(alg => ['RS256', 'ES256'].includes(alg))) {
      throw new Error('OIDC provider advertises no supported ID-token algorithm');
    }
    this.discoveryCache = {value, expires: Date.now() + 300_000};
    return value;
  }
  async jwks(metadata) {
    if (this.jwksCache && this.jwksCache.expires > Date.now()) return this.jwksCache.value;
    const value = await this.fetchJson(metadata.jwks_uri);
    if (!Array.isArray(value.keys)) throw new Error('invalid OIDC JWKS');
    this.jwksCache = {value, expires: Date.now() + 300_000};
    return value;
  }
  async exchange(code, verifier) {
    const metadata = await this.discovery();
    const form = new URLSearchParams({grant_type: 'authorization_code', code, redirect_uri: this.config.redirectUri, client_id: this.config.clientId, code_verifier: verifier});
    const headers = {'Content-Type': 'application/x-www-form-urlencoded'};
    if (this.config.tokenAuthMethod === 'client_secret_basic') {
      headers.Authorization = `Basic ${Buffer.from(`${this.config.clientId}:${this.config.clientSecret}`).toString('base64')}`;
    } else form.set('client_secret', this.config.clientSecret);
    const result = await this.fetchJson(metadata.token_endpoint, {method: 'POST', headers, body: form});
    if (typeof result.id_token !== 'string') throw new Error('OIDC token response has no ID token');
    return {idToken: result.id_token, metadata};
  }
}

async function proxy(config, request, response, raw) {
  const queueCookie = cookies(request.headers.cookie)['__Host-mct'];
  const target = config.upstream;
  await new Promise((resolve, reject) => {
    const upstream = httpsRequest({
      hostname: target.hostname, port: target.port || 443, path: request.url, method: 'POST',
      servername: config.origin.hostname, ca: config.upstreamCa, rejectUnauthorized: true,
      timeout: 5000,
      headers: {
        Host: config.origin.host, Origin: config.origin.origin,
        'Content-Type': 'application/json', 'Content-Length': raw.length,
        ...(queueCookie ? {Cookie: `__Host-mct=${queueCookie}`} : {}),
        ...(request.headers['x-mct-page'] ? {'X-MCT-Page': request.headers['x-mct-page']} : {}),
      },
    }, upstreamResponse => {
      const chunks = []; let size = 0;
      upstreamResponse.on('data', chunk => { size += chunk.length; if (size > LIMIT) upstreamResponse.destroy(new Error('upstream response too large')); else chunks.push(chunk); });
      upstreamResponse.on('end', () => {
        const headers = {};
        const setCookie = upstreamResponse.headers['set-cookie']?.find(value => value.startsWith('__Host-mct='));
        if (setCookie) headers['Set-Cookie'] = setCookie;
        reply(response, upstreamResponse.statusCode || 502, Buffer.concat(chunks), 'application/json; charset=utf-8', headers);
        resolve();
      });
    });
    upstream.on('timeout', () => upstream.destroy(new Error('upstream timeout')));
    upstream.on('error', reject);
    upstream.end(raw);
  });
}

function required(env, name) {
  if (!env[name]) throw new Error(`${name} is required`);
  return env[name];
}
function loopbackUpstream(value) {
  const upstream = strictHttps(value, 'MCT_APPROVAL_UPSTREAM', {originOnly: true});
  if (!['127.0.0.1', '::1', 'localhost'].includes(upstream.hostname)) throw new Error('approval queue upstream must be loopback');
  return upstream;
}
function tokenAuthMethod(value = 'client_secret_basic') {
  if (!['client_secret_basic', 'client_secret_post'].includes(value)) throw new Error('unsupported token auth method');
  return value;
}
function sessionLifetime(value = '900') {
  const seconds = Number(value);
  if (!Number.isInteger(seconds) || seconds < 60 || seconds > 900) throw new Error('session lifetime must be 60..900 seconds');
  return seconds;
}
async function subjectAllowlist(path) {
  const allowed = JSON.parse(await privateText(path, 'OIDC subject allowlist'));
  if (!Array.isArray(allowed) || !allowed.length || allowed.some(value => typeof value !== 'string' || !value)) {
    throw new Error('OIDC subject allowlist must be a nonempty JSON string array');
  }
  return new Set(allowed);
}
async function configuration(env) {
  const origin = strictHttps(required(env, 'MCT_APPROVAL_ORIGIN'), 'MCT_APPROVAL_ORIGIN', {originOnly: true});
  return {
    origin,
    issuer: strictHttps(required(env, 'MCT_OIDC_ISSUER'), 'MCT_OIDC_ISSUER', {originOnly: true}),
    upstream: loopbackUpstream(required(env, 'MCT_APPROVAL_UPSTREAM')),
    listen: parseListen(env.MCT_APPROVAL_LISTEN || '127.0.0.1:8443'),
    clientId: required(env, 'MCT_OIDC_CLIENT_ID'),
    tokenAuthMethod: tokenAuthMethod(env.MCT_OIDC_TOKEN_AUTH_METHOD),
    sessionSeconds: sessionLifetime(env.MCT_APPROVAL_SESSION_SECONDS),
    allowedSubjects: await subjectAllowlist(required(env, 'MCT_OIDC_ALLOWED_SUBJECTS_FILE')),
    clientSecret: await privateText(required(env, 'MCT_OIDC_CLIENT_SECRET_FILE'), 'OIDC client secret'),
    tlsCert: await readFile(required(env, 'MCT_APPROVAL_TLS_CERT_FILE')),
    tlsKey: await privateText(required(env, 'MCT_APPROVAL_TLS_KEY_FILE'), 'TLS private key'),
    upstreamCa: await readFile(required(env, 'MCT_APPROVAL_UPSTREAM_CA_FILE')),
    redirectUri: `${origin.origin}/oidc/callback`,
  };
}
async function loadAssets() {
  const specs = [['/', 'text/html; charset=utf-8', 'passkey-approval.html'],
    ['/approve/', 'text/html; charset=utf-8', 'passkey-approval.html'],
    ['/approval.mjs', 'text/javascript; charset=utf-8', 'passkey-approval.mjs'],
    ['/passkey-admin.mjs', 'text/javascript; charset=utf-8', 'passkey-admin.mjs']];
  return new Map(await Promise.all(specs.map(async ([route, type, name]) => [route, [type, await readFile(join(ROOT, name))]])));
}
function authorizationUrl(metadata, config, pending) {
  const authorization = new URL(metadata.authorization_endpoint);
  const values = {response_type: 'code', client_id: config.clientId, redirect_uri: config.redirectUri,
    scope: 'openid', state: pending.state, nonce: pending.nonce,
    code_challenge: createHash('sha256').update(pending.verifier).digest('base64url'), code_challenge_method: 'S256'};
  for (const [name, value] of Object.entries(values)) authorization.searchParams.set(name, value);
  return authorization;
}
async function handleLogin(context, response) {
  const pending = context.transactions.create();
  const target = authorizationUrl(await context.provider.discovery(), context.config, pending);
  return reply(response, 302, '', 'text/plain', {'Location': target.href, 'Set-Cookie': cookie('__Host-mct-oidc-state', pending.id, 300)});
}
async function handleCallback(context, response, url, jar) {
  const pending = context.transactions.consume(jar['__Host-mct-oidc-state'], url.searchParams.get('state'));
  const code = url.searchParams.get('code');
  if (!code) throw new Error('missing authorization code');
  const exchanged = await context.provider.exchange(code, pending.verifier);
  const algorithms = new Set(exchanged.metadata.id_token_signing_alg_values_supported.filter(alg => ['RS256', 'ES256'].includes(alg)));
  const claims = verifyIdToken(exchanged.idToken, {issuer: context.config.issuer.href.replace(/\/$/, ''),
    clientId: context.config.clientId, nonce: pending.nonce, allowedSubjects: context.config.allowedSubjects,
    jwks: await context.provider.jwks(exchanged.metadata), allowedAlgorithms: algorithms});
  const created = context.sessions.create(claims.sub);
  return reply(response, 302, '', 'text/plain', {'Location': '/', 'Set-Cookie': [clearCookie('__Host-mct-oidc-state'), cookie('__Host-mct-oidc-session', created.id, context.config.sessionSeconds)]});
}
async function handleAuthenticated(context, request, response, url, jar, session) {
  if (request.method === 'GET' && url.pathname === '/api/auth') return reply(response, 200, {subject: session.subject, csrf: session.csrf});
  if (request.method === 'GET' && context.assets.has(url.pathname)) {
    const [type, data] = context.assets.get(url.pathname); return reply(response, 200, data, type);
  }
  if (request.method !== 'POST' || !API_PATHS.has(url.pathname)) return reply(response, 404, {error: 'not found'});
  if (request.headers['content-type'] !== 'application/json') throw new Error('JSON required');
  const incoming = await body(request);
  validateApprovalRequest({method: request.method, path: url.pathname, origin: request.headers.origin,
    expectedOrigin: context.config.origin.origin, csrf: request.headers['x-mct-csrf'], session,
    queueSession: Boolean(jar['__Host-mct']), body: incoming.parsed});
  return proxy(context.config, request, response, incoming.raw);
}
async function handlePublicRoute(context, request, response, url, jar) {
  if (request.method !== 'GET') return false;
  if (url.pathname === '/login') { await handleLogin(context, response); return true; }
  if (url.pathname === '/oidc/callback') { await handleCallback(context, response, url, jar); return true; }
  if (url.pathname !== '/logout') return false;
  context.sessions.delete(jar['__Host-mct-oidc-session']);
  reply(response, 302, '', 'text/plain', {'Location': '/login', 'Set-Cookie': clearCookie('__Host-mct-oidc-session')});
  return true;
}
async function dispatchRequest(context, request, response) {
  if (request.headers.host !== context.config.origin.host) return reply(response, 421, {error: 'wrong host'});
  const url = new URL(request.url, context.config.origin);
  const jar = cookies(request.headers.cookie);
  if (await handlePublicRoute(context, request, response, url, jar)) return;
  const session = context.sessions.get(jar['__Host-mct-oidc-session']);
  if (!session) return reply(response, 302, '', 'text/plain', {'Location': '/login'});
  return handleAuthenticated(context, request, response, url, jar, session);
}
function requestHandler(context) {
  return async (request, response) => {
    securityHeaders(response);
    try { return await dispatchRequest(context, request, response); }
    catch (error) {
      console.error(`approval gate refused request: ${error.message}`);
      const headers = request.url?.startsWith('/oidc/callback') ? {'Set-Cookie': clearCookie('__Host-mct-oidc-state')} : {};
      return reply(response, 400, {error: 'request refused'}, 'application/json; charset=utf-8', headers);
    }
  };
}
function boundServer(server) {
  server.requestTimeout = 5000; server.headersTimeout = 5000; server.keepAliveTimeout = 1000;
  server.maxHeadersCount = 32; server.maxConnections = 16;
  return server;
}
export async function run(env = process.env, {fetcher = globalThis.fetch} = {}) {
  const config = await configuration(env);
  const context = {config, provider: new Provider(config, fetcher), transactions: new LoginTransactions(),
    sessions: new ApprovalSessions({ttl: config.sessionSeconds}), assets: await loadAssets()};
  const server = boundServer(createServer({cert: config.tlsCert, key: config.tlsKey, minVersion: 'TLSv1.2'}, requestHandler(context)));
  server.listen(config.listen.port, config.listen.host, () => console.log(`Pocket ID approval gate listening at ${config.origin.origin}`));
  return server;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  run().catch(error => { console.error(`Pocket ID approval gate failed: ${error.message}`); process.exitCode = 1; });
}
