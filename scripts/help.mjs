#!/usr/bin/env node
/**
 * Test Command Helper - Show available testing commands for Project Zang
 * 
 * Usage: node scripts/help.mjs
 */

const commands = {
  "Rust Unit Tests": {
    "Run all tests": "./test.sh",
    "Run specific test": "./test.sh test_name", 
    "Run benchmarks": "./test.sh --bench",
    "Rebuild Docker image": "./test.sh --rebuild",
    "Clean cache": "./test.sh --clean"
  },
  
  "TypeScript Conformance": {
    "Quick conformance check": "./differential-test/run-conformance.sh --max=1000",
    "Full conformance suite": "./differential-test/run-conformance.sh --all",
    "Test compiler category": "./differential-test/run-conformance.sh --category=compiler",
    "Test conformance category": "./differential-test/run-conformance.sh --category=conformance"
  },
  
  "Error Analysis": {
    "Find TS2454 (used before assigned)": "node differential-test/find-ts2454.mjs",
    "Find TS2322 (not assignable)": "node differential-test/find-ts2322.mjs", 
    "Find TS2339 (property doesn't exist)": "node differential-test/find-ts2339.mjs",
    "Find TS2564 (property not initialized)": "node differential-test/find-ts2564.mjs"
  },
  
  "Individual Testing": {
    "Test single file": "node scripts/run-single-test.mjs tests/cases/compiler/2dArrays.ts",
    "Test with verbose output": "node scripts/run-single-test.mjs path/to/test.ts --verbose --thin",
    "Compare baselines": "node scripts/compare-baselines.mjs 100 compiler",
    "Validate WASM module": "node scripts/validate-wasm.mjs",
    "Run batch tests": "node scripts/run-batch-tests.mjs"
  }
};

console.log("🧪 Project Zang - Testing Commands\n");

Object.entries(commands).forEach(([category, cmds]) => {
  console.log(`📁 ${category}`);
  console.log("─".repeat(50));
  
  Object.entries(cmds).forEach(([desc, cmd]) => {
    console.log(`  ${desc.padEnd(35)} ${cmd}`);
  });
  
  console.log("");
});

console.log("📖 For detailed testing guide, see: TESTING.md");
console.log("📊 For conformance metrics, run: ./differential-test/run-conformance.sh --max=1000");
console.log("");