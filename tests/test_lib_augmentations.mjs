#!/usr/bin/env node
/**
 * Test real-world lib.d.ts augmentations
 * Verifies that DOM APIs and Node.js globals are correctly resolved
 * by the type checker without TS2304 "Cannot find name" errors.
 */

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, join, resolve } from 'path';
import { readFileSync } from 'fs';

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const CONFIG = {
  wasmPkgPath: resolve(__dirname, '../pkg'),
  libPath: resolve(__dirname, './lib'),
};

// DOM API test code
const domTestCode = `
// Window and Document access
declare var myWindow: Window;
declare var myDoc: Document;

// Basic DOM operations
const element: HTMLElement = document.body;
const divEl: HTMLDivElement = document.createElement("div");
const buttonEl: HTMLButtonElement = document.createElement("button");
const inputEl: HTMLInputElement = document.createElement("input");

// Event handling
function handleClick(event: MouseEvent): void {
    const x = event.clientX;
    const y = event.clientY;
}

function handleKeyboard(event: KeyboardEvent): void {
    const key = event.key;
    const code = event.code;
}

// DOM manipulation
function manipulateDOM(doc: Document): void {
    const el = doc.getElementById("test");
    const elements = doc.getElementsByClassName("container");
    const form = doc.querySelector<HTMLFormElement>("form");
}

// Event listeners
function addEvents(target: EventTarget): void {
    target.addEventListener("click", (e) => { });
    target.removeEventListener("click", (e) => { });
}

// Fetch API
async function fetchData(url: string): Promise<Response> {
    const response = await fetch(url);
    return response;
}

// URL API
function workWithUrl(urlString: string): void {
    const url = new URL(urlString);
    const params = new URLSearchParams("key=value");
}

// Storage API
function workWithStorage(): void {
    const item = localStorage.getItem("key");
    sessionStorage.setItem("key", "value");
}

// Blob and File
function workWithBlobs(): void {
    const blob = new Blob(["content"], { type: "text/plain" });
    const file = new File(["content"], "test.txt");
}

// Headers and Request
function workWithFetch(): void {
    const headers = new Headers();
    headers.set("Content-Type", "application/json");
}

// AbortController
function workWithAbort(): void {
    const controller = new AbortController();
    const signal = controller.signal;
    controller.abort();
}

// Navigator
function workWithNavigator(nav: Navigator): void {
    const lang = nav.language;
    const online = nav.onLine;
}

// History and Location
function workWithHistory(history: History, location: Location): void {
    history.pushState({}, "", "/new-path");
    const path = location.pathname;
}
`;

// Node.js globals test code
const nodeTestCode = `
// Process object
function useProcess(): void {
    const cwd = process.cwd();
    const env = process.env.NODE_ENV;
    const argv = process.argv;
    const pid = process.pid;
    const platform = process.platform;
    const version = process.version;
}

// Buffer operations
function useBuffer(): void {
    const buf1 = Buffer.from("hello");
    const buf2 = Buffer.alloc(10);
    const buf3 = Buffer.concat([buf1, buf2]);
    const isBuffer = Buffer.isBuffer(buf1);
    const str = buf1.toString("utf8");
}

// Module system
function useModules(): void {
    const filename = __filename;
    const dir = __dirname;
}

// Timers
function useTimers(): void {
    const timeout = setTimeout(() => {}, 1000);
    const interval = setInterval(() => {}, 1000);
    const immediate = setImmediate(() => {});
    clearTimeout(timeout);
    clearInterval(interval);
    clearImmediate(immediate);
}

// Console extensions for Node.js
function useConsole(): void {
    console.log("message");
    console.error("error");
    console.warn("warning");
}

// TextEncoder/TextDecoder
function useTextCoding(): void {
    const encoder = new TextEncoder();
    const encoded = encoder.encode("hello");
    const decoder = new TextDecoder();
    const decoded = decoder.decode(encoded);
}

// AbortController (Node.js version)
function useAbortController(): void {
    const controller = new AbortController();
    const signal = controller.signal;
    controller.abort("reason");
}

// Performance API
function usePerformance(): void {
    const now = performance.now();
    const mark = performance.mark("test");
}

// Crypto API
function useCrypto(): void {
    const uuid = crypto.randomUUID();
}

// Fetch API (Node 18+)
async function useFetch(url: string): Promise<Response> {
    return fetch(url);
}

// queueMicrotask and structuredClone
function useGlobalFunctions(): void {
    queueMicrotask(() => {});
    const obj = { a: 1 };
    const clone = structuredClone(obj);
    const encoded = btoa("hello");
    const decoded = atob(encoded);
}
`;

// Standard library test code (core ES types)
const stdlibTestCode = `
// Array methods
function useArrays(): void {
    const arr = [1, 2, 3];
    const mapped = arr.map(x => x * 2);
    const filtered = arr.filter(x => x > 1);
    const reduced = arr.reduce((a, b) => a + b, 0);
    const includes = arr.includes(2);
    const found = arr.find(x => x === 2);
}

// Object methods
function useObjects(): void {
    const obj = { a: 1, b: 2 };
    const keys = Object.keys(obj);
    const values = Object.values(obj);
    const entries = Object.entries(obj);
    const frozen = Object.freeze(obj);
}

// String methods
function useStrings(): void {
    const str = "hello world";
    const upper = str.toUpperCase();
    const lower = str.toLowerCase();
    const split = str.split(" ");
    const trimmed = str.trim();
}

// Promise handling
async function usePromises(): Promise<void> {
    const p = Promise.resolve(42);
    const all = await Promise.all([p, p]);
    const race = await Promise.race([p, p]);
    const settled = await Promise.allSettled([p, p]);
}

// Map and Set
function useCollections(): void {
    const map = new Map<string, number>();
    map.set("key", 1);
    const hasKey = map.has("key");

    const set = new Set<number>();
    set.add(1);
    const hasValue = set.has(1);
}

// Error handling
function useErrors(): void {
    const err = new Error("message");
    const typeErr = new TypeError("type error");
    const rangeErr = new RangeError("range error");
}

// JSON operations
function useJson(): void {
    const obj = { a: 1 };
    const json = JSON.stringify(obj);
    const parsed = JSON.parse(json);
}

// Math operations
function useMath(): void {
    const pi = Math.PI;
    const abs = Math.abs(-5);
    const max = Math.max(1, 2, 3);
    const floor = Math.floor(3.14);
    const random = Math.random();
}

// Date operations
function useDates(): void {
    const now = new Date();
    const year = now.getFullYear();
    const month = now.getMonth();
    const time = now.getTime();
}

// RegExp
function useRegex(): void {
    const regex = new RegExp("pattern", "g");
    const test = regex.test("test pattern");
    const match = "test pattern".match(regex);
}

// Symbol
function useSymbols(): void {
    const sym = Symbol("description");
    const iter = Symbol.iterator;
}

// Typed Arrays
function useTypedArrays(): void {
    const buffer = new ArrayBuffer(16);
    const view = new DataView(buffer);
    const int8 = new Int8Array(buffer);
    const uint8 = new Uint8Array(buffer);
    const float32 = new Float32Array(buffer);
}

// Generators and Iterators (interface usage)
function* useGenerator(): Generator<number, void, unknown> {
    yield 1;
    yield 2;
}
`;

async function runTest(testName, code, libFiles) {
  const wasm = await import(join(CONFIG.wasmPkgPath, 'wasm.js'));

  const parser = new wasm.ThinParser(testName + '.ts', code);
  parser.parseSourceFile();

  // Add lib files
  for (const lib of libFiles) {
    parser.addLibFile(lib.name, lib.content);
  }

  const parseDiags = JSON.parse(parser.getDiagnosticsJson());
  const checkResult = JSON.parse(parser.checkSourceFile());

  const allDiags = [
    ...parseDiags.map(d => ({
      code: d.code,
      message: d.message,
    })),
    ...(checkResult.diagnostics || []).map(d => ({
      code: d.code,
      message: d.message_text,
    })),
  ];

  // Filter TS2304 errors (Cannot find name)
  const ts2304Errors = allDiags.filter(d => d.code === 2304);

  return {
    testName,
    totalErrors: allDiags.length,
    ts2304Errors,
    allDiags,
  };
}

async function main() {
  console.log('=== Testing Real-World lib.d.ts Augmentations ===\n');

  // Load lib files
  const libDts = readFileSync(join(CONFIG.libPath, 'lib.d.ts'), 'utf-8');
  const libDomDts = readFileSync(join(CONFIG.libPath, 'lib.dom.d.ts'), 'utf-8');
  const libNodeDts = readFileSync(join(CONFIG.libPath, 'lib.node.d.ts'), 'utf-8');

  const coreLibs = [
    { name: 'lib.d.ts', content: libDts },
  ];

  const domLibs = [
    { name: 'lib.d.ts', content: libDts },
    { name: 'lib.dom.d.ts', content: libDomDts },
  ];

  const nodeLibs = [
    { name: 'lib.d.ts', content: libDts },
    { name: 'lib.node.d.ts', content: libNodeDts },
  ];

  let allPassed = true;

  // Test 1: Standard library (core ES types)
  console.log('Test 1: Standard Library (Core ES Types)');
  const stdlibResult = await runTest('stdlib', stdlibTestCode, coreLibs);
  if (stdlibResult.ts2304Errors.length > 0) {
    console.log(`  FAIL: ${stdlibResult.ts2304Errors.length} TS2304 errors found`);
    for (const err of stdlibResult.ts2304Errors.slice(0, 10)) {
      console.log(`    - ${err.message}`);
    }
    allPassed = false;
  } else {
    console.log(`  PASS: No TS2304 errors (${stdlibResult.totalErrors} total errors)`);
  }

  // Test 2: DOM APIs
  console.log('\nTest 2: DOM APIs (Window, Document, etc.)');
  const domResult = await runTest('dom', domTestCode, domLibs);
  if (domResult.ts2304Errors.length > 0) {
    console.log(`  FAIL: ${domResult.ts2304Errors.length} TS2304 errors found`);
    for (const err of domResult.ts2304Errors.slice(0, 10)) {
      console.log(`    - ${err.message}`);
    }
    allPassed = false;
  } else {
    console.log(`  PASS: No TS2304 errors (${domResult.totalErrors} total errors)`);
  }

  // Test 3: Node.js globals
  console.log('\nTest 3: Node.js Globals (process, Buffer)');
  const nodeResult = await runTest('node', nodeTestCode, nodeLibs);
  if (nodeResult.ts2304Errors.length > 0) {
    console.log(`  FAIL: ${nodeResult.ts2304Errors.length} TS2304 errors found`);
    for (const err of nodeResult.ts2304Errors.slice(0, 10)) {
      console.log(`    - ${err.message}`);
    }
    allPassed = false;
  } else {
    console.log(`  PASS: No TS2304 errors (${nodeResult.totalErrors} total errors)`);
  }

  console.log('\n=== Summary ===');
  if (allPassed) {
    console.log('All tests PASSED: No TS2304 errors for standard library types');
    process.exit(0);
  } else {
    console.log('Some tests FAILED: TS2304 errors found for standard library types');
    process.exit(1);
  }
}

main().catch(err => {
  console.error('Test error:', err);
  process.exit(1);
});
