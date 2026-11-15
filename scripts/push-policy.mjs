#!/usr/bin/env node
import {execFileSync} from 'node:child_process';
import {basename} from 'node:path';

function git(args, options = {}) {
  return execFileSync('git', args, {encoding: 'utf8', ...options});
}

function argument(name) {
  const index = process.argv.indexOf(name);
  if (index === -1 || !process.argv[index + 1]) throw new Error(`${name} is required`);
  return process.argv[index + 1];
}

function commitish(value) {
  if (!/^0{40}$/.test(value)) return value;
  return git(['hash-object', '-t', 'tree', '/dev/null']).trim();
}

function changes(base, head) {
  const output = git(['diff', '--name-status', '--find-renames', base, head, '--']);
  return output.trim().split('\n').filter(Boolean).map(line => {
    const fields = line.split('\t');
    const status = fields[0];
    const path = fields.at(-1);
    return {status, path};
  });
}

function isDocumentation(path) {
  return path.startsWith('docs/') || path.toLowerCase().endsWith('.md');
}

function isTest(path) {
  const name = basename(path).toLowerCase();
  return /(^|\/)(__tests__|tests?)(\/|$)/.test(path.toLowerCase())
    || /(^test[-_.]|[-_.]tests?\.)/.test(name)
    || /\.test\.[^.]+$/.test(name);
}

function lineCounts(base, head, path) {
  const line = git(['diff', '--numstat', base, head, '--', path]).trim();
  if (!line) return null;
  const [added, deleted] = line.split('\t');
  if (!/^\d+$/.test(added) || !/^\d+$/.test(deleted)) return null;
  return {added: Number(added), deleted: Number(deleted)};
}

function addedLines(base, head, path) {
  return git(['diff', '--unified=0', base, head, '--', path])
    .split('\n')
    .filter(line => line.startsWith('+') && !line.startsWith('+++'))
    .map(line => line.slice(1));
}

function weakensTest(line) {
  return /#\s*\[ignore\]|\b(?:describe|it|test)\.skip\b|@unittest\.skip\b|pytest\.mark\.(?:skip|xfail)\b|\breturn\b/.test(line);
}

function classify(change, base, head) {
  if (isDocumentation(change.path)) return {path: change.path, lowRisk: true, reason: 'documentation'};
  if (!isTest(change.path)) return {path: change.path, lowRisk: false, reason: 'not documentation or a test-only addition'};
  if (change.status === 'A') return {path: change.path, lowRisk: true, reason: 'new test'};
  if (change.status !== 'M') return {path: change.path, lowRisk: false, reason: `test status ${change.status} is not an addition`};
  const counts = lineCounts(base, head, change.path);
  if (!counts || counts.added === 0 || counts.deleted !== 0) {
    return {path: change.path, lowRisk: false, reason: 'extended tests may add lines but may not delete or replace existing assertions'};
  }
  const weakening = addedLines(base, head, change.path).find(weakensTest);
  if (weakening) return {path: change.path, lowRisk: false, reason: `test addition can bypass assertions: ${weakening.trim()}`};
  return {path: change.path, lowRisk: true, reason: 'test-only addition'};
}

function main() {
  const route = argument('--route');
  if (!['direct', 'pr'].includes(route)) throw new Error('--route must be direct or pr');
  const base = commitish(argument('--base'));
  const head = argument('--head');
  const results = changes(base, head).map(change => classify(change, base, head));
  const blocked = results.filter(result => !result.lowRisk);
  if (route === 'direct' && blocked.length) {
    for (const result of blocked) {
      console.error(`push-policy: ${result.path} is not low-risk (${result.reason}); open a PR`);
    }
    process.exitCode = 1;
    return;
  }
  if (route === 'pr' && blocked.length) {
    for (const result of blocked) console.log(`push-policy: PR + BOX verify required for ${result.path}`);
    return;
  }
  console.log(`push-policy: ${route === 'direct' ? 'direct-main' : 'PR'} lane accepted ${results.length} low-risk path(s)`);
}

try {
  main();
} catch (error) {
  console.error(`push-policy: ${error.message}`);
  process.exitCode = 2;
}
