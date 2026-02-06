# TypeScript Library Loading: Complete Reference

This document explains exactly how the TypeScript compiler (`tsc`) handles library files, targets, and global type definitions. This is the canonical reference for `tsz` to match tsc's behavior.

**Verified against:** TypeScript 6.0.0-dev.20260116 (submodule commit `7f6a84673`)

## Table of Contents

1. [Overview](#overview)
2. [Key Concepts](#key-concepts)
3. [Default Library Selection](#default-library-selection)
4. [Explicit `--lib` Option](#explicit---lib-option)
5. [Triple-Slash Reference Directives](#triple-slash-reference-directives)
6. [The `--noLib` Flag](#the---nolib-flag)
7. [Library File Structure](#library-file-structure)
8. [tslib vs lib.d.ts](#tslib-vs-libdts)
9. [Related Flags](#related-flags)
10. [Virtual Host Configuration](#virtual-host-configuration-programmatic-tsc)
11. [Conformance Testing Implications](#conformance-testing-implications)
12. [tsz Implementation Requirements](#tsz-implementation-requirements)
13. [Quick Reference](#quick-reference)

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
- **`--target`** also dictates **APIs** by selecting a default set of lib files (which include DOM)
- **`--lib`** when specified, **completely replaces** the target's default libs

> When no `--lib` is specified, tsc uses the `target` to determine which libraries to load.
> When `--lib` IS specified, it **completely replaces** the defaults - you must include everything you need.

**In other words:** By default, `--target` chooses the `--lib` for you. Once you touch `--lib`, you are the pilot.

---

## Key Concepts

### Library Categories

| Category | Files | Purpose |
|----------|-------|---------|
| **Core ES** | `es5.d.ts`, `es2015.d.ts`, etc. | ECMAScript built-ins (Array, Object, Promise) |
| **DOM** | `dom.d.ts`, `dom.iterable.d.ts`, `dom.asynciterable.d.ts` | Browser APIs (document, window, fetch) |
| **WebWorker** | `webworker.d.ts` | Web Worker APIs |
| **ScriptHost** | `scripthost.d.ts` | Windows Script Host (legacy) |
| **Feature-specific** | `es2015.promise.d.ts`, `es2020.bigint.d.ts` | Individual ES features |
| **Decorators** | `decorators.d.ts`, `decorators.legacy.d.ts` | Decorator support (referenced by es5.d.ts) |

### File Naming Conventions

```
lib.d.ts              → ES5 "full" (ES5 + DOM + WebWorker.ImportScripts + ScriptHost)
lib.es5.d.ts          → Core ES5 only (no DOM), references decorators + decorators.legacy
lib.es6.d.ts          → ES2015 "full" (ES2015 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost)
lib.es2015.d.ts       → Core ES2015 only (references es5 + es2015 features)
lib.es2016.full.d.ts  → ES2016 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost
lib.es2017.full.d.ts  → ES2017 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost
lib.es2018.full.d.ts  → ES2018 + DOM + DOM.Iterable + DOM.AsyncIterable + WebWorker.ImportScripts + ScriptHost
lib.esnext.full.d.ts  → Latest ES + DOM + DOM.Iterable + DOM.AsyncIterable + WebWorker.ImportScripts + ScriptHost
```

> **Important:** There is NO `lib.es5.full.d.ts` or `lib.es2015.full.d.ts`. Instead, `lib.d.ts` serves as the ES5 "full" and `lib.es6.d.ts` serves as the ES2015 "full". The `.full.d.ts` naming convention starts at ES2016.

---

## Default Library Selection

### Target to Default Library Mapping

When NO explicit `--lib` is provided, tsc maps `target` to a default library:

| Target | Default Library File | Direct Contents |
|--------|---------------------|----------|
| ES5 (default) | `lib.d.ts` | ES5 + DOM + WebWorker.ImportScripts + ScriptHost |
| ES2015/ES6 | `lib.es6.d.ts`¹ | ES2015 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost |
| ES2016 | `lib.es2016.full.d.ts` | ES2016 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost |
| ES2017 | `lib.es2017.full.d.ts` | ES2017 + DOM + DOM.Iterable + WebWorker.ImportScripts + ScriptHost |
| ES2018 | `lib.es2018.full.d.ts` | ES2018 + DOM + DOM.Iterable + DOM.AsyncIterable + WebWorker.ImportScripts + ScriptHost |
| ES2019 | `lib.es2019.full.d.ts` | ES2019 + DOM + DOM.Iterable + DOM.AsyncIterable + WebWorker.ImportScripts + ScriptHost |
| ES2020+ | `lib.es20XX.full.d.ts` | ES20XX + DOM + DOM.Iterable + DOM.AsyncIterable + WebWorker.ImportScripts + ScriptHost |
| ESNext | `lib.esnext.full.d.ts` | ESNext + DOM + DOM.Iterable + DOM.AsyncIterable + WebWorker.ImportScripts + ScriptHost |

¹ **Note**: ES2015/ES6 uses `lib.es6.d.ts` (NOT `lib.es2015.full.d.ts` — that file doesn't exist) for historical compatibility reasons.

> **TS 6.0 Breaking Change**: ES3 target has been removed. `--target es3` gives error TS5108.

### Critical: DOM Now References ES2015 (TS 6.0)

In TypeScript 6.0, `lib.dom.d.ts` contains:
```typescript
/// <reference lib="es2015" />
/// <reference lib="es2018.asynciterable" />
```

This means **even `--target es5` effectively loads ES2015 features** because:
```
lib.d.ts → dom → es2015 → (es5 + es2015.core + es2015.promise + es2015.collection + ...)
lib.d.ts → dom → es2018.asynciterable
```

Verified with `tsc --target es5 --listFiles`:
```
lib.d.ts
lib.es5.d.ts
lib.es2015.d.ts          ← loaded transitively via dom!
lib.dom.d.ts
lib.webworker.importscripts.d.ts
lib.scripthost.d.ts
lib.es2015.core.d.ts     ← Promise, Map, Set, Array.find, etc.
lib.es2015.collection.d.ts
lib.es2015.generator.d.ts
lib.es2015.iterable.d.ts
lib.es2015.promise.d.ts  ← Promise constructor
lib.es2015.proxy.d.ts
lib.es2015.reflect.d.ts
lib.es2015.symbol.d.ts
lib.es2015.symbol.wellknown.d.ts
lib.es2018.asynciterable.d.ts  ← also from dom!
lib.decorators.d.ts
lib.decorators.legacy.d.ts
```

This means `Promise.resolve()`, `Array.find()`, `Map`, `Set`, `Symbol` all work with `--target es5` because DOM pulls them in.

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
    [ScriptTarget.ES2015, "lib.es6.d.ts"], // We don't use lib.es2015.full.d.ts due to breaking change.
]);

export function getDefaultLibFileName(options: CompilerOptions): string {
    const target = getEmitScriptTarget(options);
    switch (target) {
        case ScriptTarget.ESNext:
        case ScriptTarget.ES2024:
        // ... ES2015 through ES2024 ...
            return targetToLibMap.get(target)!;
        default:
            return "lib.d.ts"; // ES5 default (ES3 removed in TS 6.0)
    }
}
```

### What the "Full" Files Contain

Each "full" file (or equivalent) references other libs via `/// <reference lib="..." />` directives:

**lib.d.ts** (default for ES5 — there is no `lib.es5.full.d.ts`):
```typescript
/// <reference lib="es5" />
/// <reference lib="dom" />
/// <reference lib="webworker.importscripts" />
/// <reference lib="scripthost" />
```

**lib.es6.d.ts** (default for ES2015 — there is no `lib.es2015.full.d.ts`):
```typescript
/// <reference lib="es2015" />
/// <reference lib="dom" />
/// <reference lib="dom.iterable" />
/// <reference lib="webworker.importscripts" />
/// <reference lib="scripthost" />
```

**lib.es2016.full.d.ts** through **lib.es2017.full.d.ts**:
```typescript
/// <reference lib="es20XX" />
/// <reference lib="dom" />
/// <reference lib="webworker.importscripts" />
/// <reference lib="scripthost" />
/// <reference lib="dom.iterable" />
```

**lib.es2018.full.d.ts** through **lib.esnext.full.d.ts** (adds `dom.asynciterable`):
```typescript
/// <reference lib="es20XX" />
/// <reference lib="dom" />
/// <reference lib="webworker.importscripts" />
/// <reference lib="scripthost" />
/// <reference lib="dom.iterable" />
/// <reference lib="dom.asynciterable" />
```

> **Note:** `dom.asynciterable` starts from ES2018, not ES2020.

---

## Explicit `--lib` Option

### Behavior

When `--lib` is specified explicitly, it **completely replaces** the default library selection.

```bash
# Only ES2020 core - NO DOM types (but dom.d.ts brings es2015 if included)
tsc --lib es2020 file.ts

# ES2020 + DOM (must specify both!)
tsc --lib es2020,dom file.ts

# Just ES5 (no Promise constructor, no DOM)
tsc --lib es5 file.ts
```

### What `--lib es5` vs `--target es5` loads (important distinction)

| Config | Files Loaded |
|--------|-------------|
| `--lib es5` | `lib.es5.d.ts`, `lib.decorators.d.ts`, `lib.decorators.legacy.d.ts` |
| `--target es5` (no --lib) | All of the above PLUS `lib.dom.d.ts` → `lib.es2015.d.ts` → all ES2015 sub-libs, `lib.webworker.importscripts.d.ts`, `lib.scripthost.d.ts` |
| `--lib es5,dom` | `lib.es5.d.ts` + `lib.dom.d.ts` → `lib.es2015.d.ts` → all ES2015 sub-libs (because dom refs es2015) |

This means `--lib es5` gives you much less than `--target es5` because `--target es5` includes DOM which transitively includes ES2015.

### Available Library Names

From `TypeScript/src/compiler/commandLineParser.ts`:

**High-level libraries:**
- `es5`, `es6`/`es2015`, `es7`/`es2016`, `es2017`, `es2018`, `es2019`, `es2020`, `es2021`, `es2022`, `es2023`, `es2024`, `esnext`
- `dom`, `dom.iterable`, `dom.asynciterable`
- `webworker`, `webworker.importscripts`, `webworker.iterable`, `webworker.asynciterable`
- `scripthost`
- `decorators`, `decorators.legacy`

**Feature-specific libraries:**
- `es2015.core`, `es2015.collection`, `es2015.generator`, `es2015.iterable`, `es2015.promise`, `es2015.proxy`, `es2015.reflect`, `es2015.symbol`, `es2015.symbol.wellknown`
- `es2016.array.include`, `es2016.intl`
- `es2017.arraybuffer`, `es2017.date`, `es2017.object`, `es2017.string`, `es2017.intl`, `es2017.sharedmemory`, `es2017.typedarrays`
- `es2018.asyncgenerator`, `es2018.asynciterable`, `es2018.intl`, `es2018.promise`, `es2018.regexp`
- `es2019.array`, `es2019.object`, `es2019.string`, `es2019.symbol`, `es2019.intl`
- `es2020.bigint`, `es2020.date`, `es2020.promise`, `es2020.sharedmemory`, `es2020.string`, `es2020.symbol.wellknown`, `es2020.intl`, `es2020.number`
- `es2021.promise`, `es2021.string`, `es2021.weakref`, `es2021.intl`
- `es2022.array`, `es2022.error`, `es2022.intl`, `es2022.object`, `es2022.string`, `es2022.regexp`
- `es2023.array`, `es2023.collection`, `es2023.intl`
- `es2024.arraybuffer`, `es2024.collection`, `es2024.object`, `es2024.promise`, `es2024.regexp`, `es2024.sharedmemory`, `es2024.string`
- `esnext.intl`, `esnext.decorators`, `esnext.disposable`, `esnext.collection`, `esnext.array`, `esnext.iterator`, `esnext.promise`, `esnext.float16`, `esnext.typedarrays`, `esnext.error`, `esnext.sharedmemory`

Some esnext entries are **aliases** to versioned libs (e.g., `esnext.symbol` → `lib.es2019.symbol.d.ts`, `esnext.bigint` → `lib.es2020.bigint.d.ts`).

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
Try changing the 'lib' compiler option to include 'dom'.
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
1. **ALL** library loading is disabled
2. **Cannot be combined with `--lib`** — tsc gives error TS5053: "Option 'lib' cannot be specified with option 'noLib'."
3. TypeScript cannot compile without certain primitive types

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
│   │   ├── lib.es2016.array.include.d.ts
│   │   └── lib.es2016.intl.d.ts
│   ├── lib.es2017.arraybuffer.d.ts
│   ├── lib.es2017.date.d.ts
│   ├── lib.es2017.object.d.ts
│   ├── lib.es2017.string.d.ts
│   ├── lib.es2017.intl.d.ts
│   ├── lib.es2017.sharedmemory.d.ts
│   └── lib.es2017.typedarrays.d.ts
├── lib.dom.d.ts
│   ├── lib.es2015.d.ts (deduped — already loaded above)
│   └── lib.es2018.asynciterable.d.ts
├── lib.dom.iterable.d.ts
├── lib.webworker.importscripts.d.ts
└── lib.scripthost.d.ts
```

### The DOM → ES2015 Transitive Dependency (TS 6.0)

Note that `lib.dom.d.ts` references `es2015` and `es2018.asynciterable`. When DOM is already loaded via a higher target (like es2017.full), the es2015 reference is deduplicated. But for `lib.d.ts` (ES5 target), this means DOM pulls in es2015 features that wouldn't otherwise be available.

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

`lib.es5.d.ts` also defines the `Promise` and `PromiseLike` **interfaces** (the type), but NOT the `Promise` constructor (the value). The constructor is in `lib.es2015.promise.d.ts`.

---

## tslib vs lib.d.ts

These are **completely different** things:

| Aspect | lib.d.ts | tslib |
|--------|----------|-------|
| **Type** | Type definitions (`.d.ts` files) | Runtime JavaScript (npm package) |
| **When used** | Compile time only | Runtime only |
| **Purpose** | Tell TS what types exist in the environment | Provide helper functions for downleveled code |
| **Installation** | Bundled with tsc | `npm install tslib` |
| **Flag** | `--lib`, `--noLib` | `--importHelpers` |
| **Affects output** | No (only type checking) | Yes (changes emitted JavaScript) |

### How tslib Works

When TypeScript compiles modern syntax to older targets (downleveling), it needs **helper functions** to implement features that don't exist natively in the target. For example, compiling `class Child extends Base {}` to ES5 requires an `__extends` helper.

#### Without `--importHelpers` (default): Inline Helpers

By default, tsc **inlines** these helper functions into every output file that needs them:

```typescript
// Input (TypeScript)
export class Child extends Base { y = 2; }
export const copy = [...arr, 4];
export async function hello() { return "world"; }
```

```javascript
// Output (ES5, no importHelpers) — 89 lines!
"use strict";
var __extends = (this && this.__extends) || (function () {
    var extendStatics = function (d, b) { /* ... 10+ lines ... */ };
    return function (d, b) { /* ... */ };
})();
var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    /* ... 8 lines ... */
};
var __generator = (this && this.__generator) || function (thisArg, body) {
    /* ... 30+ lines ... */
};
var __spreadArray = (this && this.__spreadArray) || function (to, from, pack) {
    /* ... 6 lines ... */
};
// ... actual code at the bottom
```

**Problem:** If you have 100 files that use `async/await`, the `__awaiter` and `__generator` helpers are duplicated 100 times in your bundle.

#### With `--importHelpers`: Shared tslib

With `--importHelpers`, tsc imports helpers from `tslib` instead of inlining them:

```javascript
// Output (ES5, with importHelpers) — 33 lines!
"use strict";
var tslib_1 = require("tslib");
function hello() {
    return tslib_1.__awaiter(this, void 0, void 0, function () {
        return tslib_1.__generator(this, function (_a) {
            return [2 /*return*/, "world"];
        });
    });
}
exports.copy = tslib_1.__spreadArray(tslib_1.__spreadArray([], arr, true), [4], false);
var Child = (function (_super) {
    tslib_1.__extends(Child, _super);
    // ...
}(Base));
```

With ESM modules, it uses named imports:
```javascript
import { __awaiter, __extends, __generator, __spreadArray } from "tslib";
```

### Common tslib Helpers

| Helper | Used For | Target Threshold |
|--------|----------|-----------------|
| `__extends` | `class Child extends Base` | < ES2015 |
| `__awaiter` / `__generator` | `async` / `await` | < ES2017 |
| `__spreadArray` | `[...arr]` spread | < ES2015 |
| `__assign` | `Object.assign` / `{...obj}` | < ES2015 |
| `__decorate` | `@decorator` | All targets (legacy decorators) |
| `__metadata` | `emitDecoratorMetadata` | All targets |
| `__rest` | `const {a, ...rest} = obj` | < ES2018 |
| `__asyncValues` | `for await (const x of y)` | < ES2018 |
| `__values` | `for (const x of y)` downlevel | < ES2015 |
| `__read` | Destructuring iterables | < ES2015 |
| `__makeTemplateObject` | Tagged templates | < ES2015 |
| `__classPrivateFieldGet/Set` | `#private` fields | < ES2022 |
| `__esDecorate` / `__runInitializers` | TC39 decorators | All targets (stage 3 decorators) |
| `__disposeResources` / `__addDisposableResource` | `using` / `await using` | All targets |

### When No Helpers Are Needed

When `--target` is high enough, the syntax passes through natively and no helpers are generated:

```typescript
// Input
export async function hello() { return "world"; }
export const copy = [...arr, 4];
export class Child extends Base { y = 2; }
```

```javascript
// Output with --target esnext (no helpers needed)
async function hello() { return "world"; }
const copy = [...arr, 4];
class Child extends Base { y = 2; }
```

### When to Use tslib

Use `--importHelpers` + `tslib` when:
1. **Many files** use the same modern features downleveled to ES5 — deduplication saves bundle size
2. **Library authors** want smaller published packages
3. **Monorepos** where many packages share the same helpers

Don't bother with tslib when:
1. **`--target` is modern** (ES2017+) — few or no helpers needed
2. **Single-file outputs** — no duplication to avoid
3. **Bundlers handle deduplication** — webpack/rollup can deduplicate inline helpers

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
  return targetLibMap[target] ?? 'lib.d.ts';
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
- This includes `lib.es2017.d.ts` → `lib.es2016.d.ts` → `lib.es2015.d.ts` → `lib.es2015.promise.d.ts`
- `Promise` is available

### TS 6.0 Implication for ES5 Tests

In TS 6.0, even `--target es5` (with no `--lib`) provides ES2015 features through the DOM → ES2015 reference chain. This means:
- `Promise`, `Map`, `Set`, `Symbol`, `Array.find()` all work on ES5 target
- Tests that use these features with ES5 target may pass in tsc but fail in tsz if tsz doesn't replicate this transitive loading

### Current tsz Behavior

tsz loads only the core lib (`es2017.d.ts`) without DOM/ScriptHost. For many conformance tests this works because:
1. Tests don't typically use DOM APIs
2. Core libs include all ES features needed via the reference chain
3. Tests that need DOM specify `@lib: dom` explicitly

However, tests that rely on **implicit ES2015 features via DOM on ES5 target** will fail.

### Edge Cases

Some tests may fail if they expect:
1. `console` to be available (requires `dom`)
2. `setTimeout` / `setInterval` (requires `dom`)
3. `fetch` (requires `dom`)
4. `Promise` on ES5 target without `@lib` (works in tsc 6.0 via DOM → ES2015, but not in tsz)

---

## tsz Implementation Requirements

To match tsc exactly, tsz must:

1. **Default lib selection**: Map target → correct default lib file (`.full` for ES2016+, `lib.d.ts` for ES5, `lib.es6.d.ts` for ES2015)
2. **Explicit `--lib`**: Completely replace defaults, resolve each lib name to file
3. **`--noLib`**: Disable all lib loading; error if combined with `--lib` (TS5053)
4. **Reference resolution**: Follow `/// <reference lib="..." />` directives recursively, including DOM's transitive references to ES2015
5. **Ordering**: Load libs in the correct order (affects overload resolution)

### Current Gap

tsz currently loads **core libs only** (e.g., `es5.d.ts` instead of `lib.d.ts`). This means:
- ❌ No DOM types by default
- ❌ No ScriptHost types by default
- ❌ No transitive ES2015 features on ES5 target (Promise, Map, Set, Array.find, etc.)
- ❌ `console.log()` fails without explicit `@lib: dom`

This was intentional for conformance testing but doesn't match tsc's actual behavior for real-world usage.

From `src/cli/config.rs`:
```rust
/// Returns the core lib name (without DOM) - matches tsc conformance test behavior.
pub fn default_lib_name_for_target(target: ScriptTarget) -> &'static str {
    match target {
        ScriptTarget::ES5 => "es5",     // tsc uses "lib" (which is es5+dom+webworker+scripthost)
        ScriptTarget::ES2015 => "es2015", // tsc uses "es6" (which is es2015+dom+dom.iterable+webworker+scripthost)
        ScriptTarget::ES2016 => "es2016", // tsc uses "es2016.full"
        // ...
    }
}
```

### Recommended Fix

To match tsc behavior, change `default_lib_name_for_target()` in `src/cli/config.rs`:

| Target | Current (tsz) | Correct (tsc) |
|--------|--------------|---------------|
| ES5 | `"es5"` | `"lib"` |
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
| Default ES5 | `{}` | lib.d.ts → ES5 + DOM (→ ES2015 transitively) + WebWorker.ImportScripts + ScriptHost |
| Default ES2020 | `{"target": "ES2020"}` | lib.es2020.full.d.ts → ES2020 + DOM + ScriptHost |
| Node.js | `{"lib": ["ES2020"]}` | ES2020 only (no DOM, no transitive ES2015 from DOM) |
| Browser | `{"lib": ["ES2020", "DOM"]}` | ES2020 + DOM |
| Minimal | `{"noLib": true}` | Nothing (must provide own types) |

### Error Codes

| Code | Message | Cause |
|------|---------|-------|
| TS2318 | Cannot find global type 'X' | Missing lib file or `noLib: true` |
| TS2583 | Cannot find name 'X'. Do you need to install type definitions? | Missing lib or @types package |
| TS2584 | Cannot find name 'X'. Do you need to change your target library? | Need higher ES version or specific lib |
| TS5053 | Option 'lib' cannot be specified with option 'noLib'. | Combined `--lib` with `--noLib` |
| TS5108 | Option 'target=ES3' has been removed. | ES3 target removed in TS 6.0 |

---

## References

- [TypeScript tsconfig lib option](https://www.typescriptlang.org/tsconfig/lib.html)
- [TypeScript tsconfig target option](https://www.typescriptlang.org/tsconfig/target.html)
- [TypeScript tsconfig noLib option](https://www.typescriptlang.org/tsconfig/noLib.html)
- [TypeScript source: utilitiesPublic.ts](https://github.com/microsoft/TypeScript/blob/main/src/compiler/utilitiesPublic.ts)
- [TypeScript source: commandLineParser.ts](https://github.com/microsoft/TypeScript/blob/main/src/compiler/commandLineParser.ts)
- [TypeScript lib files](https://github.com/microsoft/TypeScript/tree/main/src/lib)
- [tslib on npm](https://www.npmjs.com/package/tslib)
