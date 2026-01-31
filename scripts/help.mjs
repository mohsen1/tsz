#!/usr/bin/env node
/**
 * Test Command Helper - Show available testing commands for Project Zang
 * 
 * Usage: node scripts/help.mjs
 */

const commands = {
  "Conformance Tests": {
    "Run 500 tests": "./scripts/conformance/run.sh",
    "Run 100 tests": "./scripts/conformance/run.sh --max=100",
    "Run all tests": "./scripts/conformance/run.sh --all",
    "Compiler tests only": "./scripts/conformance/run.sh --category=compiler",
    "Verbose output": "./scripts/conformance/run.sh --verbose",
  },
  
  "Rust Unit Tests": {
    "Run all tests": "./scripts/test.sh",
    "Run specific test": "./scripts/test.sh test_name",
    "Run benchmarks": "./scripts/test.sh --bench",
  },
  
  "Single File Debugging": {
    "Test single file": "node scripts/run-single-test.mjs path/to/test.ts",
    "Validate WASM": "node scripts/validate-wasm.mjs",
  },

  "Build": {
    "Build WASM": "./scripts/build-wasm.sh",
    "Build runner": "cd scripts/conformance && npm run build",
  }
};

console.log(`
╔══════════════════════════════════════════════════════════╗
║              🧪 Project Zang - Test Commands             ║
╚══════════════════════════════════════════════════════════╝
`);

Object.entries(commands).forEach(([category, cmds]) => {
  console.log(`📁 ${category}`);
  console.log("─".repeat(58));
  
  Object.entries(cmds).forEach(([desc, cmd]) => {
    console.log(`  ${desc.padEnd(25)} ${cmd}`);
  });
  
  console.log("");
});

console.log(`╔══════════════════════════════════════════════════════════╗
║  Scripts apply resource limits (memory, timeout) to      ║
║  protect the host from runaway tests.                    ║
╚══════════════════════════════════════════════════════════╝
`);
