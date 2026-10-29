import {registerRoot, signPrepared, expectedChallenge} from './passkey-admin.mjs';
const byId = id => document.getElementById(id);
let prepared;
function download(name, data) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2) + '\n'], {type:'application/json'}));
  const link = document.createElement('a'); link.href = url; link.download = name;
  link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  byId('result').textContent = 'Downloaded ' + name + '. Nothing was applied to Mother.';
}
async function attempt(task) {
  byId('result').textContent = '';
  try { await task(); } catch (error) { byId('result').textContent = String(error); }
}
byId('register').onclick = () => attempt(async () => {
  download('registered-root.json', await registerRoot(location.hostname, byId('name').value));
});
byId('prepared').onchange = () => attempt(async () => {
  prepared = undefined; byId('sign').disabled = true;
  const file = byId('prepared').files[0];
  if (!file || file.size > 96 * 1024) throw new Error('Expected prepared JSON <=96 KiB');
  const candidate = JSON.parse(await file.text());
  if (candidate.root.origin !== location.origin) throw new Error('Prepared root uses a different registered origin');
  if (await expectedChallenge(candidate) !== candidate.challenge) throw new Error('Challenge does not match intent');
  byId('review').textContent = JSON.stringify(candidate.intent, null, 2);
  prepared = candidate; byId('sign').disabled = false;
});
byId('sign').onclick = () => attempt(async () => {
  byId('sign').disabled = true;
  if (!prepared) throw new Error('Load a prepared intent first');
  download('signed-admin.json', await signPrepared(prepared));
});
