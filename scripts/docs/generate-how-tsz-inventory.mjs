#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const repoRoot = execFileSync('git', ['rev-parse', '--show-toplevel'], {
  encoding: 'utf8',
}).trim();

const outDir = path.join(repoRoot, 'docs/how-tsz-works/file-inventory');
const maxEntriesPerPage = 325;

const allFiles = execFileSync(
  'git',
  ['ls-files', '--cached', '--others', '--exclude-standard'],
  { cwd: repoRoot, encoding: 'utf8' },
)
  .split('\n')
  .map((line) => line.trim())
  .filter(Boolean)
  .filter((file) => !file.startsWith('target/'))
  .filter((file) => !file.startsWith('crates/.target/'))
  .sort();

function crateName(file) {
  const parts = file.split('/');
  return parts[0] === 'crates' ? parts[1] : '';
}

function afterSrc(file) {
  const marker = '/src/';
  const index = file.indexOf(marker);
  return index === -1 ? '' : file.slice(index + marker.length);
}

function alphaBucket(file) {
  const name = path.basename(file).toLowerCase();
  const first = name[0] ?? 'other';
  if (first >= 'a' && first <= 'f') return 'a-f';
  if (first >= 'g' && first <= 'p') return 'g-p';
  if (first >= 'q' && first <= 'z') return 'q-z';
  return 'other';
}

function checkerGroup(file) {
  const srcRel = afterSrc(file);
  if (!srcRel) return 'crates-checker-other';
  if (!srcRel.includes('/')) {
    return `crates-checker-root-${alphaBucket(file)}`;
  }
  const area = srcRel.split('/')[0];
  if (['assignability', 'checkers', 'classes'].includes(area)) {
    return 'crates-checker-assignability-checkers-classes';
  }
  if (['context', 'declarations', 'dispatch', 'error_reporter', 'flow', 'jsdoc'].includes(area)) {
    return 'crates-checker-context-declarations-flow-jsdoc';
  }
  if (['query_boundaries', 'recovery', 'state', 'symbols', 'types'].includes(area)) {
    return 'crates-checker-query-state-types';
  }
  if (area === 'tests' || area === 'test_utils') {
    return 'crates-checker-test-support';
  }
  return 'crates-checker-other';
}

function scriptGroup(file) {
  const area = file.split('/')[1] ?? 'root';
  if (['agents', 'arch'].includes(area)) return 'scripts-architecture-and-agents';
  if (['bench', 'ci', 'cloudbuild', 'infra', 'perf'].includes(area)) return 'scripts-bench-ci-and-performance';
  if (['conformance', 'emit', 'fourslash', 'test-directives'].includes(area)) {
    return 'scripts-conformance-emit-fourslash';
  }
  if (['build', 'githooks', 'lib', 'lsp', 'quality', 'setup', 'vscode-tsz-lsp'].includes(area)) {
    return 'scripts-build-setup-quality-and-lsp';
  }
  return 'scripts-root-and-installers';
}

function inventoryGroup(file) {
  if (file.startsWith('crates/tsz-checker/')) return checkerGroup(file);
  if (file.startsWith('crates/')) {
    const name = crateName(file);
    if (['tsz-common', 'tsz-scanner', 'tsz-parser', 'tsz-binder', 'tsz-lowering'].includes(name)) {
      return 'crates-front-end-and-shared';
    }
    if (['tsz-core', 'tsz-cli', 'tsz-lsp', 'tsz-wasm', 'conformance'].includes(name)) {
      return 'crates-core-cli-lsp-wasm-conformance';
    }
    if (name === 'tsz-solver') return 'crates-solver';
    if (name === 'tsz-emitter') return 'crates-emitter';
    if (name === 'tsz-website') return 'crates-website';
    return 'crates-other';
  }
  if (file.startsWith('scripts/')) return scriptGroup(file);
  if (file.startsWith('docs/how-tsz-works/')) return 'how-tsz-works-docs';
  if (file.startsWith('docs/')) return 'existing-docs';
  if (file.startsWith('.github/')) return 'github-and-ci-config';
  if (file.startsWith('.agents/') || file.startsWith('.claude/') || file.startsWith('.codex/')) {
    return 'agent-and-editor-workflows';
  }
  if (file.startsWith('.cargo/') || file.startsWith('.config/') || file.startsWith('.vscode/')) {
    return 'root-and-local-tool-config';
  }
  return 'root-and-local-tool-config';
}

function titleForGroup(slug) {
  return slug
    .split('-')
    .map((word) => word[0].toUpperCase() + word.slice(1))
    .join(' ');
}

function roleForFile(file) {
  const name = path.basename(file);
  const ext = path.extname(file);
  const crate = crateName(file);
  if (file === 'Cargo.toml') return 'Workspace manifest, crate membership, shared dependencies, and build profiles.';
  if (file === 'Cargo.lock') return 'Locked Rust dependency graph used by local and CI builds.';
  if (file === 'AGENTS.md') return 'Repository contract for agent behavior, architecture, verification, and PR coordination.';
  if (file === 'README.md') return 'Project entrypoint and public overview.';
  if (file.startsWith('crates/') && name === 'Cargo.toml') return `Manifest for the \`${crate}\` crate.`;
  if (file.startsWith('crates/') && name === 'lib.rs') return `Public facade and module wiring for the \`${crate}\` crate.`;
  if (file.startsWith('crates/') && name === 'main.rs') return `Binary entrypoint wiring for the \`${crate}\` crate.`;
  if (file.includes('/src/bin/')) return `Rust binary entrypoint associated with the \`${crate}\` crate.`;
  if (file.startsWith('crates/tsz-checker/src/') && file.includes('query_boundaries/')) {
    return 'Checker-to-solver query boundary code; keep semantic answers routed through structured requests.';
  }
  if (file.startsWith('crates/tsz-checker/src/') && file.includes('assignability/')) {
    return 'Checker-side assignability orchestration, diagnostic mapping, or relation-gateway support.';
  }
  if (file.startsWith('crates/tsz-solver/src/') && file.includes('relations/')) {
    return 'Solver relation logic for subtype, assignability, compatibility, or relation failure reasons.';
  }
  if (file.startsWith('crates/tsz-solver/src/') && file.includes('evaluation/')) {
    return 'Solver type evaluation and TypeScript type-operator semantics.';
  }
  if (file.startsWith('crates/tsz-emitter/src/') && file.includes('declaration_emitter/')) {
    return 'Declaration emit implementation, usage analysis, or declaration-emitter tests.';
  }
  if (file.startsWith('crates/tsz-emitter/src/') && file.includes('transforms/')) {
    return 'Emitter transform or intermediate-representation support.';
  }
  if (file.startsWith('crates/tsz-website/src/lib/')) return 'Bundled TypeScript lib declaration for the browser/playground site.';
  if (file.startsWith('crates/') && file.includes('/tests/')) return `Integration test or fixture for the \`${crate}\` crate.`;
  if (file.startsWith('crates/') && file.includes('/benches/')) return `Benchmark source for the \`${crate}\` crate.`;
  if (file.startsWith('crates/') && ext === '.rs') return `Rust source or test module in the \`${crate}\` crate.`;
  if (file.startsWith('scripts/bench/')) return 'Benchmark, project-row, fixture, or performance-measurement tooling.';
  if (file.startsWith('scripts/ci/')) return 'CI orchestration, guard, artifact, or project-compatibility tooling.';
  if (file.startsWith('scripts/conformance/')) return 'Diagnostic conformance snapshot, query, cache, or analysis tooling.';
  if (file.startsWith('scripts/emit/')) return 'JavaScript/declaration emit harness, baseline, or output-surgery tooling.';
  if (file.startsWith('scripts/fourslash/')) return 'Fourslash language-service harness or adapter tooling.';
  if (file.startsWith('scripts/arch/')) return 'Architecture guard, ratchet, or source-boundary tooling.';
  if (file.startsWith('scripts/setup/')) return 'Local setup, TypeScript submodule, disk, or worktree hygiene tooling.';
  if (file.startsWith('.github/workflows/')) return 'GitHub Actions workflow for CI, release, benchmark, or repository automation.';
  if (file.startsWith('.agents/skills/')) return 'Repo-local agent skill, reference, or helper script.';
  if (ext === '.md') return 'Markdown documentation or generated guide page.';
  if (ext === '.yml' || ext === '.yaml' || ext === '.toml' || ext === '.json') return 'Configuration, manifest, snapshot, or metadata file.';
  if (ext === '.sh' || ext === '.py' || ext === '.mjs' || ext === '.cjs' || ext === '.ts' || ext === '.js') {
    return 'Repository automation, tooling, harness, or application source.';
  }
  if (ext === '.d.ts') return 'TypeScript declaration input used by site, harnesses, or compatibility checks.';
  if (ext === '.png' || ext === '.svg') return 'Static image or visual asset.';
  return 'Repository file referenced by the TSZ guide inventory.';
}

function splitLargeGroup(slug, files) {
  if (files.length <= maxEntriesPerPage) return [[slug, files]];
  const pages = [];
  for (let i = 0; i < files.length; i += maxEntriesPerPage) {
    const page = Math.floor(i / maxEntriesPerPage) + 1;
    const padded = String(page).padStart(2, '0');
    pages.push([`${slug}-${padded}`, files.slice(i, i + maxEntriesPerPage)]);
  }
  return pages;
}

function markdownForPage(slug, files) {
  const title = titleForGroup(slug.replace(/-\d\d$/, ''));
  const lines = [
    `# ${title}`,
    '',
    `Generated by \`scripts/docs/generate-how-tsz-inventory.mjs\`. This page mentions ${files.length} repository file${files.length === 1 ? '' : 's'}.`,
    '',
    '| Path | Role |',
    '| --- | --- |',
  ];
  for (const file of files) {
    lines.push(`| \`${file}\` | ${roleForFile(file)} |`);
  }
  lines.push('');
  return lines.join('\n');
}

const grouped = new Map();
for (const file of allFiles) {
  const slug = inventoryGroup(file);
  const files = grouped.get(slug) ?? [];
  files.push(file);
  grouped.set(slug, files);
}

mkdirSync(outDir, { recursive: true });
rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

const pages = [];
for (const [slug, files] of [...grouped.entries()].sort(([a], [b]) => a.localeCompare(b))) {
  for (const [pageSlug, pageFiles] of splitLargeGroup(slug, files)) {
    const outputPath = path.join(outDir, `${pageSlug}.md`);
    pages.push({ slug: pageSlug, files: pageFiles, outputPath });
  }
}

const inventoryReadmePath = path.join(outDir, 'README.md');
pages.push({ slug: 'README', files: ['docs/how-tsz-works/file-inventory/README.md'], outputPath: inventoryReadmePath });

for (const page of pages.filter((entry) => entry.slug !== 'README')) {
  writeFileSync(page.outputPath, markdownForPage(page.slug, page.files));
}

const readmeLines = [
  '# File Inventory',
  '',
  'This directory is the mechanically generated companion to the narrative `docs/how-tsz-works/` guide. It exists for one reason: every repository file should be easy to find from the documentation.',
  '',
  'Regenerate it with:',
  '',
  '```bash',
  'node scripts/docs/generate-how-tsz-inventory.mjs',
  '```',
  '',
  'Then verify coverage with:',
  '',
  '```bash',
  'node scripts/docs/check-how-tsz-docs-coverage.mjs',
  '```',
  '',
  `The latest generation saw ${allFiles.length} repository files before writing the inventory pages.`,
  '',
  '## Inventory Pages',
  '',
];

for (const page of pages.filter((entry) => entry.slug !== 'README')) {
  const rel = path.relative(repoRoot, page.outputPath);
  readmeLines.push(`- [${titleForGroup(page.slug)}](${path.basename(page.outputPath)}) - mentions ${page.files.length} file${page.files.length === 1 ? '' : 's'}; path: \`${rel}\`.`);
}

readmeLines.push('- [File Inventory](README.md) - this index; path: `docs/how-tsz-works/file-inventory/README.md`.');
readmeLines.push('');

writeFileSync(inventoryReadmePath, readmeLines.join('\n'));

console.log(`wrote ${pages.length} inventory files under ${path.relative(repoRoot, outDir)}`);
