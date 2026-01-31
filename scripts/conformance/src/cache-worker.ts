/**
 * Worker thread for TSC cache generation
 *
 * Runs TypeScript compiler on test files and returns diagnostic codes.
 * Uses shared tsc-runner module for TSC execution.
 */

import { parentPort, workerData } from 'worker_threads';
import * as fs from 'fs';
import { parseTestDirectives, runTsc } from './tsc-runner.js';

const libSource: string = workerData.libSource;
const libDir: string = workerData.libDir;

parentPort!.on('message', (msg: { id: number; filePath: string }) => {
  try {
    const code = fs.readFileSync(msg.filePath, 'utf8');
    const testCase = parseTestDirectives(code, msg.filePath);
    const result = runTsc(testCase, libDir, libSource, false);
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
