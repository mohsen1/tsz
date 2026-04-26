---
title: Sound Mode
layout: layouts/base.njk
page_class: sound-mode
permalink: /sound-mode/index.html
extra_scripts: <script src="https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs/loader.js"></script><script src="/sound-mode-editors.js"></script>
---

# Sound Mode

<div class="alert alert-warning">
  <strong>Experimental</strong> - Sound Mode is an opt-in rollout plan, focused on a safer default for user-authored TypeScript code.
</div>

**tsz** is planning Sound Mode as a staged feature: stricter checks, intentionally introduced in small steps so teams can adopt safely.

## What Sound Mode will do

At its core, Sound Mode is meant to make project source code safer without requiring the whole ecosystem to be perfect first.

This starts as a narrow, practical profile for TypeScript users: stronger checks inside your own application source, shipped in small steps.

## <span id="sound-mode-catch-examples">What it will catch</span>

Sound Mode is aimed at teams already using `tsc --strict`. It catches a specific class of runtime bugs that `--strict` intentionally leaves behind, and it promotes three opt-in tsc flags — [`useUnknownInCatchVariables`](https://www.typescriptlang.org/tsconfig/#useUnknownInCatchVariables), [`noUncheckedIndexedAccess`](https://www.typescriptlang.org/tsconfig/#noUncheckedIndexedAccess), and [`exactOptionalPropertyTypes`](https://www.typescriptlang.org/tsconfig/#exactOptionalPropertyTypes) — to always-on defaults.

Every example below compiles without error under `tsc --strict`. Lines marked 💥 are runtime crashes.

### 1) Method parameters must be contravariant

TypeScript's `strictFunctionTypes` enforces variance for function types written as `(x: T) => R`. It intentionally **exempts method syntax** — a known, documented gap that affects virtually every class-based interface and event-handler pattern. Sound Mode closes it.

```ts
interface Formatter {
  format(value: unknown): string; // method syntax
}

class NumberFormatter implements Formatter {
  // tsc --strict: OK — method params are checked bivariantly
  format(value: number): string {
    return value.toFixed(2);
  }
}

const fmt: Formatter = new NumberFormatter();
fmt.format("hello"); // 💥 "hello".toFixed is not a function
```

```ts
// Fix: widen the parameter to match the interface
class NumberFormatter implements Formatter {
  format(value: unknown): string {
    if (typeof value !== "number") throw new TypeError("expected number");
    return value.toFixed(2);
  }
}

// Alternatively, function-property syntax is already checked contravariantly by tsc:
const fmt: Formatter = {
  format: (value: unknown): string => String(value),
};
```

### 2) Writing `any` in your source is an error

Sound Mode bans explicit `any` annotations in user-authored TypeScript files. Use `unknown` and narrow to the shape you actually need. Declaration files (`.d.ts`) and `node_modules` are exempt — your dependencies do not need to be rewritten first.

```ts
// tsc --strict: no errors
function parseRow(raw: any): any {
  return { id: raw.id, name: raw.name };
}
```

```ts
// Sound Mode: both `any` annotations are errors
// Fix: use unknown
function parseRow(raw: unknown): { id: unknown; name: unknown } {
  if (typeof raw !== "object" || raw === null) throw new TypeError("expected object");
  const { id, name } = raw as Record<string, unknown>;
  return { id, name };
}
```

### 3) Catch variables must be narrowed before use

Sound Mode behaves as if `useUnknownInCatchVariables` is always enabled. The `err` binding in every catch block is typed `unknown`, not `any`.

```ts
async function loadConfig(path: string) {
  try {
    return JSON.parse(await fs.readFile(path, "utf8"));
  } catch (err) {
    // tsc (default): err is any — no complaint
    // Sound Mode: err is unknown — must narrow before accessing properties
    console.error("Failed:", err.message); // 💥 err might be a string, number, or anything
  }
}
```

```ts
// Fix: narrow err before accessing it
} catch (err) {
  const msg = err instanceof Error ? err.message : String(err);
  console.error("Failed:", msg);
}
```

### 4) Index access may return `undefined`

Sound Mode behaves as if `noUncheckedIndexedAccess` is always enabled. Every bracket-index read — on arrays, tuples, and index-signature objects — is typed `T | undefined`.

```ts
function runFirst(queue: (() => void)[]) {
  // tsc: queue[0] is `() => void`
  // Sound Mode: queue[0] is `(() => void) | undefined`
  queue[0](); // 💥 crashes on an empty queue
}

const routes: Record<string, () => Response> = {};
const handler = routes["/home"];
handler(); // 💥 route may not be registered
```

```ts
// Fix: guard before calling
function runFirst(queue: (() => void)[]) {
  queue[0]?.();
}

const handler = routes["/home"];
if (handler) handler();
```

### 5) Optional properties cannot be explicitly assigned `undefined`

Sound Mode behaves as if `exactOptionalPropertyTypes` is always enabled. A property typed `timeout?: number` means the key may be **absent** — not that it can be present with the value `undefined`. This distinction matters at runtime in `"key" in obj` checks and `Object.assign` merges.

```ts
interface RequestConfig {
  timeout?: number;
}

// tsc: OK. Sound Mode: error — undefined is not assignable to number
const config: RequestConfig = { timeout: undefined };

function applyDefaults(base: RequestConfig, overrides: Partial<RequestConfig>) {
  // Without exactOptionalPropertyTypes, a Partial<T> can carry `{ timeout: undefined }`
  // and silently overwrite a valid timeout with undefined.
  return { ...base, ...overrides };
}
```

## What this is and is not

Sound Mode is not a full theorem of language soundness, and it does not require every third-party declaration to be fully strict from day one. The direction is to give stronger checks where your team controls the source first and add stronger declaration-boundary work in later phases.

## Further reading

The detailed plan is tracked in [SOUND_MODE.md](https://github.com/mohsen1/tsz/blob/main/docs/plan/SOUND_MODE.md), and broader milestones are in the [Internal Roadmap](https://github.com/mohsen1/tsz/blob/main/docs/plan/ROADMAP.md).
