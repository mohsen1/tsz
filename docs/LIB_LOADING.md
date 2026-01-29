# Lib Loading and Conformance Testing in tsz

This document explains how TypeScript declaration library files (lib files) are loaded in tsz and how conformance tests are run.

## Table of Contents

1. [Lib Loading Overview](#lib-loading-overview)
2. [Lib Resolution Rules](#lib-resolution-rules)
3. [Conformance Test Infrastructure](#conformance-test-infrastructure)
4. [TSC Cache System](#tsc-cache-system)
5. [Running Tests](#running-tests)
6. [Debugging](#debugging)

---

## Lib Loading Overview

TypeScript uses lib files (`lib.es5.d.ts`, `lib.es2015.d.ts`, etc.) to provide type definitions for built-in JavaScript features. The lib loaded depends on the `--target` and `--lib` compiler options.

## Canonical Lib Path

**The ONLY location for lib files is: `TypeScript/src/lib`**

This path is relative to the project root and is used consistently across:
- `tsz` CLI
- `tsz-server`
- Conformance test runner
- TSC cache generation

When packaging for npm/cargo, lib files are copied to `$PACKAGE_ROOT/TypeScript/src/lib`.

## Lib Resolution Rules

### 1. Explicit `@lib` Directive

If a test or project specifies `@lib` (or `--lib`), those exact libs are loaded:

```typescript
// @lib: es2015,dom
// Loads: es2015.d.ts, dom.d.ts (and their dependencies)
```

### 2. Default Libs from Target

When no explicit `@lib` is specified, the default lib is derived from `@target`:

| Target | Default Lib |
|--------|-------------|
| ES3, ES5 | `es5` |
| ES6, ES2015 | `es2015` |
| ES2016 | `es2016` |
| ES2017 | `es2017` |
| ES2018 | `es2018` |
| ES2019 | `es2019` |
| ES2020 | `es2020` |
| ES2021 | `es2021` |
| ES2022 | `es2022` |
| ES2023 | `es2023` |
| ES2024 | `es2024` |
| ESNext | `esnext` |

### 3. Dependency Resolution

Lib files reference other libs via `/// <reference lib="..." />` directives. These are resolved recursively:

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

### 4. `@noLib` Directive

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

When running TSC programmatically (for cache generation or testing), we configure a virtual file system:

```typescript
const host = ts.createCompilerHost(compilerOptions);

// Provide lib files via getSourceFile
host.getSourceFile = (name) => sourceFiles.get(name) ?? sourceFiles.get(path.basename(name));

// Return base lib file (tsc follows /// <reference lib="..."/> directives)
host.getDefaultLibFileName = () => {
  if (sourceFiles.has('lib.es5.d.ts')) return 'lib.es5.d.ts';
  return 'lib.d.ts';
};

// DON'T set compilerOptions.lib - it causes tsc to look for libs at absolute paths
```

**Important**: Do NOT set `compilerOptions.lib` when using a virtual host. This causes TSC to look for lib files at absolute paths in the TypeScript installation, bypassing your virtual file system.

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

```
Pass Rate: 37.8% (4,562/12,054)

Top Missing Errors:
  TS2488: 1579x    (tsz should emit but doesn't)
  TS2322: 860x

Top Extra Errors:
  TS2322: 11814x   (tsz emits but tsc doesn't)
  TS2307: 3933x
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
