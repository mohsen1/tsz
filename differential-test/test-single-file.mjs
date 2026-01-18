#!/usr/bin/env node
/**
 * Quick test for a single conformance file
 */

import { readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const TEST_FILE = process.argv[2] || 'TypeScript/tests/cases/conformance/expressions/nullishCoalescingOperator/nullishCoalescingOperator_not_strict.ts';

async function main() {
  console.log('Testing file:', TEST_FILE);

  const code = readFileSync(TEST_FILE, 'utf-8');
  console.log('\nCode preview:');
  console.log(code.substring(0, 200));

  const wasmPkgPath = join(__dirname, '../pkg');
  const wasmModule = await import(join(wasmPkgPath, 'wasm.js'));

  const parser = new wasmModule.ThinParser('test.ts', code);

  // Check if file has @strict: false directive
  const hasStrictFalse = code.includes('@strict: false');
  console.log('\n@strict: false directive present:', hasStrictFalse);

  if (hasStrictFalse) {
    parser.setCompilerOptions(JSON.stringify({ strict: false }));
    console.log('Set compiler options to strict: false');
  }

  parser.parseSourceFile();
  const checkResultJson = parser.checkSourceFile();
  const checkResult = JSON.parse(checkResultJson);
  const diags = checkResult.diagnostics || [];

  console.log('\nDiagnostics found:', diags.length);
  if (diags.length > 0) {
    console.log('Error codes:', diags.map(d => `TS${d.code}`).join(', '));
  }

  parser.free();
  console.log('\nTest completed successfully!');
}

main().catch(e => {
  console.error('Error:', e);
  process.exit(1);
});
