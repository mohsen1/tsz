#!/usr/bin/env node
/**
 * Updates README.md with conformance and/or fourslash test results.
 *
 * Usage:
 *   # Update conformance progress:
 *   node dist/update-readme.js --passed=426 --total=500 --ts-version="6.0.0-dev.20260116"
 *
 *   # Update fourslash/LSP progress:
 *   node dist/update-readme.js --fourslash --passed=3 --total=50 --ts-version="6.0.0-dev.20260116"
 *
 *   # Update both (separate invocations):
 *   node dist/update-readme.js --passed=10184 --total=12379
 *   node dist/update-readme.js --fourslash --passed=3 --total=50
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../..');
const README_PATH = path.join(ROOT_DIR, 'README.md');
const TS_VERSIONS_PATH = path.join(ROOT_DIR, 'conformance/typescript-versions.json');

interface ProgressStats {
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

function generateProgressBar(percentage: number, passed: number, total: number, width: number = 20): string {
  const filled = Math.round((percentage / 100) * width);
  const empty = width - filled;
  const bar = '\u2588'.repeat(filled) + '\u2591'.repeat(empty);
  return `Progress: [${bar}] ${percentage.toFixed(1)}% (${passed.toLocaleString()} / ${total.toLocaleString()} tests)`;
}

function updateSection(readme: string, startMarker: string, endMarker: string, newContent: string): string {
  const startIdx = readme.indexOf(startMarker);
  const endIdx = readme.indexOf(endMarker);

  if (startIdx === -1 || endIdx === -1) {
    console.error(`Could not find markers ${startMarker} / ${endMarker} in README.md`);
    process.exit(1);
  }

  return readme.slice(0, startIdx) + newContent + readme.slice(endIdx + endMarker.length);
}

function updateReadme(stats: ProgressStats, section: 'conformance' | 'fourslash'): void {
  let readme = fs.readFileSync(README_PATH, 'utf-8');

  const progressBar = generateProgressBar(stats.passRate, stats.passed, stats.total);

  if (section === 'conformance') {
    const newContent = `<!-- CONFORMANCE_START -->
Currently targeting \`TypeScript\`@\`${stats.tsVersion}\`

\`\`\`
${progressBar}
\`\`\`
<!-- CONFORMANCE_END -->`;

    readme = updateSection(readme, '<!-- CONFORMANCE_START -->', '<!-- CONFORMANCE_END -->', newContent);
    console.log(`Updated README.md with conformance stats:`);
  } else {
    const newContent = `<!-- FOURSLASH_START -->
Currently targeting \`TypeScript\`@\`${stats.tsVersion}\`

\`\`\`
${progressBar}
\`\`\`
<!-- FOURSLASH_END -->`;

    readme = updateSection(readme, '<!-- FOURSLASH_START -->', '<!-- FOURSLASH_END -->', newContent);
    console.log(`Updated README.md with fourslash/LSP stats:`);
  }

  fs.writeFileSync(README_PATH, readme);
  console.log(`  Pass Rate: ${stats.passRate.toFixed(1)}% (${stats.passed}/${stats.total})`);
  console.log(`  TypeScript: ${stats.tsVersion}`);
}

function main(): void {
  const args = process.argv.slice(2);
  const stats: Partial<ProgressStats> = {};
  let section: 'conformance' | 'fourslash' = 'conformance';

  for (const arg of args) {
    if (arg.startsWith('--passed=')) {
      stats.passed = parseInt(arg.split('=')[1], 10);
    } else if (arg.startsWith('--total=')) {
      stats.total = parseInt(arg.split('=')[1], 10);
    } else if (arg.startsWith('--ts-version=')) {
      stats.tsVersion = arg.split('=')[1];
    } else if (arg.startsWith('--pass-rate=')) {
      stats.passRate = parseFloat(arg.split('=')[1]);
    } else if (arg === '--fourslash' || arg === '--lsp') {
      section = 'fourslash';
    }
  }

  if (!stats.tsVersion) {
    stats.tsVersion = getTypeScriptVersion();
  }

  if (stats.passRate === undefined && stats.passed !== undefined && stats.total !== undefined && stats.total > 0) {
    stats.passRate = (stats.passed / stats.total) * 100;
  }

  if (stats.passed === undefined || stats.total === undefined || stats.passRate === undefined) {
    console.error('Usage: node dist/update-readme.js [--fourslash] --passed=N --total=N [--ts-version=VERSION]');
    console.error('');
    console.error('Sections:');
    console.error('  (default)     Update conformance progress');
    console.error('  --fourslash   Update fourslash/LSP progress');
    console.error('  --lsp         Alias for --fourslash');
    process.exit(1);
  }

  updateReadme(stats as ProgressStats, section);
}

main();
