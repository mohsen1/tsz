#!/usr/bin/env node
/**
 * Quick test for a single conformance file
 */

import { readFileSync } from 'fs';
import { join, dirname, basename } from 'path';
import { fileURLToPath } from 'url';
import { parseTestDirectives, mapToWasmCompilerOptions } from './directive-parser.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const TEST_FILE = process.argv[2] || '/Users/mohsenazimi/code/orchestrator-config/tsz-workspace/worktrees/em-3/ts-tests/cases/conformance/expressions/nullishCoalescingOperator/nullishCoalescingOperator_not_strict.ts';

async function main() {
  console.log('Testing file:', TEST_FILE);
  const fileName = basename(TEST_FILE);

  const rawCode = readFileSync(TEST_FILE, 'utf-8');

  // Parse test directives from source code using centralized parser
  const { options, isMultiFile, cleanCode, files } = parseTestDirectives(rawCode);

  console.log('\nParsed directives:', JSON.stringify(options, null, 2));
  console.log('Is multi-file test:', isMultiFile);

  if (isMultiFile && files.length > 0) {
    console.log('\nFiles in multi-file test:');
    for (const file of files) {
      console.log(`  - ${file.name}`);
    }
    console.log('\nSkipping multi-file test for now.');
    return;
  }

  console.log('\nCode preview (with directives removed):');
  console.log(cleanCode.substring(0, 200));

  const wasmPkgPath = join(__dirname, '../pkg');
  const wasmModule = await import(join(wasmPkgPath, 'wasm.js'));

  const parser = new wasmModule.ThinParser(fileName, cleanCode);

  // Build compiler options from test directives using the centralized mapper
  const wasmOptions = mapToWasmCompilerOptions(options);

  // Handle strict mode default: if strict is not explicitly set, default to false
  if (options.strict === undefined) {
    wasmOptions.strict = false;
  }

  console.log('\nWASM compiler options:', JSON.stringify(wasmOptions, null, 2));

  // Apply compiler options to WASM parser
  if (Object.keys(wasmOptions).length > 0) {
    parser.setCompilerOptions(JSON.stringify(wasmOptions));
  }

  parser.parseSourceFile();
  const checkResultJson = parser.checkSourceFile();
  const checkResult = JSON.parse(checkResultJson);
  const diags = checkResult.diagnostics || [];

  console.log('\nDiagnostics found:', diags.length);
  if (diags.length > 0) {
    console.log('Error codes:', diags.map(d => `TS${d.code}`).join(', '));
    for (const d of diags.slice(0, 5)) {
      console.log(`  TS${d.code}: ${d.message_text?.substring(0, 80) || 'No message'}`);
    }
    if (diags.length > 5) {
      console.log(`  ... and ${diags.length - 5} more`);
    }
  }

  parser.free();
  console.log('\nTest completed successfully!');
}

main().catch(e => {
  console.error('Error:', e);
  process.exit(1);
});
