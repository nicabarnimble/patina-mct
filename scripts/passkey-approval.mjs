import {registerRoot, signPrepared, expectedChallenge} from './passkey-admin.mjs';
const state = document.getElementById('state');
const review = document.getElementById('review');
const approve = document.getElementById('approve');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function api(operation, body = {}) {
  const response = await fetch('/api/' + operation, {
    method: 'POST', headers: {'Content-Type': 'application/json'},
    body: JSON.stringify(body), credentials: 'same-origin', cache: 'no-store',
    signal: AbortSignal.timeout(5000),
  });
  if (!response.ok) throw new Error('Local broker refused the request. Inspect the waiting CLI.');
  return response.json();
}
async function assertion(job) {
  if (job.kind === 'registration') return registerRoot(location.hostname, job.name);
  const remaining = Date.parse(job.prepared.intent.expires_at) - Date.now();
  if (!(remaining > 0 && remaining <= 300000)) throw new Error('Intent expired or has an invalid window. Run a fresh CLI command.');
  return signPrepared(job.prepared, {signal: AbortSignal.timeout(remaining)});
}
async function ceremony(job) {
  if (job.kind === 'assertion') {
    if (job.prepared.root.origin !== location.origin ||
        await expectedChallenge(job.prepared) !== job.prepared.challenge) throw new Error('Prepared intent binding mismatch');
    review.textContent = JSON.stringify(job.prepared.intent, null, 2);
  } else {
    review.textContent = 'Register a new root for ' + job.name + ' at ' + location.origin + '. Choose 1Password.';
  }
  state.textContent = 'Review the operation below. Waiting for the passkey provider.';
  await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  let result;
  try {
    result = await assertion(job);
  } catch (error) {
    if (error.name !== 'NotAllowedError') throw error;
    state.textContent = 'The browser or provider stopped automatic approval. Approve once to retry, or let this attempt expire.';
    result = await new Promise((resolve, reject) => {
      const remaining = job.kind === 'assertion' ? Date.parse(job.prepared.intent.expires_at) - Date.now() : 300000;
      const timer = setTimeout(() => { approve.hidden = true; reject(new Error('Approval expired. Run a fresh CLI command.')); }, Math.max(0, remaining));
      approve.hidden = false;
      approve.onclick = () => {
        approve.hidden = true;
        clearTimeout(timer);
        assertion(job).then(resolve, reject);
      };
    });
  }
  await api('complete', {id: job.id, result});
  state.textContent = 'Assertion returned to the CLI. This page did not apply an action.';
}
async function run() {
  const token = location.hash.slice(1);
  history.replaceState(null, '', '/');
  if (token) await api('session', {token});
  state.textContent = 'Ready. Keep this page visible and focused. The CLI prepares only when this page claims work.';
  for (;;) {
    if (document.visibilityState === 'visible' && document.hasFocus()) {
      const job = await api('next');
      if (job && job.phase === 'offered') {
        try { await ceremony(job); }
        catch (error) {
          state.textContent = String(error) + '. Nothing is retried automatically.';
          await api('fail', {id: job.id});
        }
      }
    }
    await sleep(250);
  }
}
run().catch(error => { state.textContent = String(error) + ' Reopen using the CLI; do not resubmit uncertain work.'; });
