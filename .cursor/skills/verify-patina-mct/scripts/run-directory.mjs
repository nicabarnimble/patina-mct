import fs from 'node:fs';
import path from 'node:path';

const marker = '.verify-patina-mct.json';
const uid = process.getuid();

function runPath() {
  const input = process.env.VERIFY_PATINA_MCT_ROOT;
  if (!input || !path.isAbsolute(input)) throw new Error('VERIFY_PATINA_MCT_ROOT must be absolute');
  const selected = input.replace(/\/+$/, '');
  if (!selected || ['.', '..'].includes(path.basename(selected))) throw new Error('refusing a broad run directory');
  const root = path.join(fs.realpathSync(path.dirname(selected)), path.basename(selected));
  const home = fs.realpathSync(process.env.HOME);
  if (root === home || root.startsWith(`${home}/`)) throw new Error('refusing writable run state inside home');
  if (fs.lstatSync(root, { throwIfNoEntry: false })?.isSymbolicLink()) throw new Error('refusing a symlink run directory');
  return root;
}

function validateTree(entry) {
  const info = fs.lstatSync(entry);
  if (info.uid !== uid || info.isSymbolicLink()) throw new Error(`unsafe owner or symlink at ${entry}`);
  if (info.isDirectory()) {
    if ((info.mode & 0o7777) !== 0o700) throw new Error(`run directory must be mode 0700: ${entry}`);
    for (const name of fs.readdirSync(entry)) validateTree(path.join(entry, name));
  } else if (!info.isFile() || info.nlink !== 1 || (info.mode & 0o077) !== 0) {
    throw new Error(`unsafe run file at ${entry}`);
  }
}

function validateRun(root) {
  validateTree(root);
  const recorded = JSON.parse(fs.readFileSync(path.join(root, marker), 'utf8'));
  if (recorded.version !== 1 || recorded.root !== root || recorded.uid !== uid) {
    throw new Error('run marker does not match this directory and owner');
  }
}

function prepare(root) {
  if (!fs.existsSync(root)) {
    fs.mkdirSync(root, { mode: 0o700 });
    fs.writeFileSync(path.join(root, marker), JSON.stringify({ version: 1, root, uid }), { flag: 'wx', mode: 0o600 });
  }
  validateRun(root);
  for (const directory of ['scratch', 'scratch/mother', 'proof']) {
    const target = path.join(root, directory);
    if (!fs.existsSync(target)) fs.mkdirSync(target, { mode: 0o700 });
    if (!fs.lstatSync(target).isDirectory()) throw new Error(`expected a run directory: ${target}`);
  }
}

try {
  const action = process.argv[2];
  if (!['path', 'prepare', 'reset', 'cleanup'].includes(action)) throw new Error('unknown run-directory action');
  const root = runPath();
  if (action === 'prepare' || action === 'reset') prepare(root);
  if (action === 'cleanup') {
    validateRun(root);
    if (!fs.lstatSync(path.join(root, 'proof')).isDirectory()) throw new Error('cleanup requires proof');
  }
  if (action === 'reset' || action === 'cleanup') {
    fs.rmSync(path.join(root, 'scratch'), { recursive: true, force: true });
    if (action === 'reset') prepare(root);
  }
  console.log(root);
} catch (error) {
  console.error(error.message);
  process.exitCode = 2;
}
