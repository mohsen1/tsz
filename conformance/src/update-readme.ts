#!/usr/bin/env node
/**
 * Updates README.md with conformance test results.
 *
 * Usage:
 *   node dist/update-readme.js --passed=426 --total=500 --ts-version="6.0.0-dev.20260116"
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../..');
const README_PATH = path.join(ROOT_DIR, 'README.md');
const TS_VERSIONS_PATH = path.join(ROOT_DIR, 'conformance/typescript-versions.json');

interface ConformanceStats {
  passed: number;
  total: number;
  tsVersion: string;
  passRate: number;
}

function getTypeScriptVersion(): string {
  try {
    const versions = JSON.parse(fs.readFileSync(TS_VERSIONS_PATH, 'utf-8'));
    const mappings = versions.mappings || {};
    const firstMapping = Object.values(mappings)[0] as { npm: string } | undefined;
    return firstMapping?.npm || versions.default?.npm || 'unknown';
  } catch {
    return 'unknown';
  }
}

function generateProgressBar(percentage: number, width: number = 20): string {
  const filled = Math.round((percentage / 100) * width);
  const empty = width - filled;
  const bar = '\u2588'.repeat(filled) + '\u2591'.repeat(empty);
  return `Progress: [${bar}] ${percentage.toFixed(1)}%`;
}

function updateReadme(stats: ConformanceStats): void {
  let readme = fs.readFileSync(README_PATH, 'utf-8');

  const progressBar = generateProgressBar(stats.passRate);
  const now = new Date().toISOString().split('T')[0];

  const newContent = `<!-- CONFORMANCE_START -->
| Metric | Value |
|--------|-------|
| **TypeScript Version** | \`${stats.tsVersion}\` |
| **Tests Passed** | ${stats.passed.toLocaleString()} / ${stats.total.toLocaleString()} |
| **Pass Rate** | ${stats.passRate.toFixed(1)}% |

\`\`\`
${progressBar}
\`\`\`

*Last updated: ${now}*
<!-- CONFORMANCE_END -->`;

  const startMarker = '<!-- CONFORMANCE_START -->';
  const endMarker = '<!-- CONFORMANCE_END -->';
  const startIdx = readme.indexOf(startMarker);
  const endIdx = readme.indexOf(endMarker);

  if (startIdx === -1 || endIdx === -1) {
    console.error('Could not find conformance markers in README.md');
    process.exit(1);
  }

  readme = readme.slice(0, startIdx) + newContent + readme.slice(endIdx + endMarker.length);

  fs.writeFileSync(README_PATH, readme);
  console.log(`Updated README.md with conformance stats:`);
  console.log(`  Pass Rate: ${stats.passRate.toFixed(1)}% (${stats.passed}/${stats.total})`);
  console.log(`  TypeScript: ${stats.tsVersion}`);
}

function main(): void {
  const args = process.argv.slice(2);
  const stats: Partial<ConformanceStats> = {};

  for (const arg of args) {
    if (arg.startsWith('--passed=')) {
      stats.passed = parseInt(arg.split('=')[1], 10);
    } else if (arg.startsWith('--total=')) {
      stats.total = parseInt(arg.split('=')[1], 10);
    } else if (arg.startsWith('--ts-version=')) {
      stats.tsVersion = arg.split('=')[1];
    } else if (arg.startsWith('--pass-rate=')) {
      stats.passRate = parseFloat(arg.split('=')[1]);
    }
  }

  if (!stats.tsVersion) {
    stats.tsVersion = getTypeScriptVersion();
  }

  if (stats.passRate === undefined && stats.passed !== undefined && stats.total !== undefined && stats.total > 0) {
    stats.passRate = (stats.passed / stats.total) * 100;
  }

  if (stats.passed === undefined || stats.total === undefined || stats.passRate === undefined) {
    console.error('Usage: node dist/update-readme.js --passed=N --total=N [--ts-version=VERSION]');
    process.exit(1);
  }

  updateReadme(stats as ConformanceStats);
}

main();
