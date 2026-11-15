import {sign, verify} from 'node:crypto';

const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', {fatal: true});

export function b64url(value) {
  return Buffer.from(value).toString('base64url');
}

export function canonicalJson(value) {
  if (value === null || typeof value === 'boolean' || typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'number' && Number.isSafeInteger(value)) return String(value);
  if (Array.isArray(value)) return '[' + value.map(canonicalJson).join(',') + ']';
  if (value && typeof value === 'object' && Object.getPrototypeOf(value) === Object.prototype) {
    return '{' + Object.keys(value).sort().map(key => JSON.stringify(key) + ':' + canonicalJson(value[key])).join(',') + '}';
  }
  throw new TypeError('proof value is not canonical JSON');
}

function exactKeys(value, keys) {
  return value && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).sort().join('\0') === [...keys].sort().join('\0');
}

export function signEnvelope(payload, privateKey) {
  const bytes = encoder.encode(canonicalJson(payload));
  return b64url(bytes) + '.' + b64url(sign(null, bytes, privateKey));
}

export function verifyEnvelope(token, publicKey) {
  if (typeof token !== 'string' || token.length > 8192 || !/^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.test(token)) {
    throw new Error('invalid envelope');
  }
  const [body, signature] = token.split('.');
  const bytes = Buffer.from(body, 'base64url');
  if (b64url(bytes) !== body || !verify(null, bytes, publicKey, Buffer.from(signature, 'base64url'))) {
    throw new Error('bad signature');
  }
  const payload = JSON.parse(decoder.decode(bytes));
  if (canonicalJson(payload) !== decoder.decode(bytes)) throw new Error('noncanonical envelope');
  return payload;
}

export function makeProofTicket(request, privateKey, now) {
  if (!exactKeys(request, ['id', 'nonce', 'path', 'expected_subject', 'required_group', 'return_url'])) {
    throw new TypeError('invalid proof request');
  }
  const payload = {
    kind: 'mct-access-proof-ticket/v1',
    request_id: request.id,
    path: request.path,
    nonce: request.nonce,
    expected_subject: request.expected_subject,
    required_group: request.required_group,
    return_url: request.return_url,
    iat: now,
    exp: now + 120,
  };
  return {payload, token: signEnvelope(payload, privateKey)};
}

export class ProofResultVerifier {
  constructor(request, publicKey) {
    this.request = request;
    this.publicKey = publicKey;
    this.consumed = false;
  }

  accept(token, now) {
    if (this.consumed) return {phase: 'refused', reason: 'replayed'};
    this.consumed = true;
    let payload;
    try {
      payload = verifyEnvelope(token, this.publicKey);
    } catch {
      return {phase: 'refused', reason: 'bad_signature'};
    }
    if (!exactKeys(payload, ['kind', 'request_id', 'nonce', 'path', 'subject', 'outcome', 'reason', 'iat', 'exp'])
        || payload.kind !== 'mct-access-proof-result/v1'
        || payload.request_id !== this.request.id
        || payload.nonce !== this.request.nonce
        || payload.path !== this.request.path
        || payload.subject !== this.request.expected_subject) {
      return {phase: 'refused', reason: 'mismatch'};
    }
    if (!Number.isSafeInteger(payload.iat) || !Number.isSafeInteger(payload.exp)
        || payload.iat > now || now >= payload.exp || payload.exp - payload.iat > 60) {
      return {phase: 'refused', reason: 'expired'};
    }
    if (payload.outcome === 'allowed' && payload.reason === 'ok') {
      return {phase: 'passed', outcome: 'pass', reason: 'ok'};
    }
    if (payload.outcome === 'denied' && payload.reason === 'denied') {
      return {phase: 'failed', outcome: 'fail', reason: 'denied'};
    }
    if (payload.outcome === 'not_found' && payload.reason === 'not_found') {
      return {phase: 'failed', outcome: 'fail', reason: 'not_found'};
    }
    return {phase: 'refused', reason: 'mismatch'};
  }
}

export function terminalOutcome(kind, outcome, fields = {}) {
  if (kind === 'grant') {
    if (outcome === 'approved') return {line: `✓ Approved ${fields.time} · ${fields.mother} until ${fields.grantEnd}`, code: 0};
    if (outcome === 'declined') return {line: `– Declined ${fields.time} · nothing changed`, code: 10};
    if (outcome === 'expired') return {line: `! Code expired ${fields.time} · run the command again for a new code`, code: 11};
    if (outcome === 'refused') return {line: "× Refused · the approval didn't match this request · nothing changed", code: 12};
  }
  if (kind === 'proof') {
    if (outcome === 'passed') return {line: `✓ Passed ${fields.time} · ${fields.subject} can open it`, code: 0};
    if (outcome === 'denied') return {line: `× Failed ${fields.time} · not allowed for ${fields.subject} · new code to retry`, code: 20};
    if (outcome === 'not_found') return {line: `× Failed ${fields.time} · file not found on wiki.nicabar.org`, code: 21};
    if (outcome === 'skipped') return {line: `– Skipped ${fields.time} · nothing was checked`, code: 10};
    if (outcome === 'expired') return {line: `! Code expired ${fields.time} · run the command again for a new code`, code: 11};
    if (outcome === 'refused') return {line: "× Refused · the answer didn't match this check", code: 12};
  }
  throw new TypeError('unknown terminal outcome');
}
