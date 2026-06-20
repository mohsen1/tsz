#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';

const repoRoot = execFileSync('git', ['rev-parse', '--show-toplevel'], {
  encoding: 'utf8',
}).trim();

const docsFiles = execFileSync(
  'git',
  ['ls-files', '--cached', '--others', '--exclude-standard', 'docs/how-tsz-works'],
  { cwd: repoRoot, encoding: 'utf8' },
)
  .split('\n')
  .map((line) => line.trim())
  .filter((file) => file.endsWith('.md'));

const haystack = docsFiles
  .map((file) => readFileSync(path.join(repoRoot, file), 'utf8'))
  .join('\n');

const repoFiles = execFileSync(
  'git',
  ['ls-files', '--cached', '--others', '--exclude-standard'],
  { cwd: repoRoot, encoding: 'utf8' },
)
  .split('\n')
  .map((line) => line.trim())
  .filter(Boolean)
  .filter((file) => !file.startsWith('target/'))
  .filter((file) => !file.startsWith('crates/.target/'));

const missing = repoFiles.filter((file) => !haystack.includes(file));

if (missing.length > 0) {
  console.error(`Missing ${missing.length} file path mention${missing.length === 1 ? '' : 's'} from docs/how-tsz-works:`);
  for (const file of missing.slice(0, 200)) {
    console.error(`  ${file}`);
  }
  if (missing.length > 200) {
    console.error(`  ... ${missing.length - 200} more`);
  }
  process.exit(1);
}

console.log(`ok: docs/how-tsz-works mentions ${repoFiles.length} repository files`);
