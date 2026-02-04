/**
 * Emit worker thread - runs transpile in isolation with timeout protection
 */

import { parentPort, workerData } from 'worker_threads';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);

interface TranspileJob {
  id: number;
  source: string;
  target: number;
  module: number;
  declaration?: boolean;
}

interface TranspileResult {
  id: number;
  output?: string;
  declaration?: string | null;
  error?: string;
}

let wasm: any = null;

// Initialize WASM module
try {
  wasm = require(workerData.wasmPath);
  parentPort?.postMessage({ type: 'ready' });
} catch (e) {
  parentPort?.postMessage({ type: 'error', error: String(e) });
  process.exit(1);
}

// Handle uncaught exceptions
process.on('uncaughtException', (err) => {
  parentPort?.postMessage({ type: 'crash', error: err.message });
  process.exit(1);
});

// Process transpile jobs
parentPort?.on('message', (job: TranspileJob) => {
  try {
    const options = {
      target: job.target,
      module: job.module,
      declaration: job.declaration || false,
    };
    const optionsJson = JSON.stringify(options);
    const result = JSON.parse(wasm.transpileModule(job.source, optionsJson));

    parentPort?.postMessage({
      id: job.id,
      output: result.output_text,
      declaration: result.declarationText || null
    } as TranspileResult);
  } catch (e) {
    parentPort?.postMessage({
      id: job.id,
      error: e instanceof Error ? e.message : String(e)
    } as TranspileResult);
  }
});
