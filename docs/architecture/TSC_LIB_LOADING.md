# TypeScript Library Loading: Complete Reference

This document explains exactly how the TypeScript compiler (`tsc`) handles library files, targets, and global type definitions. This is the canonical reference for `tsz` to match tsc's behavior.

## Table of Contents

1. [Overview](#overview)
2. [Key Concepts](#key-concepts)
3. [Default Library Selection](#default-library-selection)
4. [Explicit `--lib` Option](#explicit---lib-option)
5. [The `--noLib` Flag](#the---nolib-flag)
6. [Library File Structure](#library-file-structure)
7. [Reference Resolution](#reference-resolution)
8. [tslib vs lib.d.ts](#tslib-vs-libdts)
9. [Related Flags](#related-flags)
10. [Conformance Testing Implications](#conformance-testing-implications)

---

## Overview

TypeScript uses **library files** (`lib.*.d.ts`) to provide type definitions for:
- Built-in JavaScript APIs (`Array`, `Object`, `Promise`, `Math`, etc.)
- DOM APIs (`document`, `window`, `HTMLElement`, etc.)
- Host environments (`ScriptHost` for Windows Script Host)

These are **compile-time only** - they tell TypeScript what types exist in the runtime environment, but don't affect the generated JavaScript.

### The Core Principle: Default vs Override

The relationship between `--target` and `--lib` is a **"default vs. override"** dynamic:

- **`--target`** dictates **syntax** (how code is written: arrows vs functions, `const` vs `var`)
- **`--lib`** dictates **APIs** (what global objects/methods are available: `Promise`, `Map`, `fetch`)

> When no `--lib` is specified, tsc uses the `target` to determine which libraries to load.
> When `--lib` IS specified, it **completely replaces** the defaults - you must include everything you need.

**In other words:** By default, `--target` chooses the `--lib` for you. Once you touch `--lib`, you are the pilot.

---

## Key Concepts

### Library Categories

| Category | Files | Purpose |
|----------|-------|---------|
| **Core ES** | `es5.d.ts`, `es2015.d.ts`, etc. | ECMAScript built-ins (Array, Object, Promise) |
| **DOM** | `dom.d.ts`, `dom.iterable.d.ts` | Browser APIs (document, window, fetch) |
| **WebWorker** | `webworker.d.ts` | Web Worker APIs |
| **ScriptHost** | `scripthost.d.ts` | Windows Script Host (legacy) |
| **Feature-specific** | `es2015.promise.d.ts`, `es2020.bigint.d.ts` | Individual ES features |

### File Naming Conventions

```
lib.d.ts              → Default ES5 + DOM + ScriptHost (alias for es5.full.d.ts)
lib.es5.d.ts          → Core ES5 only (no DOM)
lib.es5.full.d.ts     → ES5 + DOM + ScriptHost (same as lib.d.ts)
lib.es2015.d.ts       → Core ES2015 only (references es5 + es2015 features)
lib.es2015.full.d.ts  → ES2015 + DOM + ScriptHost
lib.es6.d.ts          → Special alias for es2015.full (historical reasons)
lib.esnext.d.ts       → Latest ES features (core only)
lib.esnext.full.d.ts  → Latest ES + DOM + ScriptHost
```

---

## Default Library Selection

### Target to Default Library Mapping

When NO explicit `--lib` is provided, tsc maps `target` to a default library:

| Target | Default Library File | Contents |
|--------|---------------------|----------|
| ES3, ES5 | `lib.d.ts` | ES5 + DOM + WebWorker.ImportScripts + ScriptHost |
| ES2015/ES6 | `lib.es6.d.ts`¹ | ES2015 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost |
| ES2016 | `lib.es2016.full.d.ts` | ES2016 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost |
| ES2017 | `lib.es2017.full.d.ts` | ES2017 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost |
| ES2018 | `lib.es2018.full.d.ts` | ES2018 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost |
| ES2019 | `lib.es2019.full.d.ts` | ES2019 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost |
| ES2020+ | `lib.es20XX.full.d.ts` | ES20XX + DOM + DOM.Iterable + DOM.AsyncIterable + WebWorker.ImportScripts + ScriptHost |
| ESNext | `lib.esnext.full.d.ts` | ESNext + DOM + DOM.Iterable + DOM.AsyncIterable + WebWorker.ImportScripts + ScriptHost |

¹ **Note**: ES2015/ES6 uses `lib.es6.d.ts` (NOT `lib.es2015.full.d.ts`) for historical compatibility reasons.

### TypeScript Source Code Reference

From `TypeScript/src/compiler/utilitiesPublic.ts`:

```typescript
export const targetToLibMap: Map<ScriptTarget, string> = new Map([
    [ScriptTarget.ESNext, "lib.esnext.full.d.ts"],
    [ScriptTarget.ES2024, "lib.es2024.full.d.ts"],
    [ScriptTarget.ES2023, "lib.es2023.full.d.ts"],
    [ScriptTarget.ES2022, "lib.es2022.full.d.ts"],
    [ScriptTarget.ES2021, "lib.es2021.full.d.ts"],
    [ScriptTarget.ES2020, "lib.es2020.full.d.ts"],
    [ScriptTarget.ES2019, "lib.es2019.full.d.ts"],
    [ScriptTarget.ES2018, "lib.es2018.full.d.ts"],
    [ScriptTarget.ES2017, "lib.es2017.full.d.ts"],
    [ScriptTarget.ES2016, "lib.es2016.full.d.ts"],
    [ScriptTarget.ES2015, "lib.es6.d.ts"], // NOT lib.es2015.full.d.ts!
]);

export function getDefaultLibFileName(options: CompilerOptions): string {
    const target = getEmitScriptTarget(options);
    switch (target) {
        case ScriptTarget.ESNext:
        case ScriptTarget.ES2024:
        // ... ES2015 through ES2024 ...
            return targetToLibMap.get(target)!;
        default:
            return "lib.d.ts"; // ES3/ES5 default
    }
}
```

### What `.full` Files Contain

Each `.full.d.ts` file references other libs via `/// <reference lib="..." />` directives:

**lib.es5.full.d.ts** (same as lib.d.ts):
```typescript
/// <reference lib="es5" />
/// <reference lib="dom" />
/// <reference lib="webworker.importscripts" />
/// <reference lib="scripthost" />
```

**lib.es2015.full.d.ts**:
```typescript
/// <reference lib="es2015" />
/// <reference lib="dom" />
/// <reference lib="dom.iterable" />
/// <reference lib="webworker.importscripts" />
/// <reference lib="scripthost" />
```

**lib.es2020.full.d.ts**:
```typescript
/// <reference lib="es2020" />
/// <reference lib="dom" />
/// <reference lib="webworker.importscripts" />
/// <reference lib="scripthost" />
/// <reference lib="dom.iterable" />
/// <reference lib="dom.asynciterable" />
```

---

## Explicit `--lib` Option

### Behavior

When `--lib` is specified explicitly, it **completely replaces** the default library selection.

```bash
# Only ES2020 core - NO DOM types
tsc --lib es2020 file.ts

# ES2020 + DOM (must specify both!)
tsc --lib es2020,dom file.ts

# Just ES5 (no Promise, no DOM)
tsc --lib es5 file.ts
```

### Available Library Names

From `TypeScript/src/compiler/commandLineParser.ts`:

**High-level libraries:**
- `es5`, `es6`, `es7`, `es2015`, `es2016`, `es2017`, `es2018`, `es2019`, `es2020`, `es2021`, `es2022`, `es2023`, `es2024`, `esnext`
- `dom`, `dom.iterable`, `dom.asynciterable`
- `webworker`, `webworker.importscripts`, `webworker.iterable`, `webworker.asynciterable`
- `scripthost`

**Feature-specific libraries:**
- `es2015.core`, `es2015.collection`, `es2015.generator`, `es2015.iterable`, `es2015.promise`, `es2015.proxy`, `es2015.reflect`, `es2015.symbol`, `es2015.symbol.wellknown`
- `es2016.array.include`, `es2016.intl`
- `es2017.object`, `es2017.string`, `es2017.intl`, `es2017.sharedmemory`, `es2017.typedarrays`
- `es2018.asyncgenerator`, `es2018.asynciterable`, `es2018.promise`, `es2018.regexp`, `es2018.intl`
- `es2019.array`, `es2019.object`, `es2019.string`, `es2019.symbol`, `es2019.intl`
- `es2020.bigint`, `es2020.promise`, `es2020.sharedmemory`, `es2020.string`, `es2020.symbol.wellknown`, `es2020.intl`, `es2020.number`
- `es2021.promise`, `es2021.string`, `es2021.weakref`, `es2021.intl`
- `es2022.array`, `es2022.error`, `es2022.object`, `es2022.string`, `es2022.regexp`, `es2022.intl`
- `es2023.array`, `es2023.collection`, `es2023.intl`
- `es2024.arraybuffer`, `es2024.collection`, `es2024.object`, `es2024.promise`, `es2024.regexp`, `es2024.sharedmemory`, `es2024.string`
- `decorators`, `decorators.legacy`

### Library Name to File Mapping

```typescript
// From TypeScript/src/compiler/commandLineParser.ts
const libEntries: [string, string][] = [
    ["es5", "lib.es5.d.ts"],
    ["es6", "lib.es2015.d.ts"],
    ["es2015", "lib.es2015.d.ts"],
    ["dom", "lib.dom.d.ts"],
    ["dom.iterable", "lib.dom.iterable.d.ts"],
    // ... etc
];
```

### Common Gotcha

```typescript
// tsconfig.json
{
  "compilerOptions": {
    "target": "ES2020",
    "lib": ["ES2020"]  // ⚠️ NO DOM! console.log() will error
  }
}
```

This results in errors like:
```
error TS2584: Cannot find name 'console'. Do you need to change your target library?
```

### Why Manually Set `--lib`?

Common scenarios include:

- **Decoupling Syntax from APIs:** You want to use modern syntax (e.g., `async/await` from `ES2017`) but your environment requires a polyfill for the API
- **Non-Browser Environments:** If you're writing for Node.js, remove the `DOM` library to prevent accidentally using `window` or `document`
- **Minimal/Embedded Environments:** Custom runtimes that only support specific APIs

---

## Triple-Slash Reference Directives

### Overview

While `tsconfig.json` / `--lib` sets global rules for your project, triple-slash directives allow a **single file** to claim extra capabilities:

```typescript
/// <reference lib="es2015.promise" />
/// <reference lib="dom" />

// This file now has access to Promise and DOM types
```

### Key Difference: Addition vs Replacement

| Feature | `--lib` in tsconfig | `/// <reference lib="..." />` |
|---------|---------------------|-------------------------------|
| **Scope** | Project-wide | File-specific |
| **Behavior** | Replaces all defaults | Adds to existing defaults |
| **Best Use Case** | Defining the standard runtime | Testing edge cases or platform-specific APIs |

### Rules

1. **Addition, not Replacement:** Unlike `--lib` which replaces defaults, a triple-slash reference **adds** to the existing environment
2. **Order Matters:** These must be at the very top of the file. If there is any code (even an `import`) above them, TypeScript will ignore the directive
3. **Full IntelliSense:** Unlike `@ts-ignore`, using `/// <reference lib="..." />` provides full type checking and IntelliSense

### Use Cases in Tests

Triple-slash directives are common in testing for:

- **Polyfilled Environments:** Testing code for older environments (like IE11) where you've polyfilled `Promise` or `Map`
- **Feature Detection Tests:** Testing library code that checks `if (window.fetch)` - the test needs DOM types
- **Isolation:** Keep your main `tsconfig.json` clean - only add WebWorker API to files that need it

---

## The `--noLib` Flag

### Behavior

When `--noLib: true` is set:
1. **ALL** library loading is disabled (even explicit `--lib` is ignored)
2. TypeScript cannot compile without certain primitive types

### Required Global Types

TypeScript requires these interfaces to be defined somewhere:
- `Array`
- `Boolean`
- `Function`
- `IArguments`
- `Number`
- `Object`
- `RegExp`
- `String`

Without these, you get TS2318: "Cannot find global type 'X'".

### Use Cases

`--noLib` is used when:
1. Creating your own runtime definitions
2. Targeting non-standard JavaScript environments
3. Writing minimal type definitions for embedded systems

---

## Library File Structure

### Reference Chain Example

When tsc loads `lib.es2017.full.d.ts`, it follows references recursively:

```
lib.es2017.full.d.ts
├── lib.es2017.d.ts
│   ├── lib.es2016.d.ts
│   │   ├── lib.es2015.d.ts
│   │   │   ├── lib.es5.d.ts
│   │   │   │   ├── lib.decorators.d.ts
│   │   │   │   └── lib.decorators.legacy.d.ts
│   │   │   ├── lib.es2015.core.d.ts
│   │   │   ├── lib.es2015.collection.d.ts
│   │   │   ├── lib.es2015.generator.d.ts
│   │   │   ├── lib.es2015.iterable.d.ts
│   │   │   ├── lib.es2015.promise.d.ts
│   │   │   ├── lib.es2015.proxy.d.ts
│   │   │   ├── lib.es2015.reflect.d.ts
│   │   │   ├── lib.es2015.symbol.d.ts
│   │   │   └── lib.es2015.symbol.wellknown.d.ts
│   │   └── lib.es2016.array.include.d.ts
│   ├── lib.es2017.object.d.ts
│   ├── lib.es2017.string.d.ts
│   ├── lib.es2017.intl.d.ts
│   ├── lib.es2017.sharedmemory.d.ts
│   └── lib.es2017.typedarrays.d.ts
├── lib.dom.d.ts
├── lib.dom.iterable.d.ts
├── lib.webworker.importscripts.d.ts
└── lib.scripthost.d.ts
```

### Global Type Definitions

`lib.es5.d.ts` defines the foundational global types:

```typescript
// Global variables
declare var NaN: number;
declare var Infinity: number;
declare function eval(x: string): any;
declare function parseInt(string: string, radix?: number): number;
// ...

// Core interfaces
interface Object { ... }
interface ObjectConstructor { ... }
declare var Object: ObjectConstructor;

interface Function { ... }
interface FunctionConstructor { ... }
declare var Function: FunctionConstructor;

interface Array<T> { ... }
interface ArrayConstructor { ... }
declare var Array: ArrayConstructor;

// Utility types (defined here!)
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Record<K extends keyof any, T> = { [P in K]: T };
type Exclude<T, U> = T extends U ? never : T;
type Extract<T, U> = T extends U ? T : never;
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type NonNullable<T> = T & {};
// ... etc
```

---

## tslib vs lib.d.ts

These are **completely different** things:

| Aspect | lib.d.ts | tslib |
|--------|----------|-------|
| **Type** | Type definitions | Runtime JavaScript |
| **When used** | Compile time | Runtime |
| **Purpose** | Tell TS what types exist | Helper functions (__extends, __assign) |
| **Installation** | Bundled with tsc | npm install tslib |
| **Flag** | `--lib`, `--noLib` | `--importHelpers` |

### tslib Details

`tslib` is an npm package that provides runtime helper functions used by TypeScript's downlevel compilation:

```typescript
// When compiling ES6 spread to ES5:
// Input: [...arr]
// Output (inline helpers): var __spreadArray = function() {...}; __spreadArray(arr);
// Output (with tslib): var tslib_1 = require("tslib"); tslib_1.__spreadArray(arr);
```

Use `--importHelpers` to use tslib instead of inlining helpers, reducing bundle size.

---

## Related Flags

### `--skipLibCheck`

Skips type checking of **all** declaration files (`.d.ts`), including lib files and third-party types.

### `--skipDefaultLibCheck`

Skips type checking of **only** the default lib files (`lib.*.d.ts`), but still checks third-party `.d.ts` files.

### `--noDefaultLib` (in source files)

A triple-slash directive that tells tsc not to include default libs for this file:

```typescript
/// <reference no-default-lib="true"/>
```

This is NOT the same as `--noLib`. It's used in lib files themselves to prevent circular references.

---

## Virtual Host Configuration (Programmatic TSC)

When running TSC programmatically (for tooling, cache generation, or testing), special care is required for lib loading.

### The Critical Insight: Reference Chain Resolution

TypeScript uses `getDefaultLibFileName()` as the **ROOT** of its library dependency graph. It only loads libs reachable via `/// <reference lib="..." />` from this file:

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

If `getDefaultLibFileName()` returns the wrong lib (e.g., `lib.es5.d.ts` when `@lib: es6` is specified), TSC never discovers the ES2015 libs.

### Critical Rule: Do NOT Set `compilerOptions.lib` with Virtual Hosts

```typescript
// ❌ WRONG - This bypasses your virtual filesystem!
compilerOptions.lib = ['lib.es6.d.ts'];

// ✅ CORRECT - Load libs into sourceFiles, use getDefaultLibFileName
const libFiles = collectLibFiles(libNames, libDir);
for (const [name, content] of libFiles.entries()) {
  sourceFiles.set(name, ts.createSourceFile(name, content, target, true));
}
```

**Why?** When `compilerOptions.lib` is set, TypeScript resolves those lib files at **absolute paths** in the TypeScript installation directory (e.g., `/node_modules/typescript/lib/lib.es6.d.ts`), completely bypassing your virtual file system.

### Correct Virtual Host Setup

```typescript
const host = ts.createCompilerHost(compilerOptions);

// Provide lib files via getSourceFile (with basename fallback for full paths)
host.getSourceFile = (name) => sourceFiles.get(name) ?? sourceFiles.get(path.basename(name));

// CRITICAL: Return the correct lib based on EXPLICIT @lib if specified
host.getDefaultLibFileName = () => {
  // When explicit @lib is specified, use that lib as the dependency graph root
  if (libNames.length > 0) {
    const firstLib = libNames[0];  // e.g., "es6"
    const normalized = normalizeLibName(firstLib);  // "es6" → "es2015"
    const firstLibFile = `lib.${normalized}.d.ts`;
    if (sourceFiles.has(firstLibFile)) {
      return firstLibFile;
    }
  }
  // Fallback to target-based selection
  return targetLibMap[target] ?? 'lib.es5.d.ts';
};

host.fileExists = (name) => sourceFiles.has(name) || sourceFiles.has(path.basename(name));
host.readFile = (name) => {
  const sf = sourceFiles.get(name) ?? sourceFiles.get(path.basename(name));
  return sf?.getFullText();
};
```

### Common Bug Pattern

A bug we encountered in the conformance cache generator:

1. Test file had `@lib: es6` with `@target: es5`
2. Code set `compilerOptions.lib = ['lib.es6.d.ts']`
3. TSC looked for `/node_modules/typescript/lib/lib.es6.d.ts` (absolute path)
4. Virtual filesystem was bypassed → TS2318 "Cannot find global type 'Promise'"

**The fix:**
1. Do NOT set `compilerOptions.lib`
2. Load libs into sourceFiles via `collectLibFiles()`
3. Return the correct lib from `getDefaultLibFileName()` based on explicit `@lib`

---

## Conformance Testing Implications

### The Problem

Conformance tests often specify `@target` but not `@lib`:
```typescript
// @target: es2017
const p = Promise.resolve(1);  // Needs Promise type
```

### Expected Behavior

When `@target: es2017` is specified without `@lib`:
- tsc loads `lib.es2017.full.d.ts`
- This includes `lib.es2017.d.ts` → `lib.es2015.d.ts` → `lib.es2015.promise.d.ts`
- `Promise` is available

### Current tsz Behavior

tsz loads only the core lib (`es2017.d.ts`) without DOM/ScriptHost. This is correct for most conformance tests because:
1. Tests don't typically use DOM APIs
2. Core libs include all ES features needed
3. Tests that need DOM specify `@lib: dom` explicitly

### Edge Cases

Some tests may fail if they expect:
1. `console` to be available (requires `dom`)
2. `setTimeout` / `setInterval` (requires `dom`)
3. `fetch` (requires `dom`)

These tests should have `@lib: dom` specified.

---

## tsz Implementation Requirements

To match tsc exactly, tsz must:

1. **Default lib selection**: Map target → `.full` lib file (includes DOM, ScriptHost)
2. **Explicit `--lib`**: Completely replace defaults, resolve each lib name to file
3. **`--noLib`**: Disable all lib loading, ignore `--lib`
4. **Reference resolution**: Follow `/// <reference lib="..." />` directives recursively
5. **Ordering**: Load libs in the correct order (affects overload resolution)

### Current Gap

tsz currently loads **core libs only** (e.g., `es5.d.ts` instead of `lib.d.ts`). This means:
- ❌ No DOM types by default
- ❌ No ScriptHost types by default
- ❌ `console.log()` fails without explicit `@lib: dom`

This was intentional for conformance testing but doesn't match tsc's actual behavior for real-world usage.

From `src/cli/config.rs` (lines ~730-747):
```rust
/// Returns the core lib name (without DOM) - matches tsc conformance test behavior.
/// Use core libs by default since:
/// 1. Our conformance cache was generated with core libs (es5.d.ts, not es5.full.d.ts)
/// 2. Core libs are smaller (~220KB vs ~2MB for .full)
/// 3. Tests that need DOM should specify @lib: dom explicitly
pub fn default_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        ScriptTarget::ES3 | ScriptTarget::ES5 => "es5",  // Should be "lib" to match tsc
        ScriptTarget::ES2015 => "es2015",                 // Should be "es6" to match tsc
        ScriptTarget::ES2016 => "es2016",                 // Should be "es2016.full"
        // ...
    }
}
```

### Recommended Fix

To match tsc behavior, change `default_lib_name_for_target()` in `src/cli/config.rs`:

| Target | Current (tsz) | Correct (tsc) |
|--------|--------------|---------------|
| ES3/ES5 | `"es5"` | `"lib"` |
| ES2015 | `"es2015"` | `"es6"` |
| ES2016+ | `"es20XX"` | `"es20XX.full"` |
| ESNext | `"esnext"` | `"esnext.full"` |

**Important**: The recursive reference resolution is already implemented in `resolve_lib_files()`. Changing the default lib names should automatically load all referenced libs (DOM, ScriptHost, etc.) via `/// <reference lib="..." />` directives.

**Fallback Safety**: `resolve_default_lib_files()` has fallback logic that will use core libs if `.full` files don't exist:
```rust
if default_lib == "lib" {
    fallbacks.push("es5");
}
```

### Conformance Testing Consideration

If the conformance cache was generated with core libs, changing to `.full` libs will require regenerating the cache with the same settings. Options:
1. Regenerate cache with `.full` libs (recommended for parity)
2. Keep a `--conformance-mode` flag that uses core libs for testing
3. Accept that some tests may fail until cache is regenerated

---

## Quick Reference

### Common Scenarios

| Scenario | Config | Libraries Loaded |
|----------|--------|-----------------|
| Default ES5 | `{}` | lib.d.ts (ES5 + DOM + ScriptHost) |
| Default ES2020 | `{"target": "ES2020"}` | lib.es2020.full.d.ts (ES2020 + DOM + ScriptHost) |
| Node.js | `{"lib": ["ES2020"]}` | ES2020 only (no DOM) |
| Browser | `{"lib": ["ES2020", "DOM"]}` | ES2020 + DOM |
| Minimal | `{"noLib": true}` | Nothing (must provide own types) |

### Error Codes

| Code | Message | Cause |
|------|---------|-------|
| TS2318 | Cannot find global type 'X' | Missing lib file or `noLib: true` |
| TS2583 | Cannot find name 'X'. Do you need to install type definitions? | Missing lib or @types package |
| TS2584 | Cannot find name 'X'. Do you need to change your target library? | Need higher ES version or specific lib |

---

## References

- [TypeScript tsconfig lib option](https://www.typescriptlang.org/tsconfig/lib.html)
- [TypeScript tsconfig target option](https://www.typescriptlang.org/tsconfig/target.html)
- [TypeScript tsconfig noLib option](https://www.typescriptlang.org/tsconfig/noLib.html)
- [TypeScript source: utilitiesPublic.ts](https://github.com/microsoft/TypeScript/blob/main/src/compiler/utilitiesPublic.ts)
- [TypeScript source: commandLineParser.ts](https://github.com/microsoft/TypeScript/blob/main/src/compiler/commandLineParser.ts)
- [TypeScript lib files](https://github.com/microsoft/TypeScript/tree/main/src/lib)
