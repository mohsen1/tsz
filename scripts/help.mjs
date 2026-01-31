#!/usr/bin/env node
/**
 * Test Command Helper - Show available testing commands for Project Zang
 * 
 * Usage: node scripts/help.mjs
 */

const commands = {
  "Conformance Tests": {
    "Run 500 tests": "./conformance/run-conformance.sh",
    "Run 100 tests": "./conformance/run-conformance.sh --max=100",
    "Run all tests": "./conformance/run-conformance.sh --all",
    "Compiler tests only": "./conformance/run-conformance.sh --category=compiler",
    "Verbose output": "./conformance/run-conformance.sh --verbose",
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
    "Build runner": "cd conformance && npm run build",
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
