/**
 * Simple conformance test to measure error recovery impact
 */

import { readFileSync, readdirSync, statSync } from 'fs';
import { join, basename } from 'path';
import { parseTestDirectives, mapToWasmCompilerOptions } from './directive-parser.mjs';

const CONFIG = {
  wasmPkgPath: join(process.cwd(), '../pkg'),
  conformanceDir: join(process.cwd(), '../../tests/cases/conformance'),
};

// Get test files
function getTestFiles(dir, maxFiles = 200) {
  const files = [];
  function walk(currentDir) {
    if (files.length >= maxFiles) return;
    try {
      const entries = readdirSync(currentDir);
      for (const entry of entries) {
        if (files.length >= maxFiles) break;
        const fullPath = join(currentDir, entry);
        try {
          const stat = statSync(fullPath);
          if (stat.isDirectory()) {
            walk(fullPath);
          } else if (entry.endsWith('.ts') && !entry.endsWith('.d.ts')) {
            files.push(fullPath);
          }
        } catch (e) {
          // Skip files we can't read
        }
      }
    } catch (e) {
      // Skip directories we can't read
    }
  }
  walk(dir);
  return files;
}

async function main() {
  console.log('Simple Conformance Test - Error Recovery Impact');
  console.log('='.repeat(60));

  const wasm = await import(CONFIG.wasmPkgPath + '/wasm.js');

  const testFiles = getTestFiles(CONFIG.conformanceDir, 50);
  console.log(`Found ${testFiles.length} test files\n`);

  const results = {
    total: 0,
    success: 0,
    withErrors: 0,
    failed: 0,
    totalDiagnostics: 0,
    totalNodes: 0
  };

  for (let i = 0; i < testFiles.length; i++) {
    const filePath = testFiles[i];
    const fileName = basename(filePath);
    const relPath = filePath.replace(CONFIG.conformanceDir + '/', '');

    try {
      const rawCode = readFileSync(filePath, 'utf-8');

      // Parse test directives from source code
      const { options, isMultiFile, cleanCode, files } = parseTestDirectives(rawCode);

      // Skip multi-file tests
      if (isMultiFile && files.length > 0) {
        continue;
      }

      const parser = new wasm.ThinParser(fileName, cleanCode);

      // Build compiler options from test directives using the centralized mapper
      const wasmOptions = mapToWasmCompilerOptions(options);

      // Handle strict mode default: if strict is not explicitly set, default to false
      if (options.strict === undefined) {
        wasmOptions.strict = false;
      }

      // Apply compiler options to WASM parser
      if (Object.keys(wasmOptions).length > 0) {
        parser.setCompilerOptions(JSON.stringify(wasmOptions));
      }

      parser.parseSourceFile();

      const diagsJson = parser.getDiagnosticsJson();
      const diags = JSON.parse(diagsJson);

      const nodeCount = parser.getNodeCount();

      results.total++;
      results.totalNodes += nodeCount;
      results.totalDiagnostics += diags.length;

      if (diags.length > 0) {
        results.withErrors++;
      } else {
        results.success++;
      }

      if ((i + 1) % 10 === 0 || i === testFiles.length - 1) {
        process.stdout.write(`\r  Progress: ${i + 1}/${testFiles.length}`);
      }

      parser.free();
    } catch (e) {
      results.failed++;
      console.log(`\n  FAILED: ${relPath}`);
      console.log(`    Error: ${e.message?.substring(0, 100)}`);
    }
  }

  console.log('\n\n' + '='.repeat(60));
  console.log('RESULTS');
  console.log('='.repeat(60));
  console.log(`Total Tests:      ${results.total}`);
  console.log(`Success (0 diag):  ${results.success} (${(results.success/results.total*100).toFixed(1)}%)`);
  console.log(`With Errors:      ${results.withErrors} (${(results.withErrors/results.total*100).toFixed(1)}%)`);
  console.log(`Failed to Parse:  ${results.failed}`);
  console.log(`\nTotal Nodes:      ${results.totalNodes}`);
  console.log(`Total Diagnostics:${results.totalDiagnostics}`);
  console.log(`Avg Nodes/File:   ${(results.totalNodes/results.total).toFixed(1)}`);
  console.log(`\nError Recovery:   ${results.failed === 0 ? 'WORKING ✓' : 'NEEDS IMPROVEMENT'}`);
}

main().catch(e => {
  console.error('Fatal error:', e);
  process.exit(1);
});
