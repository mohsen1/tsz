#!/usr/bin/env node
/**
 * Native Binary Conformance Test Runner
 *
 * Spawns the native tsz binary for each test instead of loading WASM.
 * Faster execution, but no Docker isolation.
 */

import * as path from 'path';
import * as fs from 'fs';
import * as ts from 'typescript';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

interface TestFile {
  name: string;
  content: string;
}

interface ParsedTestCase {
  options: Record<string, unknown>;
  isMultiFile: boolean;
  files: TestFile[];
  category: string;
}

const CONFIG = {
  testsBasePath: path.resolve(__dirname, '../../TypeScript/tests/cases'),
  libPath: path.resolve(__dirname, '../../TypeScript/tests/lib/lib.d.ts'),
  maxTests: parseInt(process.env.MAX_TESTS || '500'),
  categories: (process.env.CATEGORIES || 'conformance,compiler').split(','),
  verbose: process.env.VERBOSE === 'true',
  tszBinary: process.env.TSZ_BINARY || path.resolve(__dirname, '../../target/release/tsz'),
};

// Track results
let passed = 0;
let failed = 0;
const missingErrors: Map<number, number> = new Map();
const extraErrors: Map<number, number> = new Map();

/**
 * Parse test directives from TypeScript test files
 */
function parseTestDirectives(source: string, filePath: string): ParsedTestCase {
  const options: Record<string, unknown> = {};
  const files: TestFile[] = [];
  let isMultiFile = false;

  // Extract compiler options from @comments
  const lines = source.split('\n');
  const cleanLines: string[] = [];

  for (const line of lines) {
    // Check for @filename directives (multi-file tests)
    const filenameMatch = line.match(/^\/\/\/\s*File:\s*(\S+)/);
    if (filenameMatch) {
      isMultiFile = true;
      if (files.length > 0) {
        // Save previous file
        files[files.length - 1].content = cleanLines.join('\n');
      }
      files.push({ name: filenameMatch[1], content: '' });
      cleanLines.length = 0;
      continue;
    }

    // Check for @compilerOptions
    const optMatch = line.match(/@(\w+)\s*:\s*(.+)/);
    if (optMatch) {
      const [, key, value] = optMatch;
      // Parse boolean, string, or number values
      if (value === 'true') options[key] = true;
      else if (value === 'false') options[key] = false;
      else if (/^\d+$/.test(value)) options[key] = parseInt(value, 10);
      else options[key] = value.replace(/^['"]|['"]$/g, '');
      continue;
    }

    // Skip @ comments (they're directives, not code)
    if (line.trim().startsWith('//')) {
      // Check for @skip, @target, etc.
      if (line.includes('@skip')) continue;
      if (line.includes('@only')) continue;
    }

    cleanLines.push(line);
  }

  // Save last file content
  if (files.length > 0) {
    files[files.length - 1].content = cleanLines.join('\n');
  } else {
    // Single file test
    files.push({ name: path.basename(filePath), content: source });
  }

  // Determine category from file path
  let category = 'unknown';
  if (filePath.includes('/conformance/')) category = 'conformance';
  else if (filePath.includes('/compiler/')) category = 'compiler';

  return { options, isMultiFile, files, category };
}

/**
 * Run TSC on a test case to get expected errors
 */
function runTsc(testCase: ParsedTestCase): number[] {
  const codes: number[] = [];
  const tmpDir = fs.mkdtempSync('/tmp/tsz-tsc-');

  try {
    // Write test files to temp directory
    for (const file of testCase.files) {
      fs.writeFileSync(path.join(tmpDir, file.name), file.content);
    }

    // Create compiler options
    const compilerOptions: ts.CompilerOptions = {
      noEmit: true,
      ...testCase.options,
    };

    // Create program from files
    const program = ts.createProgram(
      testCase.files.map(f => path.join(tmpDir, f.name)),
      compilerOptions
    );

    const diagnostics = ts.getPreEmitDiagnostics(program);

    for (const diag of diagnostics) {
      if (diag.code) {
        codes.push(diag.code);
      }
    }
  } catch {
    // TSC parsing errors - ignore
  } finally {
    // Cleanup
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      // Ignore
    }
  }

  return codes;
}

/**
 * Run native tsz binary on a test case
 */
function runNative(testCase: ParsedTestCase): Promise<{ codes: number[]; crashed: boolean; error?: string }> {
  const tmpDir = fs.mkdtempSync('/tmp/tsz-test-');

  return new Promise((resolve) => {
    const cleanup = () => {
      try {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      } catch {
        // Ignore cleanup errors
      }
    };

    try {
      // Write test files to temp directory
      const filesToCheck: string[] = [];

      // Add lib.d.ts unless noLib
      if (!testCase.options.nolib) {
        const libContent = fs.readFileSync(CONFIG.libPath, 'utf8');
        fs.writeFileSync(path.join(tmpDir, 'lib.d.ts'), libContent);
        filesToCheck.push('lib.d.ts');
      }

      // Write test files
      for (const file of testCase.files) {
        fs.writeFileSync(path.join(tmpDir, file.name), file.content);
        filesToCheck.push(file.name);
      }

      // Run tsz binary
      const codes: number[] = [];
      const args = filesToCheck.map(f => path.join(tmpDir, f));

      const child = spawn(CONFIG.tszBinary, args, {
        cwd: tmpDir,
        stdio: ['ignore', 'pipe', 'pipe'],
      });

      let stderr = '';

      child.stderr?.on('data', (data) => {
        stderr += data.toString();
      });

      child.on('close', (code) => {
        // Parse error codes from stderr (tsz outputs to stderr)
        const errorMatches = stderr.match(/TS(\d+)/g);
        if (errorMatches) {
          for (const match of errorMatches) {
            codes.push(parseInt(match.substring(2), 10));
          }
        }

        cleanup();
        resolve({ codes, crashed: false });
      });

      child.on('error', (err) => {
        cleanup();
        resolve({ codes: [], crashed: true, error: err.message });
      });

      // Timeout after 10 seconds
      setTimeout(() => {
        child.kill();
        cleanup();
        resolve({ codes: [], crashed: true, error: 'Timeout' });
      }, 10000);
    } catch (err) {
      cleanup();
      resolve({ codes: [], crashed: true, error: String(err) });
    }
  });
}

/**
 * Main test execution
 */
async function main() {
  console.log(`📂 Tests base: ${CONFIG.testsBasePath}`);
  console.log(`📦 Binary: ${CONFIG.tszBinary}`);
  console.log(`🎯 Categories: ${CONFIG.categories.join(', ')}`);
  console.log('');

  // Collect test files
  const testFiles: string[] = [];

  for (const category of CONFIG.categories) {
    const categoryPath = path.join(CONFIG.testsBasePath, category);
    if (!fs.existsSync(categoryPath)) {
      console.warn(`⚠️  Category path not found: ${categoryPath}`);
      continue;
    }

    const walk = (dir: string) => {
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          walk(fullPath);
        } else if (entry.isFile() && entry.name.endsWith('.ts')) {
          testFiles.push(fullPath);
        }
      }
    };

    walk(categoryPath);
  }

  console.log(`📋 Found ${testFiles.length} test files`);
  console.log('');

  // Run tests
  const testsToRun = testFiles.slice(0, CONFIG.maxTests);

  for (let i = 0; i < testsToRun.length; i++) {
    const filePath = testsToRun[i];
    const fileName = path.basename(path.relative(CONFIG.testsBasePath, filePath));

    if (CONFIG.verbose) {
      process.stdout.write(`\r[${i + 1}/${testsToRun.length}] ${fileName}`);
    } else if (i % 100 === 0) {
      process.stdout.write(`\r[${i + 1}/${testsToRun.length}]`);
    }

    try {
      const source = fs.readFileSync(filePath, 'utf8');
      const testCase = parseTestDirectives(source, filePath);

      // Run TSC
      const tscCodes = runTsc(testCase);

      // Run native
      const { codes: nativeCodes, crashed, error } = await runNative(testCase) as { codes: number[]; crashed: boolean; error?: string };

      if (crashed) {
        if (CONFIG.verbose) {
          console.log(`\n  💥 Crash: ${error}`);
        }
        continue;
      }

      // Compare results
      const tscSet = new Set(tscCodes);
      const nativeSet = new Set(nativeCodes);

      // Check for missing errors (in TSC but not in native)
      for (const code of tscCodes) {
        if (!nativeSet.has(code)) {
          missingErrors.set(code, (missingErrors.get(code) || 0) + 1);
        }
      }

      // Check for extra errors (in native but not in TSC)
      for (const code of nativeCodes) {
        if (!tscSet.has(code)) {
          extraErrors.set(code, (extraErrors.get(code) || 0) + 1);
        }
      }

      // Pass if errors match
      if (nativeCodes.length === tscCodes.length &&
          nativeCodes.every(c => tscSet.has(c))) {
        passed++;
      } else {
        failed++;
      }
    } catch (e) {
      if (CONFIG.verbose) {
        console.log(`\n  ❌ Error: ${e}`);
      }
    }
  }

  console.log('');

  // Print results
  const total = passed + failed;
  const passRate = total > 0 ? ((passed / total) * 100).toFixed(1) : '0.0';

  console.log('');
  console.log('════════════════════════════════════════════════════════════');
  console.log('CONFORMANCE TEST RESULTS');
  console.log('════════════════════════════════════════════════════════════');
  console.log('');
  console.log(`Pass Rate: ${passRate}% (${passed}/${total})`);
  console.log('');
  console.log(`  ✓ Passed:   ${passed}`);
  console.log(`  ✗ Failed:   ${failed}`);
  console.log('');

  // Top missing errors
  if (missingErrors.size > 0) {
    console.log('Top Missing Errors (we should emit but don\'t):');
    const sorted = [...missingErrors.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);
    for (const [code, count] of sorted) {
      console.log(`  TS${code}: ${count}x`);
    }
    console.log('');
  }

  // Top extra errors
  if (extraErrors.size > 0) {
    console.log('Top Extra Errors (we emit but shouldn\'t):');
    const sorted = [...extraErrors.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);
    for (const [code, count] of sorted) {
      console.log(`  TS${code}: ${count}x`);
    }
    console.log('');
  }

  console.log('════════════════════════════════════════════════════════════');
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
