// Import on the operator-controlled HTTPS relying-party origin, not an arbitrary
// site. Registration export is pinned separately; a command cannot register itself.
export const encode = bytes => btoa(String.fromCharCode(...new Uint8Array(bytes)))
  .replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
export const decode = value => Uint8Array.from(atob(value.replaceAll('-', '+').replaceAll('_', '/')), c => c.charCodeAt(0));
const hex = bytes => [...bytes].map(b => b.toString(16).padStart(2, '0')).join('');

export async function registerRoot(rpId, userName) {
  if (location.protocol !== 'https:') throw new Error('HTTPS registration required');
  const credential = await navigator.credentials.create({publicKey: {
    challenge: crypto.getRandomValues(new Uint8Array(32)),
    rp: {id: rpId, name: 'MCT administration'},
    user: {id: crypto.getRandomValues(new Uint8Array(32)), name: userName, displayName: userName},
    pubKeyCredParams: [{type: 'public-key', alg: -7}],
    authenticatorSelection: {residentKey: 'required', userVerification: 'required'},
    attestation: 'none', timeout: 300000,
  }});
  if (!credential || credential.response.getPublicKeyAlgorithm() !== -7) throw new Error('ES256 credential required');
  const key = await crypto.subtle.importKey('spki', credential.response.getPublicKey(), {name: 'ECDSA', namedCurve: 'P-256'}, true, ['verify']);
  const jwk = await crypto.subtle.exportKey('jwk', key);
  return {credential_id: encode(credential.rawId), public_key_sec1: hex(new Uint8Array([4, ...decode(jwk.x), ...decode(jwk.y)])), rp_id: rpId, origin: location.origin};
}

// Review prepared.intent before calling. Choose 1Password in the browser prompt.
// This helper never stores the root secret or bypasses user verification.
export function canonical(value) {
  if (typeof value === 'number' && !Number.isSafeInteger(value)) throw new Error('unsafe integer');
  if (Array.isArray(value)) return value.map(canonical);
  if (value !== null && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort().map(k => [k, canonical(value[k])]));
  return value;
}
export async function expectedChallenge(prepared) {
  const message = JSON.stringify(canonical(['mct-passkey-administration/v1', `webauthn:${prepared.root.credential_id}`, prepared.intent]));
  return encode(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(message)));
}
export async function signPrepared(prepared) {
  if (await expectedChallenge(prepared) !== prepared.challenge) throw new Error('challenge does not sign the reviewed intent');
  if (location.protocol !== 'https:' || location.origin !== prepared.root.origin) throw new Error('wrong registered origin');
  const credential = await navigator.credentials.get({publicKey: {
    challenge: decode(prepared.challenge), rpId: prepared.root.rp_id,
    allowCredentials: [{type: 'public-key', id: decode(prepared.root.credential_id)}],
    userVerification: 'required', timeout: 300000,
  }});
  if (!credential || encode(credential.rawId) !== prepared.root.credential_id) throw new Error('wrong registered credential');
  return {intent: prepared.intent, proof: {
    scheme: 'webauthn-p256-day-key/v1', credential_id: encode(credential.rawId),
    authenticator_data: encode(credential.response.authenticatorData),
    client_data_json: encode(credential.response.clientDataJSON),
    signature: encode(credential.response.signature),
  }};
}
