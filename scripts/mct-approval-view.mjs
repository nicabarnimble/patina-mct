const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
const icon = path => `<span class="icon"><svg aria-hidden="true" focusable="false" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${path}</svg></span>`;
const icons = {
  user: icon('<circle cx="12" cy="8" r="5"/><path d="M20 21a8 8 0 0 0-16 0"/>'),
  key: icon('<path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/><circle cx="16.5" cy="7.5" r=".5" fill="currentColor"/>'),
  file: icon('<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/>'),
  ok: icon('<circle cx="12" cy="12" r="10"/><path d="m16 9-5.5 5.5L8 12"/>'),
  stop: icon('<circle cx="12" cy="12" r="10"/><path d="M8 12h8"/>'),
  clock: icon('<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>'),
  shield: icon('<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m14.5 9.5-5 5"/><path d="m9.5 9.5 5 5"/>'),
  alert: icon('<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>'),
};

function formatTime(seconds) {
  return new Intl.DateTimeFormat('en-US', {hour: 'numeric', minute: '2-digit', timeZoneName: 'short'}).format(new Date(seconds * 1000));
}
function remaining(deadline) { return Math.max(0, Math.ceil((deadline - Date.now() / 1000) / 60)); }
function formatGrantEnd(value, now) {
  if (value === 'On approval') return {duration: 'On approval', deadline: ''};
  const expiry = Date.parse(value);
  if (!Number.isFinite(expiry)) return {duration: 'Invalid expiry', deadline: ''};
  const milliseconds = Math.max(0, expiry - now);
  const hours = Math.floor(milliseconds / 3_600_000);
  const duration = milliseconds > 0 && hours === 0 ? '<1 h more' : `${hours} h more`;
  const deadline = new Intl.DateTimeFormat('en-US', {
    weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit',
    hour12: true, timeZone: 'America/New_York',
  }).format(new Date(expiry)).replace(/^([^,]+),/, '$1') + ' ET';
  return {duration, deadline};
}
function shell(content, {title, token = '', phase = '', revision = 0, csrf = '', kind = '', subject = ''}) {
  return `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"><meta name="color-scheme" content="light dark"><title>${esc(title)}</title><link rel="stylesheet" href="/tokens.css"><link rel="stylesheet" href="/approval.css"><style>html,body{height:100%;background:var(--bg)}body{display:flex;flex-direction:column;position:relative;max-width:430px;margin:0 auto}</style></head><body data-token="${esc(token)}" data-phase="${esc(phase)}" data-revision="${esc(revision)}" data-csrf="${esc(csrf)}" data-kind="${esc(kind)}" data-subject="${esc(subject)}">${content}<script type="module" src="/approval-client.mjs"></script></body></html>`;
}
function head(code, subject, matched = true) {
  return `<header class="page-head"><span class="product">MCT approval</span><span class="who">${subject ? `Signed in as ${esc(subject)}` : ''}</span></header><div class="match"><span>${matched ? 'Matches' : 'Code on'} the bot’s screen</span><b class="code" aria-label="Code ${esc(code.replace('-', ' '))}">${esc(code)}</b></div>`;
}
function pending(info, token) {
  const proof = info.kind === 'proof';
  return shell(`<main class="page" id="approval">${head(info.code, '', false)}<div class="lede"><h1>${proof ? 'A bot is asking you to check access' : 'A bot is asking for your approval'}</h1><p>Check the code above matches the one on the bot’s screen.</p></div><ol class="steps"><li><span><b>Sign in</b> with your Pocket ID passkey. This only shows you ${proof ? 'what to check' : 'the request'}.</span></li><li><span><b>${proof ? 'Check' : 'Approve'}</b> ${proof ? 'that you can open one thing. Nothing is granted or shared.' : 'with your MCT passkey. Only this step grants access.'}</span></li></ol><footer class="page-foot"><p class="step">Step 1 of 2 · Sign in</p><a class="btn btn-signin" href="/a/${esc(token)}/login">${icons.user}<span>${proof ? 'Sign in to see what to check' : 'Sign in to see this request'}</span></a><p class="fine">You’ll sign in at <b>id.nicabar.org</b>, then come right back.<br>This code expires in ${remaining(info.deadline)} min.</p></footer></main>`, {title: 'MCT approval · Sign in', token, phase: info.phase, kind: info.kind});
}
function loading(state, token, csrf) {
  const proof = state.kind === 'proof';
  return shell(`<main class="page" id="approval">${head(state.code, state.subject)}<div class="lede" role="status"><h1>You’re signed in</h1><p>Nothing has been ${proof ? 'checked' : 'approved'}. Loading the request so you can check it…</p></div><div class="skeleton" aria-hidden="true"><i class="tall"></i><i class="m"></i><i></i><i class="s"></i><i class="m"></i><i class="s"></i></div><footer class="page-foot"><p class="fine">This code expires in ${remaining(state.deadline)} min.</p></footer></main>`, {title: 'MCT approval · Back from sign-in', token, phase: state.phase, revision: state.revision, csrf, kind: state.kind, subject: state.subject});
}
function grantReview(state, token, csrf, cancelled, now) {
  const request = state.request;
  const rows = request.permissions.map(item => `<li><b>${esc(item.name)}</b><span>${esc(item.description)}</span></li>`).join('');
  const grantEnd = formatGrantEnd(request.grant_ends_at, now);
  const deadline = grantEnd.deadline ? `<span>until ${esc(grantEnd.deadline)}</span>` : '';
  const notice = cancelled ? `<div class="notice" role="status">${icons.alert}<span><b>Passkey cancelled.</b> Nothing was approved. Try again when you’re ready.</span></div>` : '';
  return shell(`<main class="page" id="approval">${head(state.code, state.subject)}<section class="request" aria-label="What you are approving"><p class="req-who">${esc(request.asker)} wants to use</p><h1 class="req-mother">${esc(request.mother)}</h1><h2 class="req-label">It will be able to</h2><ul class="perms">${rows}</ul><dl class="req-meta"><dt>For</dt><dd><b>${esc(grantEnd.duration)}</b>${deadline}</dd><dt>Asked</dt><dd>${formatTime(state.created_at)}</dd></dl></section><footer class="page-foot">${notice}<p class="step">Step 2 of 2 · Approve</p><button class="btn btn-approve" id="approve">${icons.key}<span>Approve with MCT passkey</span></button><button class="btn btn-quiet" id="decline">Decline</button><p class="fine">Your MCT passkey in 1Password signs exactly what’s above.<br>Code expires in ${remaining(state.deadline)} min</p></footer></main>`, {title: 'MCT approval · Review', token, phase: state.phase, revision: state.revision, csrf, kind: state.kind, subject: state.subject});
}
function proofReview(state, token, csrf) {
  const r = state.request;
  return shell(`<main class="page" id="approval">${head(state.code, state.subject)}<section class="request" aria-label="What you are checking"><h1 class="proof-h">Confirm you can open ${esc(r.title)} on ${esc(r.site)}</h1><div class="doc">${icons.file}<div><b>${esc(r.title)}</b><span class="mono">${esc(r.site + r.path)}</span></div></div><dl class="req-meta"><dt>Asked by</dt><dd><b>${esc(r.asker)}</b><span>${formatTime(state.created_at)}</span></dd><dt>Why</dt><dd>${esc(r.why)}</dd></dl><p class="proof-safe">${icons.ok}<span>This only checks that you can open it. The bot won’t get your sign-in or a copy of the file.</span></p></section><footer class="page-foot"><p class="step">Step 2 of 2 · Check</p><button class="btn btn-check" id="check">${icons.file}<span>Check I can open it</span></button><button class="btn btn-quiet" id="skip">Not now</button><p class="fine">You’ll pass through ${esc(r.site)} for a moment and come right back.<br>This code expires in ${remaining(state.deadline)} min.</p></footer></main>`, {title: 'MCT approval · Review · access proof', token, phase: state.phase, revision: state.revision, csrf, kind: state.kind, subject: state.subject});
}
function result(state, token, csrf, now) {
  const r = state.request || {};
  const grantEnd = formatGrantEnd(r.grant_ends_at, now);
  const approvedEnd = grantEnd.deadline || grantEnd.duration;
  const map = {
    approved: ['ok', icons.ok, 'Approved', `<b>${esc(r.asker)}</b> can use <b class="mono">${esc(r.mother)}</b> until <b>${esc(approvedEnd)}</b>.`, 'The bot has been told. You can close this page.'],
    declined: ['muted', icons.stop, 'Declined', `<b>${esc(r.asker)}</b> won’t get access to <b class="mono">${esc(r.mother)}</b>. Nothing changed.`, 'The bot has been told. You can close this page.'],
    passed: ['ok', icons.ok, 'You can open it', `<b>${esc(r.asker)}</b> has been told. It got a pass, not your sign-in or the file.`, 'You can close this page.'],
    failed: ['stop', icons.shield, 'You can’t open it', state.reason === 'not_found' ? `${esc(r.title)} isn’t on ${esc(r.site)} at that address. Nothing was shared, and <b>${esc(r.asker)}</b> has been told it failed.` : `${esc(r.site)} wouldn’t let <b>nicabar</b> open ${esc(r.title)}. Nothing was shared, and <b>${esc(r.asker)}</b> has been told it failed.`, `Ask ${esc(r.asker)} for a new code once it’s fixed.`],
    skipped: ['muted', icons.stop, 'Skipped', 'Nothing was checked.', `${esc(r.asker)} has been told you skipped it.`],
    expired: ['wait', icons.clock, 'This code has expired', `Codes only last 10 minutes, so an old photo of one can’t be used. <b>Nothing was ${state.kind === 'proof' ? 'checked' : 'approved'}.</b>`, `Ask ${esc(r.asker || 'the bot')} for a new code and scan that one.`],
    refused: ['stop', icons.shield, state.kind === 'proof' ? 'We couldn’t accept this check' : 'We couldn’t accept this approval', `The request changed after the bot made it, or this was already sent once. To keep you safe, <b>nothing was ${state.kind === 'proof' ? 'checked' : 'approved'}.</b>`, `Ask ${esc(r.asker || 'the bot')} for a new code and scan again.`],
  };
  const [klass, glyph, heading, body, next] = map[state.phase] || map.refused;
  const view = r.title && state.phase === 'passed' ? `<footer class="page-foot"><a class="btn btn-quiet btn-link" href="https://${esc(r.site + r.path)}" target="_blank" rel="noopener">View the document</a></footer>` : '';
  return shell(`<main class="page" id="approval"><header class="page-head"><span class="product">MCT approval</span><span class="who">${state.subject ? `Signed in as ${esc(state.subject)}` : ''}</span></header><section class="result" role="${['failed','expired','refused'].includes(state.phase) ? 'alert' : 'status'}"><span class="result-glyph ${klass}">${glyph}</span><h1>${heading}</h1><p>${body}</p><p>${next}</p><dl class="recap"><dt>Code</dt><dd><span class="mono">${esc(state.code)}</span> ${state.phase === 'expired' ? 'is dead' : 'is now used up'}</dd>${state.decided_at ? `<dt>At</dt><dd>${formatTime(state.decided_at)}</dd>` : ''}</dl></section>${view}</main>`, {title: `MCT approval · ${heading}`, token, phase: state.phase, revision: state.revision, csrf, kind: state.kind, subject: state.subject});
}

export function renderPhone(state, {token, csrf = '', wrong = false, cancelled = false, now = Date.now()} = {}) {
  if (wrong) return shell(`<main class="page" id="approval">${head(state.code, 'another account', false)}<section class="result" role="alert" style="padding-top:22px"><span class="result-glyph wait">${icons.user}</span><h1>This request is for nicabar</h1><p>You’re signed in as <b>another account</b>. Only nicabar can ${state.kind === 'proof' ? 'do this check' : 'approve MCT requests'}. Nothing was approved.</p></section><footer class="page-foot"><a class="btn btn-signin" href="/a/${esc(token)}/login?switch=1">${icons.user}<span>Sign in as nicabar</span></a></footer></main>`, {title: 'MCT approval · Signed in as someone else', token, phase: 'wrong', kind: state.kind});
  if (!state.subject && state.phase === 'waiting') return pending(state, token);
  if (state.phase === 'opened' && state.kind === 'grant') return loading(state, token, csrf);
  if (state.phase === 'prepared') return grantReview(state, token, csrf, cancelled, now);
  if (state.phase === 'opened' && state.kind === 'proof') return proofReview(state, token, csrf);
  if (state.phase === 'signed') return loading({...state, phase: 'signed'}, token, csrf);
  return result(state, token, csrf, now);
}

export function renderDisplay(state) {
  const r = state.request;
  const proof = state.kind === 'proof';
  const items = proof ? `<li>${esc(r.path)}</li><li>${esc(r.why)}</li><li>Nothing is granted</li>` : r.permissions.map(item => `<li>${esc(item.name)}</li>`).join('');
  const line = state.phase === 'waiting' ? 'Waiting for you on your phone…'
    : state.phase === 'opened' || state.phase === 'prepared' || state.phase === 'signed' ? `Opened on your phone. Not ${proof ? 'checked' : 'approved'} yet.`
    : state.phase === 'approved' ? `✓ Approved on your phone at ${formatTime(state.decided_at)}. Continuing.`
    : state.phase === 'passed' ? `✓ Passed at ${formatTime(state.decided_at)}: nicabar can open it on wiki.nicabar.org.`
    : state.phase === 'failed' ? `× Failed at ${formatTime(state.decided_at)}: ${state.reason === 'not_found' ? "that file isn't on wiki.nicabar.org" : "wiki.nicabar.org wouldn't let nicabar open it"}.`
    : state.phase === 'declined' ? `– Declined on your phone at ${formatTime(state.decided_at)}. Nothing changed.`
    : state.phase === 'skipped' ? `– Skipped on your phone at ${formatTime(state.decided_at)}. Nothing was checked.`
    : state.phase === 'expired' ? `Code expired at ${formatTime(state.decided_at)}. Ask again for a new code.`
    : `× Refused: the ${proof ? 'answer' : 'approval'} didn't match this request.`;
  return `<!DOCTYPE html><html lang="en" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${proof ? 'Access check needed' : 'MCT approval needed'}</title><link rel="stylesheet" href="/tokens.css"><link rel="stylesheet" href="/approval.css"><style>body{padding:40px;background:var(--bg);font-family:var(--font-ui)}.bot-panel{max-width:680px;background:var(--surface);color:var(--ink);border-radius:14px;overflow:hidden;box-shadow:0 0 0 1px var(--rule-strong)}.bp-title{padding:10px 18px;border-bottom:1px solid var(--rule);text-align:center}.bp-body{display:grid;grid-template-columns:auto 1fr;gap:26px;padding:22px 24px 10px}.bp-qr{background:#fff;width:205px;height:205px}.bp-qr img{width:205px;height:205px}.bp-mother{font:650 26px/1.15 var(--font-mono);margin:2px 0 10px}.bp-perms{list-style:none;border-top:1px solid var(--rule)}.bp-perms li{padding:5px 0;border-bottom:1px solid var(--rule)}.bp-code b{font:650 34px/1 var(--font-mono);letter-spacing:.08em}.bp-status{border-top:1px solid var(--rule);padding:12px 24px;font-weight:600}.bp-url{padding:4px 24px 12px;font:12.5px var(--font-mono);color:var(--ink-2)}</style></head><body data-display="${esc(state.display_id)}" data-revision="${esc(state.revision)}"><section class="bot-panel" data-kind="${proof ? 'proof' : 'grant'}"><div class="bp-title">${proof ? 'Access check needed' : 'MCT approval needed'}</div><div class="bp-body"><div><div class="bp-qr"><img alt="QR code to open on your phone" src="/d/${esc(state.display_id)}/qr.svg"></div><p>Scan with your phone</p></div><div><p>${esc(r.asker)} ${proof ? 'needs proof you can open' : 'wants to use'}</p><p class="bp-mother">${esc(proof ? r.title : r.mother)}</p><ul class="bp-perms">${items}</ul><div class="bp-code"><span>Code </span><b>${esc(state.code)}</b></div><p>Code expires in ${state.remaining_seconds}s</p></div></div><p class="bp-url">${esc(state.url)}</p><div class="bp-status">${esc(line)}</div></section><script type="module" src="/display-client.mjs"></script></body></html>`;
}
