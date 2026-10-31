import {registerRoot, signPrepared, expectedChallenge} from './passkey-admin.mjs';
const state = document.getElementById('state');
const review = document.getElementById('review');
const approve = document.getElementById('approve');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
let pageId = '';
async function api(operation, body = {}, keepalive = false) {
  const response = await fetch('/api/' + operation, {
    method: 'POST', headers: {'Content-Type': 'application/json', 'X-MCT-Page': pageId},
    body: JSON.stringify(body), credentials: 'same-origin', cache: 'no-store', keepalive,
    signal: AbortSignal.timeout(5000),
  });
  if (!response.ok) throw new Error('The broker refused this page or offer. Inspect the waiting CLI.');
  return response.json();
}
function nextPaint(signal) {
  return new Promise((resolve, reject) => {
    let frame;
    const cancel = () => { cancelAnimationFrame(frame); reject(signal.reason); };
    signal.addEventListener('abort', cancel, {once: true});
    frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => { signal.removeEventListener('abort', cancel); resolve(); });
    });
    if (signal.aborted) cancel();
  });
}
function approveOnce(signal) {
  return new Promise((resolve, reject) => {
    const cleanup = () => {
      approve.hidden = true; approve.onclick = null;
      signal.removeEventListener('abort', cancel);
    };
    const cancel = () => { cleanup(); reject(signal.reason); };
    signal.addEventListener('abort', cancel, {once: true});
    approve.hidden = false;
    approve.onclick = () => { cleanup(); resolve(); };
    if (signal.aborted) cancel();
  });
}
async function ceremony(job) {
  const binding = {id: job.id, offer: job.offer};
  const remaining = job.kind === 'registration' ? 300000 : Date.parse(job.prepared.intent.expires_at) - Date.now();
  if (!(remaining > 0 && remaining <= 300000)) {
    await api('fail', binding);
    state.textContent = 'Intent expired. Mother would refuse it. Run a fresh CLI command.';
    return false;
  }
  if (job.kind === 'assertion') {
    if (job.prepared.root.origin !== location.origin ||
        await expectedChallenge(job.prepared) !== job.prepared.challenge) {
      await api('fail', binding);
      throw new Error('Prepared intent binding mismatch');
    }
    review.textContent = JSON.stringify(job.prepared.intent, null, 2);
  } else {
    review.textContent = 'Register a new root for ' + job.name + ' at ' + location.origin + '. Use your default browser with 1Password.';
  }
  const lease = new AbortController();
  const signal = AbortSignal.any([lease.signal, AbortSignal.timeout(remaining)]);
  let deadline, finished = false, submitted = false;
  const abort = () => lease.abort(new Error('Page left or offer ownership ended'));
  const hidden = () => { if (document.visibilityState !== 'visible') abort(); };
  const arm = (milliseconds, started) => {
    clearTimeout(deadline);
    // Abort before the broker's deadline, leaving time for the five-second
    // request timeout and one missed heartbeat. Suspended pages remain fenced.
    deadline = setTimeout(abort, Math.max(0, milliseconds - 10000 - (performance.now() - started)));
  };
  arm(job.lease_ms, performance.now());
  const heartbeat = setInterval(async () => {
    if (finished || signal.aborted) return;
    const started = performance.now();
    try {
      const renewal = await api('renew', binding);
      if (!finished && !signal.aborted) arm(renewal.lease_ms, started);
    } catch { abort(); }
  }, 5000);
  window.addEventListener('pagehide', abort);
  document.addEventListener('visibilitychange', hidden);
  const assertion = () => job.kind === 'registration'
    ? registerRoot(location.hostname, job.name, {signal}) : signPrepared(job.prepared, {signal});
  try {
    hidden();
    state.textContent = 'Review the operation below. Waiting for the passkey provider.';
    await nextPaint(signal);
    let result;
    try { result = await assertion(); }
    catch (error) {
      if (signal.aborted || error.name !== 'NotAllowedError') throw error;
      state.textContent = 'The provider stopped automatic approval. Approve once to retry, or reopen the page later.';
      await approveOnce(signal);
      result = await assertion();
    }
    signal.throwIfAborted();
    submitted = true;
    await api('complete', {...binding, result});
    state.textContent = 'Assertion returned to the CLI. This page did not apply an action.';
    return true;
  } catch (error) {
    if (!submitted && job.kind === 'assertion' && Date.parse(job.prepared.intent.expires_at) <= Date.now()) {
      await api('fail', binding).catch(() => {});
      state.textContent = 'Intent expired. Run a fresh CLI command; the old assertion cannot extend its window.';
      return false;
    }
    state.textContent = submitted
      ? 'Delivery is uncertain. Inspect the CLI and canonical state before any retry.'
      : 'Approval stopped. Open a new authenticated page to reclaim unsigned work. ' + String(error);
    return false;
  } finally {
    finished = true;
    clearInterval(heartbeat); clearTimeout(deadline);
    window.removeEventListener('pagehide', abort);
    document.removeEventListener('visibilitychange', hidden);
    approve.hidden = true;
    // This acknowledgement follows settlement of the WebAuthn promise. A lost
    // response or unloaded page falls back to lease expiry, never stale completion.
    await api('release', binding, true).catch(() => {});
  }
}
async function run() {
  const token = location.hash.slice(1);
  history.replaceState(null, '', '/');
  if (!token) throw new Error('Open a fresh authenticated page through the CLI');
  pageId = (await api('session', {token})).page_id;
  if (typeof pageId !== 'string' || !pageId) {
    throw new Error('This page requires an updated broker. Finish the existing run before starting a fresh session');
  }
  state.textContent = 'Ready. Keep this page visible and focused. The CLI prepares only when this page claims work.';
  for (;;) {
    if (document.visibilityState === 'visible' && document.hasFocus()) {
      const job = await api('next');
      if (job?.phase === 'waiting') {
        state.textContent = 'Waiting for the previous page to cancel or its offer lease to expire. Up to '
          + Math.ceil(job.wait_ms / 1000) + ' seconds remain.';
      } else if (job?.phase === 'offered' && !await ceremony(job)) {
        return;
      }
    }
    await sleep(250);
  }
}
run().catch(error => { state.textContent = String(error) + ' Reopen using the CLI; do not resubmit uncertain work.'; });
