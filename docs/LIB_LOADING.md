# Lib Loading and Conformance Testing in tsz

This document explains how TypeScript declaration library files (lib files) are loaded in tsz and how conformance tests are run.

> **See Also**: [TSC_LIB_LOADING.md](architecture/TSC_LIB_LOADING.md) - Complete reference for how `tsc` handles library loading (target, lib, noLib, etc.)

## Table of Contents

1. [Lib Loading Overview](#lib-loading-overview)
2. [Target vs Lib: The Default vs Override Relationship](#target-vs-lib-the-default-vs-override-relationship)
3. [tsc vs tsz: Current Gap](#tsc-vs-tsz-current-gap)
4. [Lib Resolution Rules](#lib-resolution-rules)
5. [Virtual Host Configuration](#virtual-host-configuration)
6. [Conformance Test Infrastructure](#conformance-test-infrastructure)
7. [TSC Cache System](#tsc-cache-system)
8. [Running Tests](#running-tests)
9. [Debugging](#debugging)

---

## Lib Loading Overview

TypeScript uses lib files (`lib.es5.d.ts`, `lib.es2015.d.ts`, etc.) to provide type definitions for built-in JavaScript features. The lib loaded depends on the `--target` and `--lib` compiler options.

**Key Principle:**
- **`--target`** dictates **syntax** (how code is written: arrows vs functions, `const` vs `var`)
- **`--lib`** dictates **APIs** (what global objects/methods are available: `Promise`, `Map`, `fetch`)

By default, `--target` chooses the `--lib` for you. Once you touch `--lib`, you are the pilot.

---

## Target vs Lib: The Default vs Override Relationship

### Default Behavior (No Explicit `--lib`)

When `--lib` is **omitted**, TypeScript automatically includes type definitions based on `--target`:

| Target | Default Libraries | What You Get |
|--------|-------------------|--------------|
| ES5 | `lib.d.ts` | ES5 + DOM + ScriptHost |
| ES6/ES2015 | `lib.es6.d.ts` | ES2015 + DOM + DOM.Iterable + ScriptHost |
| ES2020 | `lib.es2020.full.d.ts` | ES2020 + DOM + DOM.Iterable + DOM.AsyncIterable + ScriptHost |
| ESNext | `lib.esnext.full.d.ts` | ESNext + DOM + DOM.Iterable + DOM.AsyncIterable + ScriptHost |

> **The Logic:** If you tell TypeScript you're outputting modern code (like `ES2022`), it assumes you're running that code in an environment that natively supports `ES2022` features.

### Override Behavior (Explicit `--lib`)

The moment you add a `lib` array to your config, **the automatic relationship breaks.** TypeScript stops looking at your `--target` for library guidance and trusts you completely.

```typescript
// tsconfig.json
{
  "compilerOptions": {
    "target": "ESNext",
    "lib": ["ES5"]  // ⚠️ Even with ESNext target, you only get ES5 types!
  }
}
```

If you set your target to `ESNext` but only put `["ES5"]` in your `lib` array, TypeScript will throw errors if you try to use `Promise` or `Map`, even though your output code would technically support them.

### Practical Comparison

| Scenario | `--target` | `--lib` | Result |
|----------|-----------|---------|--------|
| **Default** | ES6 | *Not set* | `Map`, `Set`, `Promise` available automatically |
| **Override** | ES5 | `["ES2015", "DOM"]` | Can use `Promise` (must polyfill at runtime) |
| **Strict Node** | ESNext | `["ESNext"]` | Modern JS features, but `window` throws error (no DOM) |

### Why Manually Set `--lib`?

- **Decoupling Syntax from APIs:** Use modern syntax (e.g., `async/await`) but polyfill the API
- **Non-Browser Environments:** Remove DOM library to prevent accidentally using `window` or `document`
- **Minimal Environments:** Embedded systems or custom runtimes

---

## tsc vs tsz: Current Gap

### How tsc Works

When no `--lib` is specified, tsc loads the `.full` lib file for the target, which includes:
- Core ES types (Array, Object, Promise, etc.)
- DOM types (document, window, console, fetch, etc.)
- ScriptHost types (legacy Windows scripting)

| Target | tsc Default | Includes |
|--------|-------------|----------|
| ES5 | `lib.d.ts` | ES5 + DOM + ScriptHost |
| ES2015 | `lib.es6.d.ts` | ES2015 + DOM + ScriptHost |
| ES2020 | `lib.es2020.full.d.ts` | ES2020 + DOM + ScriptHost |
| ESNext | `lib.esnext.full.d.ts` | ESNext + DOM + ScriptHost |

### How tsz Currently Works

tsz loads only **core libs** (without DOM):

| Target | tsz Default | Includes |
|--------|-------------|----------|
| ES5 | `lib.es5.d.ts` | ES5 only |
| ES2015 | `lib.es2015.d.ts` | ES2015 only |
| ES2020 | `lib.es2020.d.ts` | ES2020 only |
| ESNext | `lib.esnext.d.ts` | ESNext only |

### Why This Matters

This causes ~1646 conformance test failures related to:
- TS2318: Cannot find global type 'X'
- TS2583: Cannot find name 'X'
- TS2584: Cannot find name 'X'. Do you need to change your target library?

Many tests expect `console`, DOM types, or utility types that are available in `.full` libs.

### The Fix

See [TSC_LIB_LOADING.md](architecture/TSC_LIB_LOADING.md) for the complete analysis. The solution involves:
1. Matching tsc's default lib selection (use `.full` files)
2. Ensuring the conformance cache is regenerated with the same lib selection

## Canonical Lib Path

**The ONLY location for lib files is: `TypeScript/src/lib`**

This path is relative to the project root and is used consistently across:
- `tsz` CLI
- `tsz-server`
- Conformance test runner
- TSC cache generation

When packaging for npm/cargo, lib files are copied to `$PACKAGE_ROOT/TypeScript/src/lib`.

## Lib Resolution Rules

### 1. Explicit `@lib` Directive (Replaces Defaults)

If a test or project specifies `@lib` (or `--lib`), it **completely replaces** the default library selection:

```typescript
// @target: es5
// @lib: es2015,dom
// Result: ES2015 + DOM types, NOT the default ES5 + DOM + ScriptHost
```

**Important:** When `@lib` is specified, you must include everything you need. TypeScript stops looking at `@target` for library guidance.

### 2. Triple-Slash Reference Directives (Adds to Existing)

Unlike the `@lib` option which **replaces** defaults, triple-slash directives **add** to the existing environment:

```typescript
/// <reference lib="es2015.promise" />
/// <reference lib="dom" />

// This file now has: (default libs based on target) + es2015.promise + dom
```

**Key Differences:**

| Feature | `@lib` / `--lib` | `/// <reference lib="..." />` |
|---------|------------------|-------------------------------|
| **Scope** | Project-wide or file-level | File-specific only |
| **Behavior** | Replaces all defaults | Adds to existing defaults |
| **Best Use Case** | Defining standard runtime | Testing edge cases or platform-specific APIs |

**Rules for Triple-Slash Directives:**
- Must be at the **very top** of the file
- If any code (even `import`) appears above them, TypeScript ignores the directive
- Provides full IntelliSense (unlike `@ts-ignore`)

### 3. Default Libs from Target

When no explicit `@lib` is specified, the default lib is derived from `@target`:

| Target | Default Lib | Includes |
|--------|-------------|----------|
| ES3, ES5 | `es5.full` | ES5 + DOM + ScriptHost |
| ES6, ES2015 | `es2015.full` | ES2015 + DOM + ScriptHost |
| ES2016-ESNext | `es20XX.full` | ES20XX + DOM + ScriptHost |

### 4. Dependency Resolution (Reference Chain)

Lib files reference other libs via `/// <reference lib="..." />` directives. These are resolved **recursively**:

```
es2017.d.ts
├── es2016.d.ts
│   └── es2015.d.ts
│       └── es5.d.ts
│           ├── decorators.d.ts
│           └── decorators.legacy.d.ts
├── es2017.arraybuffer.d.ts
├── es2017.date.d.ts
├── es2017.intl.d.ts
├── es2017.object.d.ts
├── es2017.sharedmemory.d.ts
├── es2017.string.d.ts
└── es2017.typedarrays.d.ts
```

This is why `getDefaultLibFileName()` is critical - it determines the **root** of the dependency graph.

### 5. `@noLib` Directive

When `@noLib: true` is specified, no lib files are loaded. This results in TS2318 errors for missing global types (Array, Object, etc.).

## Implementation Details

### tsz-server (`src/bin/tsz_server.rs`)

Uses `default_lib_name_for_target()` from `cli::config` to determine libs:

```rust
let lib_name = default_lib_name_for_target(target);
load_lib_recursive(lib_name, &lib_dir);
```

### Conformance Runner (`conformance/src/worker.ts`)

```typescript
function getLibNamesForTestCase(opts, compilerOptionsTarget) {
  if (opts.nolib) return [];
  const explicit = parseLibOption(opts.lib);
  if (explicit.length > 0) return explicit;
  
  // Derive from target
  const targetName = normalizeTargetName(compilerOptionsTarget ?? opts.target);
  return [defaultCoreLibNameForTarget(targetName)];
}
```

### TSC Cache Generation (`conformance/src/cache-worker.ts`)

The cache worker uses identical logic to ensure TSC baseline matches tsz behavior:

```typescript
function getLibNamesForTestCase(opts, compilerOptionsTarget) {
  if (opts.nolib) return [];
  const explicit = parseLibOption(opts.lib);
  if (explicit.length > 0) return explicit;
  
  const targetName = normalizeTargetName(compilerOptionsTarget ?? opts.target);
  return [defaultCoreLibNameForTarget(targetName)];
}
```

**Critical**: Both workers must use the same lib resolution logic, otherwise the TSC cache will have different errors than tsz produces.

## Virtual Host Configuration

When running TSC programmatically (for cache generation or testing), we configure a virtual file system. This is **critical** for correct lib loading.

### The Key Insight: Reference Chain Resolution

TypeScript uses `getDefaultLibFileName()` as the **ROOT** of its library dependency graph. It only loads libs reachable via `/// <reference lib="..." />` from this file.

```
getDefaultLibFileName() returns "lib.es2015.d.ts"
    ↓
lib.es2015.d.ts contains:
    /// <reference lib="es5" />
    /// <reference lib="es2015.core" />
    /// <reference lib="es2015.promise" />
    ...
    ↓
TSC recursively loads all referenced libs
```

If `getDefaultLibFileName()` returns the wrong lib, TSC never discovers the libs you need.

### Correct Virtual Host Setup

```typescript
const host = ts.createCompilerHost(compilerOptions);

// Provide lib files via getSourceFile (with basename fallback)
host.getSourceFile = (name) => sourceFiles.get(name) ?? sourceFiles.get(path.basename(name));

// CRITICAL: Return the correct lib based on EXPLICIT @lib if specified
host.getDefaultLibFileName = () => {
  // When explicit @lib is specified, use that as the root
  if (libNames.length > 0) {
    const firstLib = libNames[0];  // e.g., "es6"
    const normalized = normalizeLibName(firstLib);  // "es6" → "es2015"
    const firstLibFile = `lib.${normalized}.d.ts`;
    if (sourceFiles.has(firstLibFile)) {
      return firstLibFile;  // Start from the explicit lib
    }
  }
  // Fallback to target-based selection
  return targetLibMap[target] ?? 'lib.es5.d.ts';
};
```

### Critical Rule: Do NOT Set `compilerOptions.lib`

```typescript
// ❌ WRONG - This bypasses your virtual filesystem!
compilerOptions.lib = ['lib.es6.d.ts'];

// ✅ CORRECT - Load libs into sourceFiles, use getDefaultLibFileName
const libFiles = collectLibFiles(libNames, libDir);
for (const [name, content] of libFiles.entries()) {
  sourceFiles.set(name, ts.createSourceFile(name, content, target, true));
}
```

**Why?** When `compilerOptions.lib` is set, TypeScript looks for those lib files at **absolute paths** in the TypeScript installation directory (e.g., `/node_modules/typescript/lib/lib.es6.d.ts`), completely bypassing your virtual file system.

### The Bug We Fixed

Previously, the TSC cache generator had this bug:
1. Test file had `@lib: es6` with `@target: es5`
2. Code set `compilerOptions.lib = ['lib.es6.d.ts']` 
3. TSC looked for `/node_modules/typescript/lib/lib.es6.d.ts`
4. Virtual filesystem was bypassed → TS2318 "Cannot find global type 'Promise'" errors

The fix:
1. **Don't** set `compilerOptions.lib`
2. **Do** load libs into sourceFiles via `collectLibFiles()`
3. **Do** return the correct lib from `getDefaultLibFileName()` based on explicit `@lib`

## TS2318: Cannot find global type

This error occurs when:
1. `@noLib: true` is set (intentional - no libs loaded)
2. Lib files fail to load (bug)
3. A global type is used that doesn't exist in the loaded libs

Common global types that trigger TS2318:
- Core (ES5): `Array`, `Boolean`, `Function`, `IArguments`, `Number`, `Object`, `RegExp`, `String`
- ES2015+: `Promise`, `Symbol`, `Iterable`, `IterableIterator`
- ES2018+: `AsyncIterable`, `AsyncIterableIterator`

## Debugging Lib Loading

### Check what libs are loaded

```bash
# In conformance runner, add logging:
console.log('libNames:', libNames);
console.log('libFiles loaded:', libFiles.size);
```

### Verify TSC cache has correct errors

```bash
cd conformance/.tsc-cache
node -e "
const d = require('./tsc-results.json');
const test = d.entries['path/to/test.ts'];
console.log('Errors:', test?.codes);
"
```

### Test TSC directly

```bash
npx tsc test.ts --noEmit --target es2017
# Should produce same errors as cache
```

---

## Conformance Test Infrastructure

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    conformance/run.sh                            │
│                    (Entry point script)                          │
└─────────────────────┬───────────────────────────────────────────┘
                      │
          ┌───────────┴───────────┐
          ▼                       ▼
┌─────────────────────┐   ┌─────────────────────┐
│   cache generate    │   │   --server / --all  │
│                     │   │                     │
│ cache-worker.ts     │   │ runner-server.ts    │
│ (TSC baseline)      │   │ (tsz-server tests)  │
└─────────────────────┘   └─────────────────────┘
          │                       │
          ▼                       ▼
┌─────────────────────┐   ┌─────────────────────┐
│ .tsc-cache/         │   │ tsz-server          │
│ tsc-results.json    │   │ (Rust binary)       │
└─────────────────────┘   └─────────────────────┘
```

### Components

| Component | File | Purpose |
|-----------|------|---------|
| Entry script | `conformance/run.sh` | CLI interface, builds tsz-server, dispatches to runners |
| Server runner | `conformance/src/runner-server.ts` | Runs tests against tsz-server, compares to TSC cache |
| Worker | `conformance/src/worker.ts` | Parses tests, manages worker pool for server runner |
| Cache worker | `conformance/src/cache-worker.ts` | Generates TSC baseline cache |
| TSC cache | `conformance/src/tsc-cache.ts` | Cache management (load/save/invalidate) |
| Test utils | `conformance/src/test-utils.ts` | Shared test parsing, directive handling |
| Lib manifest | `conformance/src/lib-manifest.ts` | Lib file resolution utilities |

### Test File Format

Tests are TypeScript files with directive comments:

```typescript
// @target: es2017
// @lib: es2015,dom
// @strict: true
// @noEmitHelpers: true

// Test code here
class Example {
  async method() {
    return await Promise.resolve(42);
  }
}
```

**Common Directives:**
- `@target`: ECMAScript target (es5, es2015, es2017, esnext, etc.)
- `@lib`: Explicit lib files to load (comma-separated)
- `@noLib`: Disable all lib loading (true/false)
- `@strict`: Enable strict mode
- `@module`: Module system (commonjs, esnext, etc.)
- `@filename`: For multi-file tests, specifies file boundaries

### Multi-File Tests

```typescript
// @filename: types.ts
export interface User {
  name: string;
}

// @filename: main.ts
import { User } from './types';
const user: User = { name: 'Alice' };
```

---

## TSC Cache System

### Purpose

The TSC cache stores TypeScript compiler diagnostic results for all conformance tests. This allows:
1. Fast comparison without running TSC for every test
2. Consistent baseline for tsz to match against
3. Cache invalidation when TypeScript version changes

### Cache File

Location: `conformance/.tsc-cache/tsc-results.json`

```json
{
  "generatedAt": "2024-01-29T12:00:00.000Z",
  "typescriptVersion": "5.7.0",
  "typescriptSha": "b19a9da2a3b8",
  "entries": {
    "conformance/types/any/anyAssignability.ts": {
      "codes": [2322, 2322, 2345],
      "hash": "a1b2c3d4..."
    }
  }
}
```

### Cache Generation

```bash
./conformance/run.sh cache generate
```

This runs the TSC compiler on all test files using `cache-worker.ts` workers:

1. Parse test file directives (`@target`, `@lib`, etc.)
2. Determine lib files to load (explicit or from target)
3. Create virtual file system with test files + libs
4. Run `ts.createProgram()` and collect diagnostics
5. Store error codes in cache

### Cache Invalidation

Cache is regenerated when:
- TypeScript submodule SHA changes
- Cache file doesn't exist
- Manual regeneration requested

---

## Running Tests

### Quick Start

```bash
# Run all conformance tests (builds tsz-server first)
./conformance/run.sh --all

# Run subset of tests
./conformance/run.sh --max=1000

# Run specific test pattern
./conformance/run.sh --filter="async"

# Regenerate TSC cache
./conformance/run.sh cache generate
```

### Test Flow

1. **Build**: `run.sh` builds `tsz-server` in release mode
2. **Start Server**: Spawns `tsz-server` process
3. **Load Cache**: Reads TSC baseline from `.tsc-cache/tsc-results.json`
4. **Run Tests**: For each test file:
   - Parse directives
   - Send to tsz-server via JSON-RPC
   - Collect diagnostic codes
   - Compare to TSC cache
5. **Report**: Show pass rate, missing/extra errors

### Server Protocol

tsz-server uses JSON-RPC over stdin/stdout:

```json
// Request
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "check",
  "params": {
    "files": [{"name": "test.ts", "content": "..."}],
    "options": {"target": "es2017", "strict": true}
  }
}

// Response
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "diagnostics": [
      {"code": 2322, "message": "Type 'string' is not assignable..."}
    ]
  }
}
```

### Test Results

Output format example:

```
Pass Rate: XX.X% (N/M)

Top Missing Errors:
  TSXXXX: Nx    (tsz should emit but doesn't)

Top Extra Errors:
  TSXXXX: Nx    (tsz emits but tsc doesn't)
```

- **Missing**: TSC emits this error, tsz doesn't
- **Extra**: tsz emits this error, TSC doesn't
- **Pass**: Test passes when tsz produces exactly the same error codes as TSC

---

## Debugging

### Check what libs are loaded

Add logging to worker:
```typescript
console.log('libNames:', libNames);
console.log('libFiles loaded:', libFiles.size);
```

### Verify TSC cache for a test

```bash
cd conformance/.tsc-cache
node -e "
const d = require('./tsc-results.json');
const test = d.entries['conformance/types/any/anyAssignability.ts'];
console.log('Errors:', test?.codes);
"
```

### Test TSC directly

```bash
# Run tsc on a test file to see expected errors
npx tsc TypeScript/tests/cases/conformance/types/any/anyAssignability.ts \
  --noEmit --target es5 --strict
```

### Debug tsz-server

```bash
# Run server in debug mode
RUST_LOG=debug cargo run --bin tsz-server

# Or check server logs during conformance run
./conformance/run.sh --all 2>&1 | tee conformance.log
```

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Many TS2318 errors | Lib files not loading | Check `TSZ_LIB_DIR` env var |
| Cache outdated | TypeScript updated | Run `cache generate` |
| Test timeout | Infinite loop in tsz | Check recursive type handling |
| OOM | Large type expansion | Check memory limits |

---

## Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `TSZ_LIB_DIR` | Override lib file directory | `TypeScript/src/lib` |
| `TSZ_SERVER_TIMEOUT` | Per-test timeout (ms) | 5000 |
| `TSZ_MAX_WORKERS` | Parallel worker count | CPU cores |

---

## History

Prior to the fix, the `cache-worker.ts` had a bug where `getLibNamesForTestCase` only returned explicitly specified `@lib` values and never derived default libs from `@target`. This caused the TSC cache to contain many false TS2318 errors (6960 tests affected).

The fix aligned both `worker.ts` and `cache-worker.ts` to use identical lib resolution logic, reducing TS2318 discrepancies by 89%.
