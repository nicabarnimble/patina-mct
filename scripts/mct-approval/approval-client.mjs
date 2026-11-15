import {signPrepared} from '/passkey-admin.mjs';

const body = document.body;
const token = body.dataset.token;
const csrf = body.dataset.csrf;
const phase = body.dataset.phase;
const revision = Number(body.dataset.revision || 0);

async function post(path, value = {}) {
  const response = await fetch(path, {method: 'POST', credentials: 'same-origin', cache: 'no-store',
    headers: {'Content-Type': 'application/json', 'X-MCT-CSRF': csrf}, body: JSON.stringify(value),
    signal: AbortSignal.timeout(10000)});
  if (!response.ok) throw new Error('The approval server refused this action. Ask the bot for a new code.');
  return response.json();
}

const approve = document.getElementById('approve');
if (approve) approve.onclick = async () => {
  approve.disabled = true; approve.setAttribute('aria-busy', 'true');
  try {
    const response = await fetch(`/a/${token}/challenge`, {credentials: 'same-origin', cache: 'no-store', signal: AbortSignal.timeout(5000)});
    if (!response.ok) throw new Error('This request is no longer signable.');
    const prepared = await response.json();
    const result = await signPrepared(prepared, {signal: AbortSignal.timeout(120000)});
    await post(`/a/${token}/approve`, {result});
    location.reload();
  } catch (error) {
    approve.disabled = false; approve.setAttribute('aria-busy', 'false');
    if (error?.name === 'NotAllowedError') location.replace(`/a/${token}?cancelled=1`);
    else document.querySelector('.page-foot')?.insertAdjacentHTML('afterbegin', `<div class="notice" role="status"><span><b>Approval stopped.</b> ${String(error.message || error)}</span></div>`);
  }
};
const decline = document.getElementById('decline');
if (decline) decline.onclick = async () => { decline.disabled = true; await post(`/a/${token}/decline`); location.reload(); };
const check = document.getElementById('check');
if (check) check.onclick = () => { location.href = `/a/${token}/proof/go`; };
const skip = document.getElementById('skip');
if (skip) skip.onclick = async () => { skip.disabled = true; await post(`/a/${token}/skip`); location.reload(); };

if (token && !['approved','declined','passed','failed','skipped','expired','refused'].includes(phase)) {
  const events = new EventSource(`/a/${token}/events`);
  events.addEventListener('status', event => {
    const update = JSON.parse(event.data);
    if (update.revision !== revision || update.phase !== phase) location.reload();
  });
}
