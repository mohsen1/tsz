/**
 * Worker thread for TSC cache generation
 *
 * Runs TypeScript compiler on test files and returns diagnostic codes.
 * Uses TypeScript's harness infrastructure for consistent parsing with conformance tests.
 */

import { parentPort, workerData } from 'worker_threads';
import * as fs from 'fs';
import { parseTestCase, shouldSkipTest } from './test-utils.js';
import { runTsc, type ParsedTestCase as TscParsedTestCase } from './tsc-runner.js';

const libSource: string = workerData.libSource;
const libDir: string = workerData.libDir;

parentPort!.on('message', (msg: { id: number; filePath: string }) => {
  try {
    const code = fs.readFileSync(msg.filePath, 'utf8');
    
    // Use the shared parseTestCase which uses TypeScript's harness
    const parsed = parseTestCase(code, msg.filePath);
    
    // Check if test should be skipped (noCheck, unsupported version, etc.)
    const skipResult = shouldSkipTest(parsed.harness);
    if (skipResult.skip) {
      // Return empty codes for skipped tests - they won't be compared
      parentPort!.postMessage({ id: msg.id, codes: [], skipped: true, reason: skipResult.reason });
      return;
    }
    
    // Convert to tsc-runner format (directives -> options)
    const testCase: TscParsedTestCase = {
      options: parsed.directives as Record<string, unknown>,
      isMultiFile: parsed.isMultiFile,
      files: parsed.files,
    };
    
    const result = runTsc(testCase, libDir, libSource, false, msg.filePath);
    parentPort!.postMessage({ id: msg.id, codes: result.codes });
  } catch (e) {
    const errorMsg = e instanceof Error ? e.message : String(e);
    parentPort!.postMessage({
      id: msg.id,
      codes: [],
      error: errorMsg,
    });
  }
});

parentPort!.postMessage({ type: 'ready' });
