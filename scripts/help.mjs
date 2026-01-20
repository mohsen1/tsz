#!/usr/bin/env node
/**
 * Test Command Helper - Show available testing commands for Project Zang
 * 
 * Usage: node scripts/help.mjs
 */

const commands = {
  "Rust Unit Tests": {
    "Run all tests": "./scripts/test.sh",
    "Run specific test": "./scripts/test.sh test_name",
    "Run benchmarks": "./scripts/test.sh --bench",
    "Rebuild Docker image": "./scripts/test.sh --rebuild"
  },
  
  "TypeScript Conformance (Docker)": {
    "Quick conformance (100)": "./conformance/run-conformance.sh --max=100",
    "Standard conformance (500)": "./conformance/run-conformance.sh --max=500",
    "Verbose output": "./conformance/run-conformance.sh --max=100 --verbose",
    "Full conformance suite": "./conformance/run-conformance.sh --all"
  },
  
  "Single File Testing": {
    "Test single file": "node scripts/run-single-test.mjs path/to/test.ts",
    "Test with verbose output": "node scripts/run-single-test.mjs path/to/test.ts --verbose",
    "Validate WASM module": "node scripts/validate-wasm.mjs"
  },

  "Build": {
    "Build WASM": "./scripts/build-wasm.sh",
    "Build conformance runner": "cd conformance && npm run build"
  }
};

console.log("🧪 Project Zang - Testing Commands\n");

Object.entries(commands).forEach(([category, cmds]) => {
  console.log(`📁 ${category}`);
  console.log("─".repeat(50));
  
  Object.entries(cmds).forEach(([desc, cmd]) => {
    console.log(`  ${desc.padEnd(30)} ${cmd}`);
  });
  
  console.log("");
});

console.log("⚠️  Always run conformance tests in Docker to prevent OOM/hangs");
console.log("📖 See docs/TESTING_CLEANUP_PLAN.md for details");
console.log("");
