# Sound Mode

**Status**: Partially Implemented
**Last Updated**: April 2026

## Quick Start

```bash
# Enable sound mode via CLI (current implementation)
tsz check --sound src/

# Planned tsconfig shape (not wired yet)
{
  "compilerOptions": {
    "sound": true
  }
}

# Per-file pragma (planned — not yet parsed from source)
// @tsz-sound
```

## Implementation Status

Today, sound mode is effectively a **CLI-only boolean** (`--sound`). The planned `sound` tsconfig option, per-file pragma, and server/LSP exposure are not wired yet. The live implementation works by tightening the existing Lawyer (`CompatChecker`) with `strict_subtype_checking` and `strict_any_propagation` flags on `RelationPolicy`. Errors are still emitted as standard TS diagnostic codes (TS2322, TS2345, etc.), not dedicated TSZ-family sound diagnostics yet.

**Note on diagnostic codes:** The codebase defines `SoundDiagnosticCode` with codes TS9001–TS9005 in `crates/tsz-solver/src/sound.rs`. Those should be treated as temporary implementation placeholders, not the public contract. This doc uses the target **family-based TSZNNNN taxonomy** (`TSZ1000`, `TSZ2000`, etc.), which has not yet been applied to the implementation. A `DiagnosticDomain::Sound` infrastructure exists but is not yet used by the checker.

| Feature | Target Code | Impl Code | Status | Mechanism |
|---------|------------|-----------|--------|-----------|
| Method bivariance | TSZ2002 | TS9003 | LIVE | `strict_subtype_checking` disables method bivariance |
| `any` restriction | TSZ1001 | TS9004 | LIVE | `strict_any_propagation` → `TopLevelOnly` mode |
| Sticky freshness | TSZ3006 | TS9001 | LIVE | Skips `widen_freshness()` in 4 checker locations |
| Mutable array covariance | TSZ2001 | TS9002 | Dead code | `SoundLawyer.check_array_covariance()` exists but not wired into pipeline |
| Enum-number assignment | TSZ6001 | TS9005 | Dead code | `SoundModeConfig.strict_enums` defined but unused |
| Unsafe type assertion | TSZ1011 | — | Planned | No implementation |
| Unchecked indexed access | TSZ5001 | — | Planned | Not auto-enabled by `sound: true`; uses standard `noUncheckedIndexedAccess` flag separately |
| Missing index signature | TSZ3007 | — | Planned | Currently uses standard TS2329 |

**Known implementation gaps:**
- `SoundLawyer` struct in `sound.rs` is fully defined but never called from the checker pipeline (dead code)
- `SoundModeConfig` struct with granular flags is defined but never consumed
- `strict_subtype_checking` is not reflected in the `RelationCacheKey` — potential cache correctness issue
- The `any_mode: u8` field in `RelationCacheKey` partially differentiates sound vs non-sound cache entries
- The tsconfig `sound` option described in this doc is not currently parsed into `CheckerOptions`
- `tsz-server` / LSP currently hardcodes `sound_mode: false`

See `crates/tsz-solver/src/sound.rs` for sound mode definitions and `crates/tsz-checker/src/query_boundaries/assignability.rs` for the live integration.

---

## The Sound Mode Contract

TypeScript explicitly lists "apply a sound or provably correct type system" as a **non-goal**. tsz Sound Mode needs to be crystal-clear about what it guarantees, what it deliberately refuses to assume, and how users cross trust boundaries safely.

### What "Sound" Means in tsz

1. **Within user-authored Sound Mode code**, tsz rejects the known TypeScript unsound patterns that can cause type-driven runtime exceptions — calling methods that aren't there, indexing missing keys, writing through an alias with the wrong element type.

2. **User code is `any`-less by default**: in normal source files, developers do not write explicit `any`. They use `unknown`, narrowing, validators, or explicit suppressions instead.

3. **Declaration files are treated as trust boundaries, not as adoption blockers**: third-party `.d.ts` files may still contain `any`, bivariance hacks, and other legacy patterns, but those unsound types are quarantined before they flow into sound user code.

4. **Any use of unsoundness must be explicit and auditable** (`// @tsz-unsound TSZNNNN: reason`) and should be trackable in CI as technical debt.

This framing matches the reality that TypeScript won't add runtime checks; Sound Mode forces users to do validation where it matters.

### Scope Model

The key product decision is that Sound Mode should optimize for **developer life in application code**, not for perfectly purifying the entire npm ecosystem on day one.

Default scope:

1. **Checked as sound user code**: `.ts`, `.tsx`, `.mts`, `.cts` source files that are part of the project and are not declaration files.
2. **Treated as trust-boundary inputs**: `.d.ts`, `.d.mts`, `.d.cts`, `.d.tsx`, default libs, and third-party declaration files under `node_modules`.
3. **Not the primary ergonomics target**: soundness inside declaration internals themselves. The first priority is what user code can rely on after crossing the boundary.

Planned follow-on options:

1. `soundPedantic: true` for bug-finding heuristics in user code
2. `soundCheckDeclarations: true` for teams that also want first-party declaration files checked as sound source
3. `soundReportOnly: true` for staged rollout without failing CI immediately

This default keeps the product promise simple:

1. **Your source files stop writing `any`.**
2. **Third-party declarations can still exist as they are today.**
3. **What crosses from those declarations into your code becomes a sound boundary problem, usually `unknown`, instead of silently poisoning your program.**

### What Sound Mode Does NOT Guarantee

- Runtime immutability (JavaScript is fundamentally mutable; type erasure applies)
- Protection against code outside Sound Mode scope (non-sound files, native modules)
- That all possible runtime errors are caught (only *type-driven* ones)
- That third-party declaration files are themselves internally perfect or truthful
- That external JavaScript implementations honor their declarations without validation

---

## Two-Layer Configuration

> **Status: Planned design.** Currently sound mode is a single `bool` (`CheckerOptions.sound_mode`). The planned direction is a public `sound` master switch plus a flat family of sibling `sound*` compiler options. `sound: true` means "use the default sound profile"; companion flags such as `soundPedantic`, `soundReportOnly`, or `soundArrayVariance` refine rollout or targeted checks. A nested `sound: { ... }` object should **not** be the public tsconfig shape. A `SoundModeConfig` struct with individual flags exists in `sound.rs` but is dead code — not wired into the pipeline.

Sound Mode is a **dial**, not a single switch. The configuration separates core runtime-safety checks from bug-finding heuristics, while also preserving the user-code-vs-declaration-boundary distinction:

### Layer A: `sound: true` (Core Runtime Safety)

These checks address patterns where the doc can truthfully say: **"tsc allows it; this can crash."** Every check in this layer corresponds to a known TypeScript unsoundness that produces type-driven runtime exceptions.

Core Layer defaults:

1. Ban explicit `any` in user-authored non-declaration code
2. Treat external declaration `any` as a boundary hazard, not as a bottom type inside user code
3. Prefer `unknown` at readable boundaries (`return`, readable properties, callback parameters flowing into user code)
4. Keep declaration-file internals out of the primary diagnostic surface unless explicitly opted in

| Diagnostic | Description |
|-----------|-------------|
| TSZ2001 | Covariant mutable arrays |
| TSZ2002 | Method parameter bivariance |
| TSZ2003 | Covariant `this` type |
| TSZ1001 | `any` escape / taint detection |
| TSZ1011 | Unsafe type assertions |
| TSZ1021 | Non-null assertion escape |
| TSZ1022 | Definite assignment assertion |
| TSZ1031 | Catch variables default to `unknown` |
| TSZ3001 | Weak type acceptance |
| TSZ3003 | Readonly property aliasing |
| TSZ3004 | Split accessor assignability |
| TSZ3005 | Unsafe `Object.assign` |
| TSZ3007 | Missing index signature |
| TSZ4001 | Rest parameter bivariance |
| TSZ4002 | Void return exception |
| TSZ4003 | Opaque `Function` type |
| TSZ5001 | Unchecked indexed access |
| TSZ5002 | Strict array/set membership |
| TSZ5003 | Non-empty array reduction |
| TSZ5004 | Tuple length mutation |
| TSZ6001 | Enum-number assignment |
| TSZ6002 | Implicit primitive boxing |
| TSZ8001 | Switch statement exhaustiveness |
| TSZ3008 | Exact optional property types |

### Layer B: `soundPedantic: true` (Bug-Finding Heuristics)

These checks are useful bug detectors but are not strictly about soundness in a structural type system. Extra properties don't cause runtime crashes — they're more of a typo-catching heuristic.

| Diagnostic | Description |
|-----------|-------------|
| TSZ3006 | Sticky freshness (unless tied to `Exact<T>`) |
| TSZ3002 | Exact types & object iteration (when not using `Exact<T>` semantics) |

### Planned Config Defaults

```jsonc
{
  "compilerOptions": {
    "sound": true,
    "soundPedantic": false,
    "soundCheckDeclarations": false,
    "soundReportOnly": false
  }
}
```

Default profile semantics:

1. `sound: true`: enable the default sound profile
2. `soundPedantic`: add bug-finding heuristics that go beyond strict runtime-safety checks
3. `soundCheckDeclarations`: opt first-party declaration files into sound checking too
4. `soundReportOnly`: report sound diagnostics without making them fail the run
5. The default sound profile itself still bans explicit `any` in user source, treats declaration unsoundness as a boundary problem, and may later swap in sound library surfaces where available

### Public Config Rule

To keep the product understandable, Sound Mode should expose one coherent **flat `sound*` family**:

1. `sound: true` means "enable the default sound profile"
2. Companion flags such as `soundPedantic`, `soundReportOnly`, `soundCheckDeclarations`, and future flags like `soundArrayVariance` live beside it at top level
3. Future advanced knobs should follow that same `sound*` naming family
4. We should **not** support a competing nested `sound: { ... }` object form in public tsconfig docs

This keeps the config model coherent:

1. one naming family
2. easy grep/discovery in `tsconfig.json`
3. no nested-object parsing story for one experimental feature
4. room for incremental adoption with targeted `sound*` flags

### Configuration Examples

```jsonc
// Core soundness only (recommended starting point)
{
  "compilerOptions": {
    "sound": true
  }
}

// Core + pedantic bug-finding heuristics
{
  "compilerOptions": {
    "sound": true,
    "soundPedantic": true
  }
}

// Core soundness + also check first-party declaration files
{
  "compilerOptions": {
    "sound": true,
    "soundCheckDeclarations": true
  }
}

// Migration mode: report but do not fail CI yet
{
  "compilerOptions": {
    "sound": true,
    "soundReportOnly": true
  }
}

// Targeted sound rollout
{
  "compilerOptions": {
    "sound": true,
    "soundPedantic": true,
    "soundReportOnly": true,
    "soundArrayVariance": true,
    "soundMethodVariance": true
  }
}
```

This prevents Sound Mode from feeling like "it hates JavaScript patterns" while keeping the strong guarantees for runtime safety.

---

## Executive Summary

TypeScript is **intentionally unsound**. The TypeScript team made deliberate design choices to prioritize developer ergonomics over type-theoretic correctness. These choices are documented in [docs/specs/TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md).

tsz's **Judge/Lawyer architecture** separates concerns:
- **Judge (Core Solver)**: Implements strict, sound set-theory semantics
- **Lawyer (Compatibility Layer)**: Applies TypeScript-specific rules to match tsc behavior

This architecture enables **Sound Mode** — an opt-in flag that bypasses the Lawyer layer, exposing the Judge's stricter checking. Sound Mode catches real bugs that TypeScript allows by design, at the cost of rejecting some valid TypeScript patterns.

---

## Diagnostics as a Product Surface

> **Status: Planned design.** The `DiagnosticDomain::Sound` and `SoundDiagnosticCode` enum exist in code but are not used by the checker. None of the CLI features below (`tsz explain`, `--sound-summary`) or suppression mechanisms (`@tsz-unsound`) are implemented yet.

### Code Families

Public Sound Mode diagnostics should use stable **topic families**, not a single 9000-series block:

| Range | Family | Typical Checks |
|------|--------|----------------|
| TSZ1000-1099 | Trust Boundaries & Escape Hatches | `any`, assertions, non-null escapes, catch `unknown` |
| TSZ2000-2099 | Variance & Subtyping | arrays, methods, `this` variance |
| TSZ3000-3099 | Object Shapes & Mutation | weak types, readonly aliasing, exact optional properties |
| TSZ4000-4099 | Function & Callable Surfaces | rest args, `void`, `Function` |
| TSZ5000-5099 | Collections & Indexing | unchecked access, membership, tuples |
| TSZ6000-6099 | Primitives & Enums | enum/number mixing, boxing |
| TSZ7000-7099 | Generics & Type Algebra | conditionals, intersections, generic escapes |
| TSZ8000-8099 | Control Flow & Exhaustiveness | switches and reachability |

Design rule:

1. Public docs, suppressions, and CI summaries should speak in these families.
2. Implementation placeholders like `TS9001`-`TS9005` may exist temporarily, but should not leak into the user-facing contract.
3. New checks should join the most specific family that matches the user-visible remediation story.

### TSZNNNN Diagnostic Requirements

Every public TSZNNNN sound diagnostic must include:

1. **"Why this is unsafe"** — a one-paragraph explanation with a minimal crash example
2. **"How to fix"** — 2–3 common refactoring patterns
3. **"How to suppress"** — with required reason string format

### CLI Explain Command

```bash
# Explain any sound mode diagnostic
tsz explain TSZ2002

# Output:
# TSZ2002: Method Parameter Bivariance
#
# WHY THIS IS UNSAFE:
#   TypeScript allows method parameters to be checked bivariantly...
#   [minimal crash example]
#
# HOW TO FIX:
#   1. Widen the subclass method parameter type
#   2. Use function-property syntax instead of method syntax
#   3. Use overloads for event-map patterns
#
# HOW TO SUPPRESS:
#   // @tsz-unsound TSZ2002: validated by runtime guard in handler.ts
```

### Sound Summary Mode

```bash
# Overview of sound mode diagnostics in a project
tsz check --sound --sound-summary src/

# Output:
# Sound Mode Summary
# ──────────────────
# TSZ2001  12 errors  (3 suppressed)   src/models/ (8), src/utils/ (4)
# TSZ1001   7 errors  (1 suppressed)   src/api/ (5), src/lib/ (2)
# TSZ5001   4 errors  (0 suppressed)   src/handlers/ (4)
# ──────────────────
# Total: 23 errors, 4 suppressed (17% suppression rate)
```

### Suppression Design

Suppressions follow the best part of `@ts-expect-error` but are stricter:

1. **Targeted** — must name a specific TSZ code
2. **Sticky** — if the error disappears, the suppression itself becomes an error
3. **Reason required** — enforced formatting with explanation

```typescript
// ✅ Valid suppression
// @tsz-unsound TSZ1011: validated by legacy runtime guard in foo.ts

// ❌ Invalid: no code specified
// @tsz-unsound: trust me

// ❌ Invalid: no reason
// @tsz-unsound TSZ1011

// ❌ Invalid: suppression without matching error (becomes an error itself)
// @tsz-unsound TSZ2001: array is readonly  ← no TSZ2001 error here
```

This is the difference between "sound mode is strict" and "sound mode is adoptable."

**Suppression semantics**: `@tsz-unsound` is strictly **diagnostic-only (suppress-only)**. It locally mutes the error output but does **not** act as a semantic escape hatch. The compiler still evaluates the AST node using strict sound semantics. This is crucial for caching and performance: if suppression changed semantics, it would invalidate the cache for that specific node and cascade changes to downstream inference and overload resolution. If users need true TS-compatible semantics at a boundary, they must use explicit types or casts, not a suppression comment.

---

## Why Sound Mode?

TypeScript's unsoundness causes real runtime errors:

```typescript
// This compiles but crashes at runtime
const dogs: Dog[] = [new Dog()];
const animals: Animal[] = dogs;  // tsc allows (covariant arrays)
animals.push(new Cat());
dogs[0].bark();  // Works
dogs[1].bark();  // 💥 Cat has no bark()
```

Sound Mode rejects the problematic assignment, catching the bug at compile time.

**Use cases for Sound Mode:**
- Safety-critical code (financial, medical, infrastructure)
- Library authors wanting stronger guarantees
- Teams willing to trade ergonomics for correctness
- Gradual migration to stricter typing

---

## What Sound Mode Catches

### Variance & Subtyping

#### TSZ2001: Covariant Mutable Arrays

> **Implementation: PARTIAL.** `SoundLawyer.check_array_covariance()` in `sound.rs` can detect covariant array assignments, but `SoundLawyer` is not wired into the checker pipeline. The live `SubtypeChecker` still treats arrays covariantly in sound mode. The diagnostic helper exists but is disconnected from the assignability flow.

**TypeScript allows:**
```typescript
const dogs: Dog[] = [new Dog()];
const animals: Animal[] = dogs;  // ✅ tsc
animals.push(new Cat());         // 💥 Runtime: Cat in Dog[]
```

**Sound Mode:** Reject `Dog[]` → `Animal[]` for mutable arrays. Only `readonly Dog[]` → `readonly Animal[]` is safe.

**How to fix:**
- Prefer `readonly` in APIs that only read: accept `readonly Animal[]` if you only read
- Return `readonly` from producers
- Break aliasing when mutability is needed: `const animals: Animal[] = [...dogs];` (copying prevents corruption of the original array)

**Design principle:** In sound mode, mutable generic containers should be invariant unless proven otherwise. This applies beyond arrays to any mutable container type.

#### TSZ2002: Method Parameter Bivariance

> **Implementation: LIVE.** `strict_subtype_checking` → `disable_method_bivariance = true` in `SubtypeChecker`. Activated via `RelationPolicy` in `assignability.rs:103`. Errors emit as standard TS codes (TS2322/TS2345), not TSZ2002. The `@tsz-bivariant` annotation mechanism is planned but not implemented.

This is the linchpin of Sound Mode. TypeScript documents that even `strictFunctionTypes` intentionally does not apply to method syntax because of unsafe hierarchies (including DOM).

**TypeScript allows:**
```typescript
class Animal {
    feed(food: Food) {}
}
class Dog extends Animal {
    feed(food: DogFood) {}  // ✅ tsc allows narrower param (bivariant)
}

const animal: Animal = new Dog();
animal.feed(new CatFood());  // 💥 Runtime: Dog gets CatFood
```

**Sound Mode:** Enforce contravariance for method parameters. Subclass methods must accept *at least* what the superclass accepts.

**How to fix:**
1. Widen the subclass method parameter type to match or exceed the parent
2. Use function-property syntax (`foo: (x: T) => R` instead of `foo(x: T): R`) so variance rules are "function-like"
3. Use overloads for event-map patterns (TS docs themselves point to overload workarounds)

**Bivariant-on-purpose annotation:** For declarations that intentionally use bivariance (e.g., DOM event handlers), provide an explicit opt-out mechanism:
- SoundlyTyped pipeline can mechanically convert method signatures to function-valued properties
- Support a JSDoc tag on method parameters: `/** @tsz-bivariant */`
- A practical mechanical transformation: convert `foo(x: T): R` → `foo: (x: T) => R` in object types

**Library patch strategy:** SoundlyTyped can automatically rewrite bivariant methods in upstream `.d.ts` files to use function-property syntax where safe.

#### TSZ2003: Covariant `this` Type

**TypeScript allows:**
```typescript
class Box {
    compare(other: this) { ... }
}
class StringBox extends Box {
    compare(other: StringBox) { ... }  // ✅ tsc allows tighter `this`
}

const b: Box = new StringBox();
b.compare(new Box());  // 💥 Runtime: StringBox.compare gets wrong type
```

**Sound Mode:** Make classes with `this` in contravariant positions invariant.

### Trust Boundaries & Escape Hatches

`any` is the biggest source of unsoundness in TypeScript. It infects anything it touches by acting as both a top and a bottom type.

#### TSZ1001: `any` Taint Detection

> **Implementation: LIVE (partial).** `strict_any_propagation` → `AnyPropagationMode::TopLevelOnly` restricts `any` at nested depths via `STRICT_ANY` demotion in `SubtypeChecker`. The `SoundLawyer.is_assignable()` correctly implements the full taint model (`any` only assignable to `any`/`unknown`) but is dead code — not called from the checker. The live path is thinner: `any` is restricted at depth > 0 but still works at top-level.

In the target design, `any` is treated as a **boundary taint**, not as normal source-language vocabulary for sound user code.

**TypeScript allows:**
```typescript
const x: any = "hello";
const y: number = x;      // ✅ tsc: any → number
y.toFixed(2);             // 💥 Runtime: "hello".toFixed()
```

**Sound Mode rules:**
1. In user-authored non-declaration files, developers are banned from writing explicit `any` — use `unknown` instead
2. `any` is never allowed to behave like a bottom type inside sound user code
3. Declaration-origin `any` is tolerated as an ecosystem input, but only as a quarantine source
4. At the point declaration-origin `any` becomes observable in user code, it is treated as `unknown` in readable positions unless an overlay provides something better
5. `any` member access / call / `new` in sound user code is a dedicated error unless the value has been validated, narrowed, or explicitly suppressed

Readable boundary positions that should become `unknown` by default:

1. Function and method return types from declaration files
2. Readable properties and getter return types
3. Iterator / async iterator yielded values exposed to user code
4. Callback parameters supplied by external libraries into user-authored handlers

Positions that should **not** be naively rewritten:

1. Purely type-level plumbing inside declaration internals (conditional helpers, constraints, infer machinery)
2. Write-only positions where the user is sending values *into* a library
3. Declaration internals that are preserved specifically to avoid breaking DT-style metaprogramming

**How to fix:**
- Replace `any` with `unknown` and add narrowing
- Use runtime validation (Zod, ArkType) at trust boundaries
- Use the sound core libraries that return `unknown` from `JSON.parse`, `fetch().json()`, etc.

**Diagnostic shape (planned):**

TSZ1001 should likely stay a single umbrella code with multiple targeted messages instead of exploding into many codes too early:

1. **Explicit any in sound user code** — "Type annotation `any` is not allowed in Sound Mode user code. Use `unknown` or a specific type."
2. **External any crossing a readable boundary** — "Value from declaration boundary is `any`; Sound Mode exposes it as `unknown` until validated."
3. **Unsafe any operation** — "Cannot access/call value of tainted `any` in Sound Mode without narrowing."

#### TSZ1011: Unsafe Type Assertions

> **Implementation: Planned.** No code exists for type assertion checking in sound mode. No `SoundDiagnosticCode` variant for assertions. The widening-only restriction and sideways cast detection are entirely unimplemented.

TypeScript's own "Soundness" documentation calls out type assertions as a core unsoundness mechanism.

**Sound Mode rules:**
1. **Ban "sideways" casts** even through `unknown`/`any`: `cat as unknown as Dog` is rejected
2. **Allow only widening assertions**: assertions where the source is assignable to the target (i.e., going from specific to general)
3. **Narrowing assertions require justification**: either a suppression with reason, or a recognized refinement pattern (`instanceof`, `in`, discriminants, `asserts` functions, schema parsers)

```typescript
// ✅ Widening assertion (source assignable to target)
const x: "hello" = "hello";
const y = x as string;  // OK: "hello" is assignable to string

// ❌ Narrowing assertion (requires suppression or refinement)
const a: string = "hello";
const b = a as "hello";  // Error: use narrowing instead

// ❌ Sideways cast (always rejected)
const cat: Cat = { meow() {} };
const dog = cat as unknown as Dog;  // Error: disjoint types
```

**How to fix:**
- Use `instanceof`, `in`, discriminant checks
- Use `asserts` functions for custom type guards
- Use schema parsers (Zod, ArkType) for external data
- If truly needed: `// @tsz-unsound TSZ1011: validated by runtime check in validator.ts`

#### TSZ1021: Non-Null Assertion Escape

The non-null assertion operator (`!`) is an explicit unsound escape hatch, just like type assertions.

```typescript
function getUser(): User | null { return null; }

const user = getUser()!;  // ✅ tsc: trust me, it's not null
user.name;                 // 💥 Runtime: Cannot read properties of null
```

**Sound Mode:** Flag `!` as an unsound escape that requires suppression or a recognized null-check pattern.

#### TSZ1022: Definite Assignment Assertions

Class field definite assignment assertions (`!:`) bypass initialization checks.

```typescript
class Service {
  db!: Database;  // "trust me, it'll be initialized"

  query() {
    this.db.execute("SELECT 1");  // 💥 if init() never called
  }
}
```

**Sound Mode:** Flag `!:` on class fields as an unsound assertion requiring suppression.

#### TSZ1031: Catch Variables Default to `unknown`

> **Implementation: Feasible.** This is effectively a policy decision layered on top of existing TypeScript behavior. In practice, sound mode should force the equivalent of `useUnknownInCatchVariables = true` regardless of broader project defaults.

**TypeScript allows (historically):**
```typescript
try { throw { message: "oops" }; }
catch (e) {
    e.toUpperCase(); // ✅ tsc allows if useUnknownInCatchVariables is false
}
```

**Sound Mode:** Strictly enforce `unknown` for catch variables. Developers *must* type-narrow or cast `e` before interacting with it.

#### Ecosystem Boundaries & Strategies

Sound Mode's biggest blocker is ecosystem typing hygiene. It relies on **explicit boundary enforcement** via automated structural transformations of the ecosystem's definitions.

The central strategy is:

1. Do **not** make developers clean npm by hand before they can use Sound Mode.
2. Do **not** thread declaration-origin provenance through every solver relation if a source transformation can solve the problem more locally and transparently.
3. Prefer **rewrite-at-the-boundary** over **global taint semantics everywhere**.

**1. `SoundlyTyped`: Automated Ecosystem Soundness (Boundary Overlay)**
To handle the massive ecosystem of `node_modules` and DefinitelyTyped (DT), we introduce **`SoundlyTyped`** — a fully automated infrastructure layer acting as a boundary-aware overlay, rather than a naive token-replacement fork.
- **Boundary-Aware Transformation:** Instead of replacing every `any` (which breaks DT's internal type-level machinery), it maps `any` to `unknown` specifically at **value flow boundaries** — return types, readable properties, and callback arguments flowing into consumer code.
- **Pattern Rewrites over Bans:** Rewrites bivariance hacks (e.g., React patterns) and method-to-property variances to sound equivalents.
- **Cached Overlays:** Transforms are cached by package/version/hash to keep it fast and incremental.

Planned transformation priority:

1. Core libs first (`JSON.parse`, `Response.json`, DOM queries)
2. High-value ecosystem packages next (`react`, `node`, fetch/http clients, schema libs)
3. Long tail packages through deterministic cached overlays

**Boundary conversion rules for overlays:**

1. `() => any` in external declarations becomes `() => unknown` when the return value flows into user code
2. `readonly prop: any` becomes `readonly prop: unknown`
3. `handler(cb: (value: any) => void)` becomes `handler(cb: (value: unknown) => void)`
4. Internal helper aliases like `type DeepPartial<T = any>` are left alone unless they leak through a readable surface
5. Generic defaults / constraints are preserved unless rewriting them is required to stop concrete unsound flow

This is the part that makes the "any-less user code" story actually adoptable.

**2. Sound Core Libraries (`*.sound.lib.d.ts`)**
Standard libraries often return `any` for ergonomics. A true Sound Mode defaults to sound alternative core typings (e.g., `dom.sound.lib.d.ts`). For instance, `JSON.parse` and `fetch().json()` return `unknown`. This prevents `any` from entering the program in the first place.

**3. Developer Strategies**
When consuming code without `SoundlyTyped` guarantees:
- **Runtime Validation:** Use Zod or ArkType.
- **Explicit Module Augmentation:** Override unsound exports manually.
- **Deep Type Transformation:** Use utilities like `ReplaceAnyDeep<T, unknown>` at the boundary.

**First-party `.d.ts` policy (planned):**

1. Default: first-party declaration files are exempt from the explicit-`any` ban, just like third-party declaration files
2. Reason: forcing declaration cleanup before source adoption makes the first rollout much harder
3. Future opt-in: `checkDeclarations: true` can apply the same no-`any` discipline to first-party `.d.ts` once a team is ready

### Object Shapes & Mutation

#### TSZ3001: Weak Type Acceptance

**TypeScript allows (with warning):**
```typescript
interface Config { port?: number; host?: string; }
const opts = { timeout: 5000 };  // No overlap with Config
const config: Config = opts;     // ⚠️ tsc warns but allows
```

**Sound Mode:** Strictly reject objects with no overlapping properties.

#### TSZ3002: Exact Types & Object Iteration

**TypeScript allows:**
```typescript
interface User { name: string; }
const admin = { name: "Alice", role: "admin" };
const user: User = admin;  // ✅ tsc allows structural subtyping

// Later, passing this to an object iterator:
Object.keys(user).forEach(key => {
    // 💥 Runtime: visits "role", which is invisible to the type system!
});
```

**Sound Mode and `Exact<T>`:**

`Exact<T>` is the biggest "design win" opportunity. TypeScript doesn't have exact/sealed object types today; it's a long-standing request.

Design principles:
- **Opt-in and non-infectious**: only values created as exact (object literal, `Exact.of`, or validated) are `Exact<T>`
- **One-way assignability**: `Exact<T>` is assignable to `T`, but not the other way around without proof
- **Document in ecosystem terms**: "sealed objects" vs "open objects", and "why `Object.keys` can't be `(keyof T)[]` without exactness"

```typescript
// Exact<T> is opt-in
function getKeys<T>(obj: Exact<T>): (keyof T)[] {
  return Object.keys(obj) as (keyof T)[];  // Sound!
}

// Open objects (default) don't get this guarantee
function getKeysUnsafe<T>(obj: T): string[] {
  return Object.keys(obj);  // Returns string[], not (keyof T)[]
}
```

#### TSZ3003: Readonly Property Aliasing

**TypeScript allows:**
```typescript
interface Immutable { readonly id: number; }
interface Mutable { id: number; }

const ro: Immutable = { id: 1 };
const mut: Mutable = ro;  // ✅ tsc allows dropping 'readonly'
mut.id = 2;               // 💥 Runtime: ro.id is now 2!
```

**Sound Mode:** A type with a `readonly` property cannot be assigned to a type where that property is mutable, enforcing `readonly` guarantees across aliases.

*Limitation:* Because JavaScript is fundamentally mutable and `tsz` adheres to type erasure, this enforces correctness at the reference level but cannot prevent underlying mutations if a mutable reference to the same object is retained elsewhere.

#### TSZ3004: Split Accessor Assignability

**TypeScript allows:**
```typescript
interface Prop { value: string | number; }

class Box {
    get value(): string | number { return ""; }
    set value(v: string) { }  // Setter is narrower
}

const p: Prop = new Box();  // ✅ tsc allows
p.value = 42;               // 💥 Runtime: Box setter rejects number
```

**Sound Mode:** When assigning objects with split accessors to interfaces, verify that the setter accepts the interface's write type.

#### TSZ3005: Unsafe Object Mutation (`Object.assign`)

**TypeScript allows:**
```typescript
interface User { name: string; age: number; }
const user: User = { name: "Alice", age: 30 };

Object.assign(user, { age: "thirty" }); // ✅ tsc allows
user.age.toFixed(); // 💥 Runtime: user.age is a string!
```

**Sound Mode:** `Object.assign(target, ...sources)` must enforce that any overlapping properties in `sources` are deeply assignable to the corresponding properties in `target`.

#### TSZ3006: Sticky Freshness

> **Implementation: LIVE.** Sound mode skips `widen_freshness()` calls at 4 checker locations (`variable_checking/core.rs:844`, `state/state.rs:987`, `computation/call.rs:923`, `computation/identifier.rs:814`). Currently active under `sound: true` (not behind a separate pedantic flag, since pedantic layer doesn't exist yet). Freshness is preserved by keeping the `FRESH_LITERAL` object flag.

**Layer: Pedantic** (not part of core soundness)

Object literal freshness is preserved through variables to ensure excess property checks are not easily bypassed.

In a structural type system, "extra properties" are usually not a runtime crash; excess property checks are more of a typo-catcher heuristic. The existing example shows the heuristic being bypassed via indirection.

**Design decision:** Sticky Freshness belongs in the **pedantic** layer unless tied directly to `Exact<T>` semantics.

- **Option 1 (current):** Keep in pedantic layer. Available for teams who want stronger typo detection, but not part of the core soundness story.
- **Option 2 (future):** Tie freshness to `Exact<T>` — freshness matters when assigning to `Exact<T>`, not for all interface assignments. This aligns with the long-standing TypeScript exact types discussion.

#### TSZ3007: Missing Index Signature

Requires explicit definitions of index signatures where standard TypeScript infers them loosely from object structural assignments.

#### TSZ3008: Exact Optional Property Types

TypeScript has `exactOptionalPropertyTypes` — distinguishing "missing" vs "present but `undefined`" matters at runtime.

```typescript
interface Config {
  timeout?: number;
}

const c: Config = { timeout: undefined };
// Without exactOptionalPropertyTypes: ✅
// With it: ❌ 'undefined' is not assignable to 'number'

// This matters because:
"timeout" in c  // true (key exists)
// vs
const d: Config = {};
"timeout" in d  // false (key missing)
```

**Sound Mode:** Implies `exactOptionalPropertyTypes` behavior.

### Function & Callable Surfaces

#### TSZ4001: Rest Parameter Bivariance

**TypeScript allows:**
```typescript
type Logger = (...args: any[]) => void;
const fn = (id: number, name: string) => {};
const logger: Logger = fn;  // ✅ tsc
logger("wrong", "types");   // 💥 Runtime: fn expects number, string
```

**Sound Mode:** Reject — `(...args: any[])` is NOT a supertype of specific signatures.

#### TSZ4002: Void Return Exception

**TypeScript allows:**
```typescript
type Callback = () => void;
const cb: Callback = () => "hello";  // ✅ tsc allows returning string
```

**Sound Mode:** Require return types to actually match.

#### TSZ4003: Opaque Function Type

**TypeScript allows:**
```typescript
const f: Function = (x: number) => x * 2;
f("not", "a", "number", 42, true);  // ✅ tsc allows any args!
```

**Sound Mode:** Reject assignment of specific function types to `Function`, or require explicit cast.

### Collections & Indexing

#### TSZ5001: Unchecked Index Access

> **Implementation: Planned.** Sound mode does NOT currently auto-enable `noUncheckedIndexedAccess`. The index access evaluation in `evaluate.rs:1263` reads the `no_unchecked_indexed_access` parameter from compiler options, but `sound: true` does not force this to `true`. It must be set separately. Wiring this is a one-line change.

**TypeScript allows:**
```typescript
const arr: number[] = [];
const x = arr[100];       // ✅ tsc: x is number
x.toFixed();              // 💥 Runtime: undefined.toFixed()
```

**Sound Mode:** Type index access as `T | undefined` by default. This matches TypeScript's `noUncheckedIndexedAccess` semantics exactly — sound mode should imply this behavior even if the tsconfig flag isn't set.

#### TSZ5002: Strict Array/Set Membership

**TypeScript restricts:**
```typescript
const arr: string[] = ["a", "b"];
arr.includes(1); // ❌ tsc errors: Argument of type 'number' is not assignable to parameter of type 'string'.
```

**Sound Mode:** Modify core library definitions so that `includes` and `has` accept `unknown`. This allows perfectly safe membership checks without arbitrary subtyping restrictions.

#### TSZ5003: Non-Empty Array Reduction

**TypeScript allows:**
```typescript
const arr: number[] = [];
const sum = arr.reduce((a, b) => a + b); // ✅ tsc allows, 💥 Runtime: TypeError on empty array
```

**Sound Mode:** If `reduce` is called without an initial value, the array must be proven to be non-empty (e.g., `[T, ...T[]]`), or the return type evaluates to `T | undefined`.

#### TSZ5004: Tuple Length Mutation

**TypeScript allows:**
```typescript
const tuple: [number, string] = [1, "hello"];
tuple.push(2);      // ✅ tsc allows, tuple is now [1, "hello", 2]
const len = tuple.length; // ✅ tsc says '2', 💥 Runtime: actual length is 3
```

**Sound Mode:** Tuple types omit length-mutating methods entirely, or those methods are typed to require `never` as an argument.

### Primitives & Enums

#### TSZ6001: Strict Nominal Enums

> **Implementation: PARTIAL (dead code).** `SoundModeConfig.strict_enums` exists in `sound.rs` but is never used by `SoundLawyer.is_assignable()` or any other code path. Standard tsc bidirectional enum/number behavior runs in sound mode.

*Note: Modern TypeScript (5.0+) already fixes the classic `const s: Status = 999` bug. The `tsz` baseline matches this modern behavior.*

**TypeScript allows:**
```typescript
enum Status { Active = 0, Inactive = 1 }
const s: Status = Status.Active;
const n: number = s; // ✅ tsc allows implicit enum-to-number conversion
```

**Sound Mode:** Enforce strict nominal typing for enums. Implicit conversions between enums and numbers (in either direction) are forbidden without an explicit cast, treating the enum as a fully opaque type.

**Canonical escape patterns:**
```typescript
// Provided by sound core libraries
function parseEnum<E>(enumObj: E, value: number): E[keyof E] | undefined;
function assertEnum<E>(enumObj: E, value: number): asserts value is E[keyof E];

// Usage
const status = parseEnum(Status, userInput);
if (status !== undefined) {
  handleStatus(status);  // Safe!
}
```

**Library strategy:** SoundlyTyped can "close open numeric enums" in upstream `.d.ts` files, which is important because ecosystem enums are everywhere.

#### TSZ6002: Implicit Primitive Boxing

**TypeScript allows:**
```typescript
const o: Object = 42;     // ✅ tsc allows
const e: {} = "hello";    // ✅ tsc allows
```

**Sound Mode:** Reject primitive-to-`Object`/`{}` assignment.

### Generics & Type Algebra

#### TSZ7001: Conditional Type Distribution

**TypeScript does:**
```typescript
type IsString<T> = T extends string ? true : false;
type A = IsString<string | number>;
// Expected by many: false (the union is not a string)
// Actual: boolean (true | false) - distributes!
```

**Sound Mode (revised approach):** Keep TypeScript's distribution behavior as the default. Changing the default to opt-in would break a huge amount of real-world type-level code (utility types and libraries) unless we also ship rewritten "sound" versions of those utilities.

Instead:
- **Add a diagnostic** that flags surprising distributions
- **Suggest the non-distributive pattern**: `[T] extends [U] ? ...`
- **Consider a tsz-only helper type** that makes intent explicit

#### TSZ7002: Intersection Survival

**TypeScript allows:**
```typescript
type Broken = { [key: string]: number } & { name: string };
// Impossible type - name must be both number and string
const x: Broken = ???;  // Can't create, but type exists
```

**Sound Mode:** Reduce impossible intersections to `never` immediately.

**Caution:** Watch for cases where TS intentionally keeps an intersection "alive" as a modeling trick:
- Optional properties
- Index signatures
- Places where intersection is used for type-level computation

This should be a **separate TSZ code** with "why" + "how to rewrite" documentation, because type-level programmers will hit it first.

#### TSZ7003: Generic Constraint Confusion

**Status: Under review.** In modern TypeScript, `return t` where `T extends string` and return type is `U extends string` is already rejected with the "could be instantiated with a different subtype" error.

**Decision:**
- If modern tsc already rejects this, drop this planned item (tsz already matches tsc)
- If specific real unsoundness cases exist in current tsc behavior that tsz matches, re-scope to those specific cases

### Control Flow & Exhaustiveness

#### TSZ8001: Switch Statement Exhaustiveness

**TypeScript allows:**
```typescript
type Direction = "Up" | "Down";

function move(d: Direction) {
    switch (d) {
        case "Up": return 1;
        // ⚠️ tsc allows missing "Down" unless you use strict null checks + return types
    }
}
```

**Sound Mode:** Enforce native switch exhaustiveness. All cases must be handled explicitly (or via a `default` case).

---

## What Sound Mode Does NOT Change

Some things that might seem like unsoundness are actually deliberate precision trade-offs:

### Literal Widening for `let`

```typescript
let x = "hello";  // x: string (not "hello")
```

This is **not unsound** — it's conservative. The type is wider than necessary but still correct.

### Split Accessor Types

Getters and setters with different types are **fine** for direct usage:

```typescript
class Box {
    get value(): string | number { return ""; }
    set value(v: string) { }
}

const x = box.value;  // x: string | number ✅
box.value = x;        // ❌ tsc correctly errors!
```

TypeScript handles this correctly. The issue is only in *assignability* (see TSZ3004).

### Control Flow Analysis

TypeScript already does excellent CFA for narrowing. Sound Mode wouldn't change this — it would focus on the structural type system rules.

---

## SoundlyTyped: Auditability & Trust

> **Implementation: Planned.** No SoundlyTyped infrastructure exists in the codebase. No `*.sound.lib.d.ts` files, no `.tsz/soundlytyped/` cache directory, no lockfile, no `tsz soundlytyped diff` command.

The SoundlyTyped idea is strong: rewrite upstream `.d.ts` mechanically to remove common unsoundness. The product risk is trust ("what did you change?") and reproducibility.

### Deterministic Cache

```
.tsz/soundlytyped/
  <pkg>@<version>/
    index.d.ts          # Transformed output
    ...
```

### Lockfile

```jsonc
// .tsz/soundlytyped.lock
{
  "packages": {
    "react@18.3.1": {
      "upstream_hash": "sha256:abc123...",
      "transform_version": "1.2.0",
      "output_hash": "sha256:def456...",
      "transforms_applied": [
        "any_to_unknown_boundaries",
        "method_to_property_variance",
        "enum_closing"
      ]
    }
  }
}
```

### Diff Command

```bash
# Human-readable summary of what SoundlyTyped changed
tsz soundlytyped diff react

# Output:
# react@18.3.1 — 3 transforms applied
# ────────────────────────────
# any → unknown at boundaries:    47 locations
#   - createElement return type
#   - useRef initial value
#   - ...
# Method variance rewrites:       12 locations
#   - EventHandler.handleEvent
#   - ...
# Enum closing:                    2 locations
#   - FormEncType
#   - ...
```

If people can audit it quickly, they'll actually enable it.

---

## Detailed Rollout Plan

This is the execution plan that best matches the "any-less user code, tolerant external declarations" model.

### Validation Result

This plan has been checked against the current codebase rather than written as a pure wish list.

1. `cargo run --quiet --bin audit-unsoundness -- --summary` currently reports **44/44 TypeScript unsoundness catalog rules implemented** somewhere in the compat/subtype engine.
2. That does **not** mean Sound Mode is finished. It means the remaining work is mostly about **policy exposure, file-scope gating, diagnostics, cache correctness, and declaration-boundary ergonomics**.
3. The hard problem is no longer "can the solver represent this?".
4. The remaining risk lives in making stricter behavior apply only to sound-scoped user code.
5. The remaining risk lives in keeping relation caches correct when policy changes.
6. The remaining risk lives in making external declaration boundaries usable without demanding ecosystem cleanup by hand.

### Feasibility Snapshot

1. **High feasibility: explicit `any` ban in user-authored TS source**
   Evidence: `crates/tsz-checker/src/types/type_node.rs`, `crates/tsz-checker/src/flow/control_flow/assignment_fallback.rs`, `crates/tsz-checker/src/context/compiler_options.rs`, `crates/tsz-core/src/diagnostics.rs`.
   Why: the checker already recognizes `AnyKeyword`, can already tell whether the current file is a declaration file, and already has `tsz-sound` diagnostics infrastructure.
   Main caveat: the implementation must gate on **user-authored non-declaration TypeScript source**, not accidentally flag `.d.ts`, generated libs, JavaScript/JSDoc surfaces, or unrelated fallback resolution paths.

2. **High feasibility: make sound mode imply existing TypeScript safety flags**
   Evidence: `useUnknownInCatchVariables`, `exactOptionalPropertyTypes`, and `noUncheckedIndexedAccess` already exist in `crates/tsz-core/src/config.rs`, `crates/tsz-cli/src/driver/core.rs`, `crates/tsz-solver/src/evaluation/evaluate.rs`, and `crates/tsz-solver/src/relations/subtype/rules/objects.rs`.
   Why: this is largely existing plumbing, not new semantics.
   Main caveat: decide whether `sound: true` hard-forces these flags or sets them as sound defaults that can later be overridden by more granular config.

3. **High feasibility: tsconfig parsing for `sound`**
   Evidence: CLI `--sound` already exists in `crates/tsz-cli/src/args.rs` and `crates/tsz-cli/src/driver/core.rs`, but `CompilerOptions` in `crates/tsz-core/src/config.rs` does not parse `sound`.
   Why: this is straightforward config plumbing.
   Main caveat: the product question is not technical feasibility; it is whether we want to expose tsconfig support before semantics settle.

4. **Medium feasibility: dedicated TSZ sound diagnostics**
   Evidence: `DiagnosticDomain::Sound` plus `DiagnosticBag::sound_error` already exist in `crates/tsz-core/src/diagnostics.rs`; temporary `SoundDiagnosticCode` definitions exist in `crates/tsz-solver/src/sound.rs`.
   Why: the diagnostic substrate exists.
   Main caveat: assignability failures are still emitted through standard TypeScript-style diagnostic paths in the checker, so surfacing TSZ codes means touching real checker call sites, not just flipping a global switch.

5. **Medium-to-high feasibility: true sound `any` semantics**
   Evidence: `SoundLawyer::is_assignable()` already models the stricter `any` rule in `crates/tsz-solver/src/sound.rs`; `RelationPolicy`, `AnyPropagationMode`, and `RelationCacheKey.any_mode` already exist in `crates/tsz-solver/src/relations/relation_queries.rs`, `crates/tsz-solver/src/relations/subtype/helpers.rs`, and `crates/tsz-solver/src/types.rs`.
   Why: the system already has most of the knobs needed.
   Main caveat: `RelationPolicy::from_flags()` still infers "strict any" from `FLAG_STRICT_FUNCTION_TYPES`, which is the wrong abstraction boundary for long-term sound mode correctness.

6. **Low feasibility as a "small patch": declaration-origin `any` quarantine**
   Evidence: there is currently no general declaration-origin provenance threading in the solver, and no transformed declaration overlay pipeline in-tree.
   Why: the plan's "external `.d.ts` may contain `any`, but user code sees `unknown`" story is correct as a product direction.
   Main caveat: it should be treated as a separate architecture track, not as a quick follow-up to the explicit-`any` ban.

7. **High strategic value but high implementation cost: `SoundlyTyped` overlays**
   Evidence: there is no existing package transform/cache/lockfile pipeline for rewritten declaration overlays.
   Why: this is the cleanest long-term adoption strategy.
   Main caveat: it is greenfield work and should only be promised after we validate the smaller boundary strategy first.

8. **Medium-to-high feasibility: `@tsz-unsound` suppressions**
   Evidence: `crates/tsz-cli/src/driver/check_utils.rs` already scans comments for `@ts-expect-error` / `@ts-ignore`, applies line-targeted suppression, and emits stale-directive diagnostics.
   Why: the repo already has the shape of the feature.
   Main caveat: the current implementation is **line-targeted**, while `@tsz-unsound` wants **code-aware** suppression with a required reason, so this is an adaptation of existing machinery rather than a free toggle.

9. **Medium-to-high feasibility: sound core libraries**
   Evidence: lib loading and replacement already exist in `crates/tsz-core/src/config.rs` and `crates/tsz-cli/src/driver/core.rs`, including `libReplacement` and compiler-lib path resolution.
   Why: a first version can likely reuse existing lib-selection paths instead of inventing a parallel loader.
   Main caveat: the first step should probably be a small pilot set of replacement libs, not an immediate full alternate standard library surface, and the current replacement path is geared toward `@typescript/lib-*` package layouts.

10. **Medium feasibility: `reportOnly` migrations**
    Evidence: diagnostics already flow through one reporter path and one exit-code decision path in `crates/tsz-cli/src/reporter.rs` and `crates/tsz-cli/src/bin/tsz.rs`.
    Why: the feature is operationally straightforward once sound diagnostics exist.
    Main caveat: `reportOnly` is not merely "show errors but return 0" — it needs a deliberate policy for severity, summary lines, emit gating, and exit codes, and the CLI layer may need to preserve sound-domain metadata more explicitly. The good news is that the current reporter summary already keys off `DiagnosticCategory::Error`, which makes a reporting-boundary downgrade strategy mechanically plausible.

11. **Medium feasibility, but intentionally deferred: JS/JSDoc participation**
    Evidence: `crates/tsz-checker/src/context/compiler_options.rs` already distinguishes `.js` files, `checkJs`, `@ts-check`, and JSDoc resolution.
    Why: the codebase already has separate JS/JSDoc mode plumbing, so this is not impossible.
    Main caveat: it is a product-scope decision, not a free add-on. JSDoc `@type {any}` and TS `: any` may both flow through `AnyKeyword`, but they live in different adoption worlds. Treating them identically in the first release would create noise and muddy the message.

### Recommended Sequencing

The earlier version of this plan was directionally right, but the validated sequencing should be:

1. Start with **existing safety behavior that already has plumbing**.
2. Then ban **explicit `any` in user source**.
3. Then fix **sound `any` semantics and cache policy**.
4. Then emit **dedicated sound diagnostics**.
5. Then improve **developer experience surfaces** (config/editor/tooling).
6. Then ship a **small sound core-lib pilot**.
7. Then tackle **ecosystem boundaries in two steps**: boundary-strategy validation first, package overlays second.

Most importantly:

1. Do **not** promise full declaration-boundary quarantine as a near-term patch.
2. Do **not** make `SoundlyTyped` a prerequisite for the first useful version of Sound Mode.
3. Do **not** let internal `TS9001`-style placeholder enums block the public TSZ family taxonomy.

### First Shippable Slice

The most credible first dogfood release is intentionally smaller than the full vision:

1. `sound: true` implies the existing safety behaviors that already exist in the codebase (`useUnknownInCatchVariables`, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`)
2. Explicit `any` is banned in user-authored `.ts` / `.tsx` / `.mts` / `.cts` files
3. `.d.ts` files stay exempt by default
4. JS/JSDoc remains out of scope for the first slice, even when `allowJs` / `checkJs` is enabled
5. Sound mode becomes available in tsconfig and server/LSP paths once the checker semantics above are stable
6. External declaration quarantine, sound core libs, and SoundlyTyped overlays remain later phases

Why this slice is right:

1. It is already materially useful for application teams
2. It avoids turning npm and JSDoc cleanup into a prerequisite
3. It gives the team real adoption feedback before we commit to harder boundary architecture
4. It keeps the public story honest: "any-less TS user code first" is a clearer promise than "entire ecosystem soundness soon"

### Decision Gates

These are the remaining product choices worth locking deliberately before implementation starts:

1. **Sound-implied safety flags**
   Recommendation: in the first release, treat `useUnknownInCatchVariables`, `exactOptionalPropertyTypes`, and `noUncheckedIndexedAccess` as effective **hard-on semantics** of `sound: true`, not user-overridable defaults. This keeps the product contract crisp while the feature is still settling.

2. **JS/JSDoc scope**
   Recommendation: keep JS/JSDoc out of the first release. Revisit only after TS-source sound mode is working and adoption feedback is real.

3. **`reportOnly` behavior**
   Recommendation: downgrade sound diagnostics at the final reporting boundary rather than returning success with "Found N errors". This matches the current reporter architecture better and gives a cleaner user experience.

4. **`@tsz-unsound` availability**
   Recommendation: allow it only in user-authored non-declaration source at first. Do not design the first version around editing vendor `.d.ts` files.

5. **Sound core-lib packaging**
   Recommendation: start by reusing the existing lib-replacement path, even if it needs one small selector extension, instead of designing a brand-new package/discovery system up front.

6. **Public config shape**
   Recommendation: support a flat `sound*` family with `sound` as the master switch and sibling flags such as `soundReportOnly`, `soundPedantic`, and `soundArrayVariance`. Do not ship a competing nested `sound: { ... }` object form.

### Phase 0: Lock the Product Semantics

Goal: make the contract unambiguous before adding more checks.

Deliverables:

1. Document that Sound Mode's primary scope is user-authored non-declaration source files
2. Document that declaration files are trust boundaries by default
3. Decide the exact tsconfig surface: whether `sound: true` expands to object defaults, and which knobs are public vs internal
4. Decide whether TSZ1001 remains one umbrella code or splits later
5. Decide explicitly that the first shipped scope is TS source, not JS/JSDoc

Exit criteria:

1. Docs, website copy, and CLI help all describe the same scope model
2. There is a single source of truth for defaults
3. The docs do not imply JS/JSDoc coverage that the first implementation will not enforce

### Phase 1: Align Existing Safety Flags Under Sound Mode

Goal: deliver immediate runtime-safety wins using machinery that already exists in the codebase.

Implementation sketch:

1. Make `sound_mode` imply the effective behavior of `useUnknownInCatchVariables`
2. Make `sound_mode` imply the effective behavior of `exactOptionalPropertyTypes`
3. Make `sound_mode` imply the effective behavior of `noUncheckedIndexedAccess`
4. Decide whether tsconfig support for `sound` ships in this phase or remains CLI-only during exploration
5. Keep the behavior scoped to TypeScript source semantics; do not accidentally expand JS/JSDoc enforcement in the same patch

Primary touchpoints:

1. `crates/tsz-core/src/config.rs`
2. `crates/tsz-cli/src/driver/core.rs`
3. `crates/tsz-cli/src/driver/check.rs`
4. `crates/tsz-solver/src/evaluation/evaluate.rs`
5. `crates/tsz-solver/src/relations/subtype/rules/objects.rs`

Tests:

1. Catch variables are treated as `unknown` under sound mode
2. Optional property assignability follows exact-optional semantics under sound mode
3. Indexed access returns `T | undefined` under sound mode
4. Existing non-sound behavior remains unchanged
5. JS/checkJs behavior does not regress unintentionally

Exit criteria:

1. Sound mode immediately tightens three real runtime-safety surfaces with minimal new architecture
2. The docs stop over-promising on checks that are actually just dormant flag wiring

### Phase 2: Ban Explicit `any` in User Code

Goal: deliver the first tangible user-visible behavior with minimal ecosystem pain.

Implementation sketch:

1. In type-node resolution, detect `AnyKeyword` in user-authored non-declaration files
2. Emit a sound diagnostic instead of silently producing ordinary `TypeId::ANY` without explanation
3. Keep declaration files exempt in the default mode
4. Preserve `noImplicitAny` behavior separately; this phase is about **explicit** `any`, not implicit `any`
5. Cover both the primary type-node path and the checker fallback path so behavior is consistent
6. Scope the first version to `.ts` / `.tsx` / `.mts` / `.cts`; do not fold JS/JSDoc `@type {any}` into the same rule yet

Primary touchpoints:

1. `crates/tsz-checker/src/types/type_node.rs`
2. `crates/tsz-checker/src/flow/control_flow/assignment_fallback.rs`
3. `crates/tsz-checker/src/context/compiler_options.rs`
4. Sound diagnostic plumbing in `tsz-core`

Tests:

1. `.ts` / `.tsx` explicit `any` errors in sound mode
2. `.d.ts` explicit `any` allowed by default
3. Same source passes when sound mode is off
4. Existing `noImplicitAny` tests still behave independently
5. The fallback type-node path does not silently reintroduce `TypeId::ANY`
6. `.js` with `checkJs` and JSDoc types does not start failing under the Phase 2 rule unless we explicitly opt into that later

Exit criteria:

1. A team can enable sound mode and immediately stop writing new `any` in source files
2. No flood of `.d.ts` errors
3. The rollout message remains simple: "sound mode makes TS source any-less first"

### Phase 3: Remove `any` Bottom-Type Behavior from Sound User Code

Goal: make the runtime safety story match the docs, not just nested-structure checks.

Implementation sketch:

1. Replace the current `TopLevelOnly` behavior with a true sound-mode `any` policy for user code
2. Either wire `SoundLawyer` into the assignability path or port its semantics into the unified relation policy
3. Ensure top-level `any -> T` no longer succeeds in sound user code except for `T = any | unknown`
4. Keep an explicit legacy path for non-sound mode
5. Remove or make explicit the accidental coupling between `strict_any_propagation` and `FLAG_STRICT_FUNCTION_TYPES`

Important constraint:

1. The cache key must fully capture the policy, including sound-vs-non-sound relation behavior
2. `RelationPolicy::from_flags()` must not keep inferring sound-only `any` behavior from unrelated function-variance flags
3. Query-cache fast paths that construct `RelationCacheKey(..., any_mode = 0)` must not be left behind for sound-sensitive checks

Tests:

1. `const x: any = ...; const y: number = x;` fails in sound user code
2. Function returns and top-level assignments behave consistently with nested cases
3. Cache correctness tests cover sound/non-sound toggles
4. Identity/redeclaration checks still behave correctly when strict `any` is active

Exit criteria:

1. TSZ1001 is no longer "nested only"
2. The checker and docs agree on the core `any` semantics

### Phase 4: Add Sound Diagnostics as a First-Class Surface

Goal: make sound mode understandable and adoptable at scale.

Implementation sketch:

1. Route sound errors through `DiagnosticDomain::Sound`
2. Emit family-based TSZNNNN codes in the checker rather than standard TS2322/TS2345 for sound-specific failures
3. Add rich messages for explicit `any`, boundary-quarantined `any`, and unsafe operations on tainted values
4. Add `@tsz-unsound` parsing and stale-suppression enforcement later in the phase
5. Treat the current `TS9001`-`TS9005` enum in `sound.rs` as temporary implementation detail, not public naming authority
6. Add `tsz explain TSZNNNN` and `--sound-summary` once the code surface is stable enough to be useful
7. Reuse the existing directive-suppression pipeline shape rather than building a second comment scanner from scratch
8. Extend the current line-targeted suppression matcher into a code-aware matcher rather than assuming that the existing `@ts-expect-error` logic is sufficient as-is

Tests:

1. Diagnostics use `tsz-sound` source and `TSZ` prefix
2. Suppressions only apply to matching codes
3. Stale suppressions become errors
4. CLI summary groups sound diagnostics by code family correctly
5. `tsz explain` resolves known sound codes to stable help text

Exit criteria:

1. Teams can triage sound debt by code, not just by generic assignment errors

### Phase 5: Expose Configuration and Editor Parity

Goal: make the feature practical to dogfood once the core semantics are real.

Implementation sketch:

1. Parse `sound` from tsconfig into `CheckerOptions`
2. Decide whether LSP/server exposure ships immediately or behind an experimental gate
3. Keep CLI, tsconfig, and server defaults aligned
4. Update hidden CLI/help text and config docs so they match the new TSZ family taxonomy
5. Add optional `reportOnly` mode for migrations once sound diagnostics exist
6. Choose one explicit `reportOnly` policy before coding:
   keep sound diagnostics as errors but exempt them from failure gates, or
   downgrade them to warnings at the final reporting boundary
7. Preserve enough sound-domain identity through the driver/reporter boundary to implement the chosen policy cleanly
8. Make sure summary output, watch mode, and emit gating all follow the same policy rather than drifting independently

Recommended direction:

1. Prefer **downgrading report-only sound diagnostics at the final reporting boundary**.
2. Reason: users should not see "Found N errors" with a successful exit code.
3. `--sound-summary` can still present report-only sound debt as a dedicated tracked surface.
4. If carrying full sound-domain metadata through the CLI boundary is awkward, a temporary fallback is to key off the TSZ code family range intentionally and explicitly.

Primary touchpoints:

1. `crates/tsz-core/src/config.rs`
2. `crates/tsz-core/src/lib.rs`
3. `crates/tsz-cli/src/bin/tsz_server/check.rs`
4. `crates/tsz-cli/src/args.rs`
5. `crates/tsz-cli/src/bin/tsz.rs`
6. `crates/tsz-cli/src/reporter.rs`

Tests:

1. `sound` round-trips through tsconfig parsing
2. Server-side checker options do not silently drop sound mode
3. Non-sound projects remain unaffected
4. `reportOnly` preserves diagnostic visibility without failing the run

Exit criteria:

1. Sound mode is no longer effectively "CLI-only"
2. Users can evaluate the feature in editors without custom wrappers

### Phase 6: Ship Sound Core Libs

Goal: stop standard library APIs from injecting `any` into otherwise sound code.

Initial targets:

1. `JSON.parse(): unknown`
2. `Response.json(): Promise<unknown>`
3. DOM query surfaces that currently yield `any`
4. Other obvious trust-boundary APIs in core libs

Implementation sketch:

1. Start with a small pilot set of replacement declarations rather than an all-at-once alternate standard library
2. Reuse the existing compiler lib resolution / lib replacement path where possible
3. Keep ordinary libs untouched for non-sound mode
4. Make this a source-selection problem, not ad hoc checker special-casing
5. Decide whether the pilot rides on the existing `@typescript/lib-*` replacement convention or whether sound mode needs one small selector extension for alternate lib packages

Suggested pilot targets:

1. `JSON.parse(): unknown`
2. `Response.json(): Promise<unknown>`
3. one DOM query surface that is clearly boundary-like and high impact

Reason for the pilot shape:

1. It validates that the loader and replacement strategy feel right before we commit to a full `*.sound.lib.d.ts` universe.
2. It lets us measure adoption value without expanding the surface area too early.

Exit criteria:

1. Common trust-boundary APIs stop producing `any` before user code even sees them

### Phase 7: Validate a Boundary Strategy Before Generalizing It

Goal: prove the declaration-boundary story on a small set of high-value surfaces before building a general overlay platform.

Implementation sketch:

1. Pilot boundary rewrites on a narrow set of declarations where the payoff is obvious
2. Start with core-library and one-or-two-package patterns instead of promising all of npm
3. Decide whether source transformation alone is sufficient or whether some declaration-origin provenance must be tracked in the type system
4. Document the exact set of boundary positions we quarantine (`return`, readable properties, callback params into user code) before scaling the mechanism

Exit criteria:

1. We have at least one concrete, working quarantine strategy beyond "just rewrite all `any`"
2. The team can make an informed decision on whether `SoundlyTyped` should stay a transform-only architecture

### Phase 8: Build SoundlyTyped Overlays and Advanced Surfaces

Goal: make third-party `.d.ts` files mostly work out of the box once the smaller boundary strategy has proven itself.

Implementation sketch:

1. Add a deterministic transform pipeline for external declarations
2. Rewrite readable-boundary `any` to `unknown`
3. Rewrite method bivariance patterns where mechanically safe
4. Cache transformed outputs by package/version/hash
5. Add human-auditable diff output and lockfile metadata

Design rule:

1. Prefer declaration rewriting over pervasive solver provenance tracking
2. Only add provenance metadata in the type system if the transformation approach leaves important gaps

Exit criteria:

1. Popular dependency stacks become usable without local declaration forks

### Phase 9: Tighten Optional Surfaces

Goal: let advanced teams go further once the default mode is stable.

Candidate follow-ons:

1. `checkDeclarations: true`
2. stronger `any` operation bans
3. soundness checks for unsafe assertions
4. stricter declaration overlay validation
5. first-party package publishing workflows with sound declaration emit

Exit criteria:

1. Advanced strictness is additive and does not block mainstream adoption

### Verification Plan

Every phase should ship with:

1. unit tests for checker and solver behavior
2. integration tests covering `.ts` vs `.d.ts` boundaries
3. fixtures with representative third-party declaration patterns
4. website/doc updates
5. conformance snapshots to prove non-sound mode parity is unaffected
6. `cargo run --quiet --bin audit-unsoundness -- --summary` recorded as a regression signal for compat behavior

### Explicit Non-Goals for the First Useful Version

1. Full npm ecosystem purification is **not** required before Sound Mode is useful
2. General declaration-origin provenance tracking in the solver is **not** required for the first shipping slice
3. Renaming every internal `TS900x` placeholder in one shot is **not** required before the public TSZ family taxonomy is stable

---

## Implementation Considerations

### The Caching "Correctness Tax"

Caching is the hidden complexity of gradual soundness. Because Sound Mode is a policy switch over the same Judge solver, any feature that makes relation outcomes depend on sound policy (for example, `soundArrayVariance` or `any` behavior within the active sound profile) must be reflected in the cache key.
- **Policy Bitsets:** The solver's relation cache keys must include a versioned policy ID or bitset. This ensures that a `Dog[]` to `Animal[]` relation check caches as `false` in Sound Mode, but doesn't pollute the cache for a non-sound file that expects `true`.

> **Current state:** `RelationCacheKey` in `types.rs:259` has `flags: u16` (4 compiler flags) and `any_mode: u8`. The `any_mode` field differentiates sound vs non-sound `any` propagation behavior. However, `strict_subtype_checking` (which affects `CompatChecker` behavior like method bivariance) is NOT reflected in the cache key. This is a correctness gap: sound and non-sound compat results could theoretically collide in the cache. A `FLAG_SOUND_MODE` bit should be added.

### Compatibility & Performance

Sound Mode rejects some valid TypeScript code. Migration requires running in "report only" mode and manually fixing code structures. Most sound checks are `O(1)` additions to existing checks. The main cost is not raw relation complexity; it is **policy plumbing, caching correctness, and boundary transformation infrastructure**.

Performance strategy:

1. Keep the base mode cheap by banning explicit `any` in user code before introducing heavier overlay machinery
2. Solve ecosystem adoption with precomputed declaration rewrites rather than per-query provenance checks where possible
3. Cache overlays aggressively and make them deterministic so repeated builds stay fast

---

## Trade-offs Summary

| Aspect | TypeScript (tsc) | Sound Mode (Core) | Pedantic |
|--------|------------------|-------------------|----------|
| Array variance | Covariant (unsafe) | Invariant for mutable | — |
| Method params | Bivariant | Contravariant | — |
| `this` in params | Covariant | Contravariant | — |
| `any` type | Top + Bottom | Not allowed in user source; external `any` quarantined | — |
| Type assertions | Widening + narrowing | Widening only | — |
| `Function` type | Accepts all callables | Requires explicit cast | — |
| Index access | `T` | `T \| undefined` | — |
| Numeric enums | Open to any number | Closed to defined values | — |
| Void returns | Any return OK | Strict matching | — |
| Weak types | Warning | Error | — |
| Non-null assertion | Trusted | Flagged as escape | — |
| Definite assignment | Trusted | Flagged as escape | — |
| Catch variables | Implicit `any` | Strict `unknown` | — |
| Optional properties | Missing ≈ undefined | Exact distinction | — |
| Conditional distribution | Automatic | Automatic + diagnostic | — |
| Generic identity | Constraint-based | Re-scoped (see notes) | — |
| Primitive boxing | `number` → `Object` OK | Explicit wrapper required | — |
| Split accessors | Assignment allowed | Check setter accepts write type | — |
| Switch Exh. | Checked via return type | Checked implicitly | — |
| Array.includes | Subtype match required | `unknown` type allowed | — |
| Array.reduce | Permissive | `NonEmptyArray` or `T \| undefined` | — |
| Readonly Props | Assignable to mutable | Rejected (strict aliasing) | — |
| Tuples | Inherit Array mutations | Mutation methods typed `never` | — |
| Error handling | Implicit `any` | Strict `unknown` | — |
| Object.assign | Intersects `T & U` | Enforces structural compatibility | — |
| Excess properties (indirect) | Accepted on variables | — | Rejected (sticky freshness) |
| Exact types | Not available | `Exact<T>` opt-in | Default for all |

**The fundamental trade-off:** Sound Mode catches more bugs but requires more explicit boundaries. The crucial adoption choice is to spend that explicitness budget in **user code where it buys safety**, while using declaration overlays to avoid making the ecosystem somebody's manual cleanup project.

---

## References

- [TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md) - Complete list of TypeScript's intentional unsoundness
- [NORTH_STAR.md](../architecture/NORTH_STAR.md) - Judge/Lawyer architecture documentation
- [TypeScript Design Goals](https://github.com/Microsoft/TypeScript/wiki/TypeScript-Design-Goals) - Why TypeScript chose pragmatism
- [TypeScript Soundness Documentation](https://www.typescriptlang.org/docs/handbook/type-compatibility.html#a-note-on-soundness) - TS's own soundness discussion
