import assert from 'node:assert/strict';
import {createHash, generateKeyPairSync} from 'node:crypto';
import {readFile} from 'node:fs/promises';
import test from 'node:test';

import {
  ProofResultVerifier,
  makeProofTicket,
  signEnvelope,
  terminalOutcome,
} from './mct-approval-protocol.mjs';

test('authoritative packet CSS and Datastar assets remain byte-for-byte pinned', async () => {
  for (const [path, expected] of [
    ['scripts/mct-approval/tokens.css', '8e55ee7affe40d75591bf03724396e1ab31852a85a7c7f448aaefbec5907fbbd'],
    ['scripts/mct-approval/approval.css', '11f341415db53670a0e9e369ce2c13554abe333d21d97b56576f8a73d89e6db5'],
    ['scripts/mct-approval/datastar.js', '727844adfc825ee651fb93c544a2a739986f9a21820a94524b35f0cac470cf91'],
  ]) assert.equal(createHash('sha256').update(await readFile(path)).digest('hex'), expected, path);
});

const approvalKeys = generateKeyPairSync('ed25519');
const wikiKeys = generateKeyPairSync('ed25519');

const request = {
  id: 'request-1',
  nonce: 'n'.repeat(43),
  path: '/library/example-handbook.pdf',
  expected_subject: 'nicabar',
  required_group: 'wiki-admin',
  return_url: 'https://mct-approval.nicabar.org/a/example/proof/result',
};

function result(overrides = {}) {
  return {
    kind: 'mct-access-proof-result/v1', request_id: request.id,
    nonce: request.nonce, path: request.path, subject: 'nicabar',
    outcome: 'allowed', reason: 'ok', iat: 1_000, exp: 1_060,
    ...overrides,
  };
}

test('proof ticket specifies the Grok Life wiki handler contract', () => {
  const ticket = makeProofTicket(request, approvalKeys.privateKey, 1_000);
  assert.equal(ticket.payload.kind, 'mct-access-proof-ticket/v1');
  assert.equal(ticket.payload.expected_subject, 'nicabar');
  assert.equal(ticket.payload.required_group, 'wiki-admin');
  assert.equal(ticket.payload.path, request.path);
  assert.equal(ticket.payload.return_url, request.return_url);
  assert.equal(ticket.payload.exp, 1_120);
  assert.match(ticket.token, /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/);
});

test('failed proof consumes the code and cannot be replayed', () => {
  const verifier = new ProofResultVerifier(request, wikiKeys.publicKey);
  const denied = signEnvelope(result({outcome: 'denied', reason: 'denied'}), wikiKeys.privateKey);
  assert.deepEqual(verifier.accept(denied, 1_010), {phase: 'failed', outcome: 'fail', reason: 'denied'});
  assert.equal(verifier.consumed, true);
  assert.deepEqual(verifier.accept(denied, 1_011), {phase: 'refused', reason: 'replayed'});
});

test('forged proof is refused and consumes the code', () => {
  const verifier = new ProofResultVerifier(request, wikiKeys.publicKey);
  const forged = signEnvelope(result(), approvalKeys.privateKey);
  assert.deepEqual(verifier.accept(forged, 1_010), {phase: 'refused', reason: 'bad_signature'});
  assert.equal(verifier.consumed, true);
});

test('expired proof is refused and consumes the code', () => {
  const verifier = new ProofResultVerifier(request, wikiKeys.publicKey);
  const expired = signEnvelope(result({iat: 900, exp: 960}), wikiKeys.privateKey);
  assert.deepEqual(verifier.accept(expired, 1_010), {phase: 'refused', reason: 'expired'});
  assert.equal(verifier.consumed, true);
});

test('valid proof is accepted once and replay is refused', () => {
  const verifier = new ProofResultVerifier(request, wikiKeys.publicKey);
  const allowed = signEnvelope(result(), wikiKeys.privateKey);
  assert.deepEqual(verifier.accept(allowed, 1_010), {phase: 'passed', outcome: 'pass', reason: 'ok'});
  assert.deepEqual(verifier.accept(allowed, 1_011), {phase: 'refused', reason: 'replayed'});
});

test('terminal output and exit codes match the packet', () => {
  assert.deepEqual(terminalOutcome('grant', 'approved', {time: '6:53 AM ET', mother: 'unraid-mother', grantEnd: 'Tue Nov 10, 6:00 AM ET'}),
    {line: '✓ Approved 6:53 AM ET · unraid-mother until Tue Nov 10, 6:00 AM ET', code: 0});
  assert.equal(terminalOutcome('grant', 'declined', {time: '6:53 AM ET'}).code, 10);
  assert.equal(terminalOutcome('grant', 'expired', {time: '6:58 AM ET'}).code, 11);
  assert.equal(terminalOutcome('grant', 'refused', {}).code, 12);
  assert.equal(terminalOutcome('proof', 'passed', {time: '6:52 AM ET', subject: 'nicabar'}).code, 0);
  assert.equal(terminalOutcome('proof', 'denied', {time: '6:52 AM ET', subject: 'nicabar'}).code, 20);
  assert.equal(terminalOutcome('proof', 'not_found', {time: '6:52 AM ET'}).code, 21);
  assert.equal(terminalOutcome('proof', 'skipped', {time: '6:52 AM ET'}).code, 10);
});
