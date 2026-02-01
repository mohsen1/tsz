#!/usr/bin/env node
/**
 * Updates README.md with conformance, fourslash, or emit test results.
 *
 * Usage:
 *   # Update conformance progress:
 *   node dist/update-readme.js --passed=426 --total=500 --ts-version="6.0.0-dev.20260116"
 *
 *   # Update fourslash/LSP progress:
 *   node dist/update-readme.js --fourslash --passed=3 --total=50 --ts-version="6.0.0-dev.20260116"
 *
 *   # Update emit progress (JS and/or DTS):
 *   node dist/update-readme.js --emit --js-passed=37 --js-total=448 --dts-passed=0 --dts-total=0
 *
 *   # Update all (separate invocations):
 *   node dist/update-readme.js --passed=10184 --total=12379
 *   node dist/update-readme.js --fourslash --passed=3 --total=50
 *   node dist/update-readme.js --emit --js-passed=37 --js-total=448
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');
const README_PATH = path.join(ROOT_DIR, 'README.md');
const TS_VERSIONS_PATH = path.join(ROOT_DIR, 'scripts/conformance/typescript-versions.json');

interface ProgressStats {
  passed: number;
  total: number;
  tsVersion: string;
  passRate: number;
}

interface EmitStats {
  jsPassed: number;
  jsTotal: number;
  dtsPassed: number;
  dtsTotal: number;
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

function updateEmitReadme(stats: EmitStats): void {
  let readme = fs.readFileSync(README_PATH, 'utf-8');

  const jsPassRate = stats.jsTotal > 0 ? (stats.jsPassed / stats.jsTotal) * 100 : 0;
  const dtsPassRate = stats.dtsTotal > 0 ? (stats.dtsPassed / stats.dtsTotal) * 100 : 0;

  const jsBar = generateProgressBar(jsPassRate, stats.jsPassed, stats.jsTotal);
  const dtsBar = generateProgressBar(dtsPassRate, stats.dtsPassed, stats.dtsTotal);

  const newContent = `<!-- EMIT_START -->
\`\`\`
JavaScript:  ${jsBar.replace('Progress: ', '')}
Declaration: ${dtsBar.replace('Progress: ', '')}
\`\`\`
<!-- EMIT_END -->`;

  readme = updateSection(readme, '<!-- EMIT_START -->', '<!-- EMIT_END -->', newContent);
  fs.writeFileSync(README_PATH, readme);

  console.log(`Updated README.md with emit stats:`);
  console.log(`  JavaScript:  ${jsPassRate.toFixed(1)}% (${stats.jsPassed}/${stats.jsTotal})`);
  console.log(`  Declaration: ${dtsPassRate.toFixed(1)}% (${stats.dtsPassed}/${stats.dtsTotal})`);
}

function main(): void {
  const args = process.argv.slice(2);
  const stats: Partial<ProgressStats> = {};
  const emitStats: Partial<EmitStats> = {};
  let section: 'conformance' | 'fourslash' | 'emit' = 'conformance';

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
    } else if (arg === '--emit') {
      section = 'emit';
    } else if (arg.startsWith('--js-passed=')) {
      emitStats.jsPassed = parseInt(arg.split('=')[1], 10);
    } else if (arg.startsWith('--js-total=')) {
      emitStats.jsTotal = parseInt(arg.split('=')[1], 10);
    } else if (arg.startsWith('--dts-passed=')) {
      emitStats.dtsPassed = parseInt(arg.split('=')[1], 10);
    } else if (arg.startsWith('--dts-total=')) {
      emitStats.dtsTotal = parseInt(arg.split('=')[1], 10);
    }
  }

  // Handle emit section
  if (section === 'emit') {
    if (emitStats.jsPassed === undefined || emitStats.jsTotal === undefined) {
      console.error('Usage: node dist/update-readme.js --emit --js-passed=N --js-total=N [--dts-passed=N --dts-total=N]');
      process.exit(1);
    }
    emitStats.dtsPassed = emitStats.dtsPassed ?? 0;
    emitStats.dtsTotal = emitStats.dtsTotal ?? 0;
    updateEmitReadme(emitStats as EmitStats);
    return;
  }

  // Handle conformance/fourslash sections
  if (!stats.tsVersion) {
    stats.tsVersion = getTypeScriptVersion();
  }

  if (stats.passRate === undefined && stats.passed !== undefined && stats.total !== undefined && stats.total > 0) {
    stats.passRate = (stats.passed / stats.total) * 100;
  }

  if (stats.passed === undefined || stats.total === undefined || stats.passRate === undefined) {
    console.error('Usage: node dist/update-readme.js [--fourslash|--emit] --passed=N --total=N [--ts-version=VERSION]');
    console.error('');
    console.error('Sections:');
    console.error('  (default)     Update conformance progress');
    console.error('  --fourslash   Update fourslash/LSP progress');
    console.error('  --lsp         Alias for --fourslash');
    console.error('  --emit        Update emit progress (use --js-passed/--js-total/--dts-passed/--dts-total)');
    process.exit(1);
  }

  updateReadme(stats as ProgressStats, section);
}

main();
