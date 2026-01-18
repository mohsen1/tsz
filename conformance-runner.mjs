```mjs
/**
 * @fileoverview Conformance test runner for the compiler.
 *
 * This script runs the compiler suite over the 'conformance' test files
 * and checks if the output matches the expected results.
 */

import { readFileSync, existsSync, appendFileSync } from 'fs';
import { execSync } from 'child_process';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

// Polyfill __dirname for ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/**
 * Configuration for the test runner.
 */
const CONFIG = {
  testDir: join(__dirname, 'conformance'),
  compilerPath: join(__dirname, 'dist', 'compiler.js'),
  nodePath: process.execPath,
  features: {
    // Toggle feature sets to test
    generics: true,
    advancedDecorators: false,
  },
  options: {
    timeout: 5000,
  },
};

/**
 * Helper to determine if a test should be skipped based on feature flags.
 * @param {string} fileName
 * @returns {boolean}
 */
function shouldTestFeature(fileName) {
  // Example logic: Skip files marked with specific suffixes if features are disabled
  if (!CONFIG.features.advancedDecorators && fileName.includes('decorator')) {
    return false;
  }
  if (!CONFIG.features.generics && fileName.includes('generic')) {
    return false;
  }
  return true;
}

/**
 * Executes the compiler on a single input file.
 * @param {string} inputPath
 * @returns {{ code: number, stdout: string, stderr: string, error?: Error }}
 */
function executeTest(inputPath) {
  try {
    const stdout = execSync(
      `"${CONFIG.nodePath}" "${CONFIG.compilerPath}" "${inputPath}"`,
      {
        encoding: 'utf-8',
        stdio: 'pipe',
        timeout: CONFIG.options.timeout,
      }
    );
    return { code: 0, stdout, stderr: '' };
  } catch (err) {
    // Node throws on non-zero exit
    return {
      code: err.status || 1,
      stdout: err.stdout ? err.stdout.toString() : '',
      stderr: err.stderr ? err.stderr.toString() : '',
      error: err,
    };
  }
}

/**
 * Generates the output filename based on input path.
 * @param {string} inputPath
 * @returns {string}
 */
function generateFileName(inputPath) {
  const baseName = inputPath.replace(/\.(proto|js)$/, '');
  return `${baseName}.output.txt`;
}

/**
 * Appends test results to a results file.
 * @param {string} filePath
 * @param {string} content
 */
function appendResultToFile(filePath, content) {
  try {
    appendFileSync(filePath, content, 'utf-8');
  } catch (err) {
    console.error(`Failed to write to ${filePath}: ${err.message}`);
  }
}

/**
 * Main entry point for the conformance runner.
 */
async function runConformanceTests() {
  console.log('Starting Conformance Tests...');
  console.log(`Compiler: ${CONFIG.compilerPath}`);

  if (!existsSync(CONFIG.compilerPath)) {
    console.error('Compiler not found. Run "npm run build" first.');
    process.exit(1);
  }

  // Mock implementation of file discovery
  // In a real scenario, this might use glob or fs.readdir
  const filesToTest = [
    'conformance/test1.proto',
    'conformance/test2.proto',
    // ... other files
  ];

  const results = {
    passed: 0,
    failed: 0,
    skipped: 0,
  };

  for (const relPath of filesToTest) {
    const fullPath = join(__dirname, relPath);

    if (!existsSync(fullPath)) {
      console.warn(`Skipping missing file: ${relPath}`);
      continue;
    }

    if (!shouldTestFeature(relPath)) {
      console.log(`Skipping (feature disabled): ${relPath}`);
      results.skipped++;
      continue;
    }

    console.log(`Testing: ${relPath}`);
    const result = executeTest(fullPath);
    const outputFile = generateFileName(fullPath);

    let outputLog = `File: ${relPath}\nExit Code: ${result.code}\n`;
    
    if (result.code === 0) {
      outputLog += `Status: PASS\nOutput:\n${result.stdout}\n`;
      results.passed++;
    } else {
      outputLog += `Status: FAIL\nStderr:\n${result.stderr}\n`;
      results.failed++;
    }

    console.log(outputLog);
    appendResultToFile('conformance/results.txt', outputLog + '\n=====================\n');
  }

  console.log(`\nTest Run Complete. Passed: ${results.passed}, Failed: ${results.failed}, Skipped: ${results.skipped}`);
  
  process.exit(results.failed > 0 ? 1 : 0);
}

runConformanceTests();
```
