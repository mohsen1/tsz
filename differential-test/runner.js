#!/usr/bin/env node
/**
 * Differential Testing Harness for TypeScript vs Rust/WASM Compiler
 *
 * This harness runs the same TypeScript code through both compilers and
 * compares their diagnostic outputs to identify semantic parity issues.
 *
 * Usage:
 *   node runner.js                    # Run all tests
 *   node runner.js --catalog          # Run tests from unsoundness catalog
 *   node runner.js --verbose          # Show detailed output
 *   node runner.js --report-only      # Just show last report
 */

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, join, resolve } from 'path';
import { readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync } from 'fs';
import { execSync, spawn } from 'child_process';

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Configuration
const CONFIG = {
  wasmPkgPath: resolve(__dirname, '../pkg'),
  testCasesDir: resolve(__dirname, 'test-cases'),
  catalogPath: resolve(__dirname, '../specs/TS_UNSOUNDNESS_CATALOG.md'),
  repoTestsDir: resolve(__dirname, '../../tests/cases'),
  outputDir: resolve(__dirname, 'output'),
  reportPath: resolve(__dirname, 'output/report.json'),
};
const DEFAULT_LIB_PATH = resolve(__dirname, '../../tests/lib/lib.d.ts');
const DEFAULT_LIB_SOURCE = readFileSync(DEFAULT_LIB_PATH, 'utf-8');
const DEFAULT_LIB_NAME = 'lib.d.ts';

// ANSI colors
const colors = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
};

function log(msg, color = '') {
  console.log(`${color}${msg}${colors.reset}`);
}

// ============================================================================
// Test Case Parser - Extracts test cases from TS_UNSOUNDNESS_CATALOG.md
// ============================================================================

function parseUnsoundnessCatalog() {
  const catalog = readFileSync(CONFIG.catalogPath, 'utf-8');
  const testCases = [];

  // Parse each numbered section
  const sectionRegex = /## (\d+)\. ([^\n]+)\n([\s\S]*?)(?=## \d+\.|## Implementation Priority|$)/g;
  let match;

  while ((match = sectionRegex.exec(catalog)) !== null) {
    const [, number, title, content] = match;

    // Extract example file references
    const exampleMatch = content.match(/\*\*Example:\*\*\s*`([^`]+)`/);
    const exampleFile = exampleMatch ? exampleMatch[1] : null;

    // Extract inline code examples
    const codeBlocks = [];
    const codeBlockRegex = /```typescript\n([\s\S]*?)```/g;
    let codeMatch;
    while ((codeMatch = codeBlockRegex.exec(content)) !== null) {
      codeBlocks.push(codeMatch[1].trim());
    }

    // Extract solver rule info
    const solverRuleMatch = content.match(/\*\*Solver Rule:\*\*([\s\S]*?)(?=\*\*|$)/);
    const solverRule = solverRuleMatch ? solverRuleMatch[1].trim() : '';

    // Determine phase from Implementation Priority section
    let phase = 4; // Default to phase 4
    if (number <= 5 || ['1', '3', '6', '11', '20'].includes(number)) phase = 1;
    else if (['2', '4', '10', '14', '19'].includes(number)) phase = 2;
    else if (['21', '25', '30', '40', '41'].includes(number)) phase = 3;

    testCases.push({
      id: `unsoundness-${number}`,
      title: title.trim(),
      category: 'unsoundness',
      phase,
      exampleFile,
      inlineCode: codeBlocks,
      solverRule,
      expectedBehavior: content.match(/\*\*Behavior:\*\*([\s\S]*?)(?=\*\*Example|\*\*Solver)/)?.[1]?.trim() || '',
    });
  }

  return testCases;
}

// ============================================================================
// Test Case Generator - Creates concrete test files from catalog entries
// ============================================================================

function generateTestCases(catalogEntries) {
  const testCases = [];

  for (const entry of catalogEntries) {
    // Use inline code examples if available
    if (entry.inlineCode.length > 0) {
      for (let i = 0; i < entry.inlineCode.length; i++) {
        testCases.push({
          id: `${entry.id}-inline-${i + 1}`,
          title: `${entry.title} (inline example ${i + 1})`,
          code: entry.inlineCode[i],
          phase: entry.phase,
          category: entry.category,
          expectedBehavior: entry.expectedBehavior,
        });
      }
    }

    // Try to load referenced test file
    if (entry.exampleFile) {
      const fullPath = resolve(CONFIG.repoTestsDir, '..', entry.exampleFile);
      if (existsSync(fullPath)) {
        try {
          const code = readFileSync(fullPath, 'utf-8');
          testCases.push({
            id: `${entry.id}-file`,
            title: `${entry.title} (from ${entry.exampleFile})`,
            code,
            phase: entry.phase,
            category: entry.category,
            expectedBehavior: entry.expectedBehavior,
            sourceFile: entry.exampleFile,
          });
        } catch (e) {
          log(`  Warning: Could not read ${entry.exampleFile}: ${e.message}`, colors.yellow);
        }
      }
    }
  }

  return testCases;
}

// ============================================================================
// Load Custom Test Files from test-cases directory
// ============================================================================

function loadCustomTestCases() {
  const testCases = [];

  if (!existsSync(CONFIG.testCasesDir)) {
    return testCases;
  }

  const files = readdirSync(CONFIG.testCasesDir).filter(f => f.endsWith('.ts'));

  for (const file of files) {
    const filePath = join(CONFIG.testCasesDir, file);
    const code = readFileSync(filePath, 'utf-8');

    // Extract individual namespace tests as separate test cases
    const namespaceRegex = /export namespace (\w+)\s*\{([\s\S]*?)(?=export namespace|\n}[\s]*$)/g;
    let match;
    let nsCount = 0;

    while ((match = namespaceRegex.exec(code)) !== null) {
      const [fullMatch, namespaceName, content] = match;
      nsCount++;

      // Determine phase from filename or namespace name
      let phase = 4;
      if (file.includes('phase1') || namespaceName.includes('Phase1')) phase = 1;
      else if (file.includes('phase2') || namespaceName.includes('Phase2')) phase = 2;
      else if (file.includes('phase3') || namespaceName.includes('Phase3')) phase = 3;

      // Create standalone code for this namespace test
      const standaloneCode = `// From ${file}: ${namespaceName}\n${fullMatch}}`;

      testCases.push({
        id: `custom-${file.replace('.ts', '')}-${namespaceName}`,
        title: `${namespaceName} (from ${file})`,
        code: standaloneCode,
        phase,
        category: 'custom',
        sourceFile: file,
      });
    }

    // If no namespaces found, treat whole file as one test
    if (nsCount === 0) {
      testCases.push({
        id: `custom-${file.replace('.ts', '')}`,
        title: `Custom test: ${file}`,
        code,
        phase: file.includes('phase1') ? 1 : file.includes('phase2') ? 2 : 4,
        category: 'custom',
        sourceFile: file,
      });
    }
  }

  return testCases;
}

// ============================================================================
// TSC Runner - Invokes the TypeScript compiler
// ============================================================================

async function runTsc(code, fileName = 'test.ts') {
  const ts = require('typescript');

  const compilerOptions = {
    strict: true,
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    noEmit: true,
    skipLibCheck: true,
  };

  // Create a virtual source file
  const sourceFile = ts.createSourceFile(
    fileName,
    code,
    ts.ScriptTarget.ES2020,
    true
  );

  // Create a simple compiler host
  const host = ts.createCompilerHost(compilerOptions);
  const originalGetSourceFile = host.getSourceFile;
  host.getSourceFile = (name, languageVersion, onError) => {
    if (name === fileName) {
      return sourceFile;
    }
    return originalGetSourceFile.call(host, name, languageVersion, onError);
  };

  // Create program and get diagnostics
  const program = ts.createProgram([fileName], compilerOptions, host);

  const allDiagnostics = [
    ...program.getSyntacticDiagnostics(sourceFile),
    ...program.getSemanticDiagnostics(sourceFile),
  ];

  return {
    diagnostics: allDiagnostics.map(d => ({
      code: d.code,
      message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
      start: d.start,
      length: d.length,
      category: ts.DiagnosticCategory[d.category],
    })),
    version: ts.version,
  };
}

// ============================================================================
// WASM Runner - Invokes the Rust/WASM compiler
// ============================================================================

async function runWasm(code, fileName = 'test.ts', testOptions = {}) {
  try {
    // Dynamic import of the WASM module
    const wasm = await import(join(CONFIG.wasmPkgPath, 'wasm.js'));

    // Create parser and check
    const parser = new wasm.ThinParser(fileName, code);
    if (!testOptions.nolib) {
      parser.addLibFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
    }
    parser.parseSourceFile();

    // Get parse diagnostics
    const parseDiagsJson = parser.getDiagnosticsJson();
    const parseDiags = JSON.parse(parseDiagsJson);

    // Get type check diagnostics
    const checkResultJson = parser.checkSourceFile();
    const checkResult = JSON.parse(checkResultJson);

    // Combine diagnostics
    const allDiagnostics = [
      ...parseDiags.map(d => ({
        code: d.code,
        message: d.message,
        start: d.start,
        length: d.length,
        category: 'Error',
        source: 'parser',
      })),
      ...(checkResult.diagnostics || []).map(d => ({
        code: d.code,
        message: d.message_text,
        start: d.start,
        length: d.length,
        category: d.category,
        source: 'checker',
      })),
    ];

    parser.free();

    return {
      diagnostics: allDiagnostics,
      version: 'rust-wasm',
      typeCount: checkResult.typeCount,
    };
  } catch (e) {
    return {
      diagnostics: [{
        code: -1,
        message: `WASM Error: ${e.message}`,
        start: 0,
        length: 0,
        category: 'Error',
        source: 'runtime',
      }],
      version: 'rust-wasm',
      error: e.message,
    };
  }
}

// ============================================================================
// Comparison Logic
// ============================================================================

function compareDiagnostics(tscResult, wasmResult) {
  const tscCodes = new Set(tscResult.diagnostics.map(d => d.code));
  const wasmCodes = new Set(wasmResult.diagnostics.map(d => d.code));

  const missingInWasm = [...tscCodes].filter(c => !wasmCodes.has(c));
  const extraInWasm = [...wasmCodes].filter(c => !tscCodes.has(c));

  // Check for same error at different locations
  const locationMismatches = [];
  for (const tscDiag of tscResult.diagnostics) {
    const wasmDiag = wasmResult.diagnostics.find(d => d.code === tscDiag.code);
    if (wasmDiag && (wasmDiag.start !== tscDiag.start || wasmDiag.length !== tscDiag.length)) {
      locationMismatches.push({
        code: tscDiag.code,
        tsc: { start: tscDiag.start, length: tscDiag.length },
        wasm: { start: wasmDiag.start, length: wasmDiag.length },
      });
    }
  }

  const match = missingInWasm.length === 0 &&
                extraInWasm.length === 0 &&
                locationMismatches.length === 0;

  return {
    match,
    tscErrorCount: tscResult.diagnostics.length,
    wasmErrorCount: wasmResult.diagnostics.length,
    missingInWasm,  // Errors TSC finds but WASM doesn't
    extraInWasm,    // Errors WASM finds but TSC doesn't
    locationMismatches,
    severity: calculateSeverity(missingInWasm, extraInWasm),
  };
}

function calculateSeverity(missingInWasm, extraInWasm) {
  // Missing errors are more severe (potential unsoundness)
  if (missingInWasm.length > 0) return 'high';
  // Extra errors are less severe (potential false positives)
  if (extraInWasm.length > 0) return 'medium';
  return 'low';
}

// ============================================================================
// Test Runner
// ============================================================================

async function runTest(testCase, verbose = false) {
  const startTime = Date.now();

  if (verbose) {
    log(`\n${'─'.repeat(60)}`, colors.dim);
    log(`Test: ${testCase.id}`, colors.cyan);
    log(`Title: ${testCase.title}`, colors.dim);
    log(`${'─'.repeat(60)}`, colors.dim);
  }

  // Run both compilers
  const [tscResult, wasmResult] = await Promise.all([
    runTsc(testCase.code),
    runWasm(testCase.code),
  ]);

  // Compare results
  const comparison = compareDiagnostics(tscResult, wasmResult);

  const duration = Date.now() - startTime;

  if (verbose) {
    log(`\nTSC (${tscResult.version}): ${tscResult.diagnostics.length} diagnostic(s)`, colors.blue);
    for (const d of tscResult.diagnostics) {
      log(`  TS${d.code}: ${d.message.split('\n')[0]}`, colors.dim);
    }

    log(`\nWASM: ${wasmResult.diagnostics.length} diagnostic(s)`, colors.blue);
    for (const d of wasmResult.diagnostics) {
      log(`  ${d.code}: ${d.message?.split('\n')[0] || 'N/A'}`, colors.dim);
    }

    if (comparison.match) {
      log(`\n✓ MATCH`, colors.green);
    } else {
      log(`\n✗ MISMATCH`, colors.red);
      if (comparison.missingInWasm.length > 0) {
        log(`  Missing in WASM: ${comparison.missingInWasm.map(c => `TS${c}`).join(', ')}`, colors.yellow);
      }
      if (comparison.extraInWasm.length > 0) {
        log(`  Extra in WASM: ${comparison.extraInWasm.join(', ')}`, colors.yellow);
      }
    }
  }

  return {
    testCase,
    tscResult,
    wasmResult,
    comparison,
    duration,
    passed: comparison.match,
  };
}

// ============================================================================
// Report Generator
// ============================================================================

function generateReport(results) {
  const passed = results.filter(r => r.passed);
  const failed = results.filter(r => !r.passed);

  // Group failures by severity
  const highSeverity = failed.filter(r => r.comparison.severity === 'high');
  const mediumSeverity = failed.filter(r => r.comparison.severity === 'medium');

  // Group by missing error code
  const missingByCode = {};
  for (const result of failed) {
    for (const code of result.comparison.missingInWasm) {
      if (!missingByCode[code]) missingByCode[code] = [];
      missingByCode[code].push(result.testCase.id);
    }
  }

  const report = {
    timestamp: new Date().toISOString(),
    summary: {
      total: results.length,
      passed: passed.length,
      failed: failed.length,
      passRate: ((passed.length / results.length) * 100).toFixed(1) + '%',
    },
    severity: {
      high: highSeverity.length,
      medium: mediumSeverity.length,
    },
    missingErrorCodes: missingByCode,
    prioritizedBugList: highSeverity.map(r => ({
      id: r.testCase.id,
      title: r.testCase.title,
      phase: r.testCase.phase,
      missingErrors: r.comparison.missingInWasm,
      code: r.testCase.code.slice(0, 200) + (r.testCase.code.length > 200 ? '...' : ''),
    })),
    results: results.map(r => ({
      id: r.testCase.id,
      passed: r.passed,
      severity: r.comparison.severity,
      tscErrors: r.tscResult.diagnostics.length,
      wasmErrors: r.wasmResult.diagnostics.length,
      missingInWasm: r.comparison.missingInWasm,
      extraInWasm: r.comparison.extraInWasm,
      duration: r.duration,
    })),
  };

  return report;
}

function printReport(report) {
  log('\n' + '═'.repeat(60), colors.bold);
  log('  DIFFERENTIAL TEST REPORT', colors.bold);
  log('═'.repeat(60), colors.bold);

  log(`\n  Timestamp: ${report.timestamp}`, colors.dim);
  log(`\n  Summary:`, colors.cyan);
  log(`    Total Tests:  ${report.summary.total}`);
  log(`    Passed:       ${report.summary.passed}`, colors.green);
  log(`    Failed:       ${report.summary.failed}`, report.summary.failed > 0 ? colors.red : '');
  log(`    Pass Rate:    ${report.summary.passRate}`);

  if (report.summary.failed > 0) {
    log(`\n  Severity Breakdown:`, colors.cyan);
    log(`    High (missing errors):   ${report.severity.high}`, colors.red);
    log(`    Medium (extra errors):   ${report.severity.medium}`, colors.yellow);

    if (Object.keys(report.missingErrorCodes).length > 0) {
      log(`\n  Missing Error Codes:`, colors.cyan);
      for (const [code, tests] of Object.entries(report.missingErrorCodes)) {
        log(`    TS${code}: ${tests.length} test(s)`, colors.yellow);
      }
    }

    if (report.prioritizedBugList.length > 0) {
      log(`\n  Prioritized Bug List:`, colors.cyan);
      for (const bug of report.prioritizedBugList.slice(0, 10)) {
        log(`\n    [Phase ${bug.phase}] ${bug.id}`, colors.red);
        log(`    ${bug.title}`, colors.dim);
        log(`    Missing: ${bug.missingErrors.map(c => `TS${c}`).join(', ')}`, colors.yellow);
      }
      if (report.prioritizedBugList.length > 10) {
        log(`\n    ... and ${report.prioritizedBugList.length - 10} more`, colors.dim);
      }
    }
  }

  log('\n' + '═'.repeat(60) + '\n', colors.bold);
}

// ============================================================================
// Main Entry Point
// ============================================================================

async function main() {
  const args = process.argv.slice(2);
  const verbose = args.includes('--verbose') || args.includes('-v');
  const catalogOnly = args.includes('--catalog');
  const reportOnly = args.includes('--report-only');

  // Ensure output directory exists
  if (!existsSync(CONFIG.outputDir)) {
    mkdirSync(CONFIG.outputDir, { recursive: true });
  }

  // Report only mode
  if (reportOnly) {
    if (existsSync(CONFIG.reportPath)) {
      const report = JSON.parse(readFileSync(CONFIG.reportPath, 'utf-8'));
      printReport(report);
    } else {
      log('No report found. Run tests first.', colors.yellow);
    }
    return;
  }

  log('Differential Testing Harness', colors.bold);
  log('═'.repeat(60), colors.dim);

  // Parse catalog and generate test cases
  log('\nParsing unsoundness catalog...', colors.cyan);
  const catalogEntries = parseUnsoundnessCatalog();
  log(`  Found ${catalogEntries.length} catalog entries`, colors.dim);

  log('\nGenerating test cases...', colors.cyan);
  const catalogTests = generateTestCases(catalogEntries);
  log(`  Generated ${catalogTests.length} catalog test cases`, colors.dim);

  log('\nLoading custom test cases...', colors.cyan);
  const customTests = loadCustomTestCases();
  log(`  Loaded ${customTests.length} custom test cases`, colors.dim);

  const testCases = [...catalogTests, ...customTests];
  log(`  Total: ${testCases.length} test cases`, colors.dim);

  // Filter by phase 1 first (most critical)
  const phase1Tests = testCases.filter(t => t.phase === 1);
  const testsToRun = catalogOnly ? phase1Tests : testCases;

  log(`\nRunning ${testsToRun.length} tests...`, colors.cyan);

  // Run tests
  const results = [];
  let passCount = 0;
  let failCount = 0;

  for (let i = 0; i < testsToRun.length; i++) {
    const testCase = testsToRun[i];

    if (!verbose) {
      process.stdout.write(`\r  Progress: ${i + 1}/${testsToRun.length}`);
    }

    try {
      const result = await runTest(testCase, verbose);
      results.push(result);

      if (result.passed) {
        passCount++;
      } else {
        failCount++;
      }
    } catch (e) {
      log(`\n  Error in test ${testCase.id}: ${e.message}`, colors.red);
      results.push({
        testCase,
        tscResult: { diagnostics: [] },
        wasmResult: { diagnostics: [], error: e.message },
        comparison: { match: false, severity: 'high', missingInWasm: [], extraInWasm: [] },
        duration: 0,
        passed: false,
        error: e.message,
      });
      failCount++;
    }
  }

  if (!verbose) {
    console.log(''); // New line after progress
  }

  // Generate and save report
  const report = generateReport(results);
  writeFileSync(CONFIG.reportPath, JSON.stringify(report, null, 2));
  log(`\nReport saved to: ${CONFIG.reportPath}`, colors.dim);

  // Print report
  printReport(report);

  // Exit with appropriate code
  process.exit(failCount > 0 ? 1 : 0);
}

main().catch(e => {
  console.error('Fatal error:', e);
  process.exit(1);
});
