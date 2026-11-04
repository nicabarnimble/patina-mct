import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { createReadStream, createWriteStream, mkdtempSync, rmSync } from 'node:fs';
import { Writable } from 'node:stream';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { promisify } from 'node:util';
import { sshArgs, validateConfig } from './fleet.mjs';

const exec = promisify(execFile);
const STORE = /^\/nix\/store\/[a-z0-9]{32}-[a-zA-Z0-9._+-]+$/;
const KEY = /^[a-zA-Z0-9._-]+:[a-zA-Z0-9+/]{43}=$/;
const safePath = value => typeof value === 'string' && /^\/[a-zA-Z0-9/._+-]+$/.test(value) && value.split('/').slice(1).every(part => part && part !== '.' && part !== '..');

export function cacheCommand(packagePath) {
  assert.match(packagePath, STORE, 'immutable Nix package required');
  return ['bash', '-c', `set -euo pipefail
umask 077
package=$1
root=$(mktemp -d "$HOME/.mct-federation-cache.XXXXXXXX")
printf 'mct-federation-cache-v1\\n' > "$root/.mct-federation-owned"
trap 'rm -rf -- "$root"' ERR
nix-store --generate-binary-cache-key "mct-federation-$(basename "$root")" "$root/secret" "$root/public"
sudo -n nix store sign --key-file "$root/secret" "$package" >&2
nix copy --to "file://$root/cache" "$package" >&2
read -r key < "$root/public"
sudo -n nix store verify --recursive --option extra-trusted-public-keys "$key" "$package" >&2
printf '%s\\n%s\\n' "$root" "$key"
`, '--', packagePath];
}

export function parseCacheReceipt(stdout) {
  const fields = stdout.trim().split(/\r?\n/);
  assert.equal(fields.length, 2, 'invalid cache receipt');
  assert.ok(safePath(fields[0]), 'unsafe cache path');
  assert.match(fields[1], KEY, 'invalid cache public key');
  return { root: fields[0], publicKey: fields[1] };
}

export function importCommand(packagePath, cacheRoot, publicKey) {
  assert.match(packagePath, STORE, 'immutable Nix package required');
  assert.ok(safePath(cacheRoot), 'unsafe import path');
  assert.match(publicKey, KEY, 'invalid cache public key');
  return ['bash', '-c', `set -euo pipefail
package=$1
cache=$2
key=$3
sudo -n nix copy --from "file://$cache" --option max-jobs 0 --option builders '' --option require-sigs true --option extra-trusted-public-keys "$key" "$package"
sudo -n nix store verify --recursive --option extra-trusted-public-keys "$key" "$package"
`, '--', packagePath, cacheRoot, publicKey];
}

export function cleanupCommand(root, kind) {
  assert.ok(['cache', 'import'].includes(kind), 'invalid cleanup kind');
  assert.ok(safePath(root) && new RegExp(`/\\.mct-federation-${kind}\\.[a-zA-Z0-9]{8}$`).test(root), 'unowned cleanup path');
  return ['bash', '-c', `set -euo pipefail
root=$1
kind=$2
[[ $(dirname "$root") == "$HOME" ]] || exit 72
[[ ! -L $root ]] || exit 72
[[ -e $root ]] || exit 0
read -r marker < "$root/.mct-federation-owned"
[[ $marker == "mct-federation-$kind-v1" ]] || exit 72
rm -rf -- "$root"
`, '--', root, kind];
}

function remote(config, host, command) {
  return exec('ssh', sshArgs(config, host, command), { timeout: 600_000, maxBuffer: 8 * 1024 * 1024 });
}

async function streamRemote(config, host, command, source, destination) {
  const child = spawn('ssh', sshArgs(config, host, command), { stdio: ['pipe', 'pipe', 'inherit'] });
  const exited = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code, signal) => code === 0 ? resolve() : reject(Error(`cache transfer exited ${code ?? signal}`)));
  });
  const limit = setTimeout(() => child.kill('SIGTERM'), 600_000);
  try {
    if (source) await Promise.all([pipeline(source, child.stdin), pipeline(child.stdout, destination), exited]);
    else { child.stdin.end(); await Promise.all([pipeline(child.stdout, destination), exited]); }
  } catch (error) { child.kill('SIGTERM'); await exited.catch(() => {}); throw error; }
  finally { clearTimeout(limit); }
}

export async function deploy(input, slice, emit = () => {}) {
  const config = validateConfig(input);
  assert.ok(['two', 'three', 'upgrade'].includes(slice), 'invalid slice');
  const hosts = config.hosts.slice(0, slice === 'two' ? 2 : 3);
  assert.equal(hosts.length, slice === 'two' ? 2 : 3, 'insufficient hosts');
  const builder = hosts[0];
  const packages = slice === 'upgrade' ? [config.package, config.baselinePackage] : [config.package];
  for (const packagePath of packages) {
    const receipt = parseCacheReceipt((await remote(config, builder, cacheCommand(packagePath))).stdout);
    const local = mkdtempSync(join(tmpdir(), 'mct-federation-cache-'));
    const archive = join(local, 'cache.tar');
    const failures = [];
    try {
      await streamRemote(config, builder, ['tar', '-cf', '-', '-C', `${receipt.root}/cache`, '.'], null, createWriteStream(archive, { mode: 0o600, flags: 'wx' }));
      emit({ stage: 'cache-exported', builder: builder.id, package: packagePath });
      const results = await Promise.allSettled(hosts.slice(1).map(async host => {
        const root = (await remote(config, host, ['bash', '-c', 'set -eu; umask 077; root=$(mktemp -d "$HOME/.mct-federation-import.XXXXXXXX"); printf "mct-federation-import-v1\\n" > "$root/.mct-federation-owned"; printf "%s\\n" "$root"'])).stdout.trim();
        const cleanup = cleanupCommand(root, 'import');
        try {
          const sink = new Writable({ write(_chunk, _encoding, done) { done(); } });
          await streamRemote(config, host, ['tar', '-xf', '-', '--no-same-owner', '--no-same-permissions', '-C', root], createReadStream(archive), sink);
          await remote(config, host, importCommand(packagePath, root, receipt.publicKey));
          emit({ stage: 'closure-verified', node: host.id, package: packagePath });
        } catch (error) { failures.push(error); }
        finally { await remote(config, host, cleanup); }
      }));
      failures.push(...results.filter(result => result.status === 'rejected').map(result => result.reason));
    } catch (error) { failures.push(error); }
    finally {
      rmSync(local, { recursive: true });
      try { await remote(config, builder, cleanupCommand(receipt.root, 'cache')); }
      catch (error) { failures.push(error); }
    }
    if (failures.length) throw new AggregateError(failures, 'closure delivery or cleanup failed');
  }
}
