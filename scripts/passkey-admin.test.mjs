import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {encode, decode, canonical, expectedChallenge, signPrepared, registerRoot} from './passkey-admin.mjs';

const fixture = JSON.parse(await readFile(new URL('./fixtures/passkey-admin-challenge.json', import.meta.url)));
test('browser challenge matches the Rust transcript fixture and rejects rounding', async () => {
  assert.equal(await expectedChallenge(fixture), fixture.challenge);
  assert.throws(() => canonical({generation: 9007199254740992}), /unsafe integer/);
  assert.deepEqual([...decode(encode(new Uint8Array([0, 127, 128, 255])))], [0, 127, 128, 255]);
});
test('assertion binds reviewed intent, registered origin, credential and required UV', async () => {
  globalThis.location = {protocol:'https:', origin:fixture.root.origin};
  let calls = 0;
  Object.defineProperty(globalThis, 'navigator', {configurable:true, value:{credentials:{get:async options => {
    calls++;
    assert.equal(options.publicKey.userVerification, 'required');
    assert.equal(options.publicKey.rpId, fixture.root.rp_id);
    assert.equal(encode(options.publicKey.challenge), fixture.challenge);
    return {rawId:decode(fixture.root.credential_id), response:{authenticatorData:new Uint8Array(37), clientDataJSON:new TextEncoder().encode('{}'), signature:new Uint8Array([1, 2])}};
  }}}});
  const signed = await signPrepared(fixture);
  assert.deepEqual(signed.intent, fixture.intent);
  assert.equal(signed.proof.scheme, 'webauthn-p256-day-key/v1');
  assert.equal(calls, 1);
  const changed = structuredClone(fixture); changed.intent.action.body = 'substituted';
  await assert.rejects(signPrepared(changed), /reviewed intent/);
  location.origin = 'https://evil.example.test';
  await assert.rejects(signPrepared(fixture), /wrong registered origin/);
  assert.equal(calls, 1);
});
test('registration exports only an ES256 public root after a required-UV ceremony', async () => {
  globalThis.location = {protocol:'https:', origin:fixture.root.origin};
  const keys = await crypto.subtle.generateKey({name:'ECDSA', namedCurve:'P-256'}, true, ['sign', 'verify']);
  const spki = await crypto.subtle.exportKey('spki', keys.publicKey);
  Object.defineProperty(globalThis, 'navigator', {configurable:true, value:{credentials:{create:async options => {
    assert.equal(options.publicKey.authenticatorSelection.userVerification, 'required');
    assert.equal(options.publicKey.pubKeyCredParams[0].alg, -7);
    assert.equal(options.publicKey.challenge.length, 32);
    return {rawId:decode(fixture.root.credential_id), response:{getPublicKeyAlgorithm:() => -7, getPublicKey:() => spki}};
  }}}});
  const root = await registerRoot(fixture.root.rp_id, 'operator');
  assert.equal(root.origin, fixture.root.origin);
  assert.match(root.public_key_sec1, /^04[0-9a-f]{128}$/);
  assert.deepEqual(Object.keys(root).sort(), ['credential_id', 'origin', 'public_key_sec1', 'rp_id']);
});
