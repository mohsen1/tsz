# Sound Mode

**Status**: Partially Implemented
**Last Updated**: March 2026

## Quick Start

```bash
# Enable sound mode via CLI
tsz check --sound src/

# Or in tsconfig.json
{
  "compilerOptions": {
    "sound": true
  }
}

# Per-file pragma (planned — not yet parsed from source)
// @tsz-sound
```

## Implementation Status

Sound mode is activated via a single `--sound` CLI flag or `sound: true` in tsconfig. The current implementation works by tightening the existing Lawyer (`CompatChecker`) with `strict_subtype_checking` and `strict_any_propagation` flags on `RelationPolicy`. Errors are emitted as standard TS diagnostic codes (TS2322, TS2345, etc.), not dedicated TSZ9xxx codes yet.

**Note on diagnostic codes:** The codebase defines `SoundDiagnosticCode` with codes TS9001–TS9005 in `crates/tsz-solver/src/sound.rs`. This doc uses the target TSZ9xxx numbering scheme which has not yet been applied to the implementation. A `DiagnosticDomain::Sound` infrastructure exists but is not yet used by the checker.

| Feature | Target Code | Impl Code | Status | Mechanism |
|---------|------------|-----------|--------|-----------|
| Method bivariance | TSZ9102 | TS9003 | LIVE | `strict_subtype_checking` disables method bivariance |
| `any` restriction | TSZ9201 | TS9004 | LIVE | `strict_any_propagation` → `TopLevelOnly` mode |
| Sticky freshness | TSZ9306 | TS9001 | LIVE | Skips `widen_freshness()` in 4 checker locations |
| Mutable array covariance | TSZ9101 | TS9002 | Dead code | `SoundLawyer.check_array_covariance()` exists but not wired into pipeline |
| Enum-number assignment | TSZ9601 | TS9005 | Dead code | `SoundModeConfig.strict_enums` defined but unused |
| Unsafe type assertion | TSZ9202 | — | Planned | No implementation |
| Unchecked indexed access | TSZ9501 | — | Planned | Not auto-enabled by `sound: true`; uses standard `noUncheckedIndexedAccess` flag separately |
| Missing index signature | TSZ9307 | — | Planned | Currently uses standard TS2329 |

**Known implementation gaps:**
- `SoundLawyer` struct in `sound.rs` is fully defined but never called from the checker pipeline (dead code)
- `SoundModeConfig` struct with granular flags is defined but never consumed
- `strict_subtype_checking` is not reflected in the `RelationCacheKey` — potential cache correctness issue
- The `any_mode: u8` field in `RelationCacheKey` partially differentiates sound vs non-sound cache entries

See `crates/tsz-solver/src/sound.rs` for sound mode definitions and `crates/tsz-checker/src/query_boundaries/assignability.rs` for the live integration.

---

## The Sound Mode Contract

TypeScript explicitly lists "apply a sound or provably correct type system" as a **non-goal**. tsz Sound Mode needs to be crystal-clear about what it guarantees, what it deliberately refuses to assume, and how users cross trust boundaries safely.

### What "Sound" Means in tsz

1. **Within Sound Mode–checked code**, tsz rejects the known TypeScript unsound patterns that can cause type-driven runtime exceptions — calling methods that aren't there, indexing missing keys, writing through an alias with the wrong element type.

2. **Trust boundaries are explicit**: data from the outside world is `unknown` unless validated (e.g., `JSON.parse`, `fetch().json()`, DOM queries).

3. **Any use of unsoundness must be explicit and auditable** (`// @tsz-unsound TSZ9xxx: reason`) and should be trackable in CI as technical debt.

This framing matches the reality that TypeScript won't add runtime checks; Sound Mode forces users to do validation where it matters.

### What Sound Mode Does NOT Guarantee

- Runtime immutability (JavaScript is fundamentally mutable; type erasure applies)
- Protection against code outside Sound Mode scope (non-sound files, native modules)
- That all possible runtime errors are caught (only *type-driven* ones)

---

## Two-Layer Configuration

> **Status: Planned design.** Currently sound mode is a single `bool` (`CheckerOptions.sound_mode`). The two-layer config, `sound: { pedantic: true }` object shape, and granular per-check flags are not yet implemented. A `SoundModeConfig` struct with individual flags exists in `sound.rs` but is dead code — not wired into the pipeline.

Sound Mode is a **dial**, not a single switch. The configuration separates core runtime-safety checks from bug-finding heuristics:

### Layer A: `sound: true` (Core Runtime Safety)

These checks address patterns where the doc can truthfully say: **"tsc allows it; this can crash."** Every check in this layer corresponds to a known TypeScript unsoundness that produces type-driven runtime exceptions.

| Diagnostic | Description |
|-----------|-------------|
| TSZ9101 | Covariant mutable arrays |
| TSZ9102 | Method parameter bivariance |
| TSZ9103 | Covariant `this` type |
| TSZ9201 | `any` escape / taint detection |
| TSZ9202 | Unsafe type assertions |
| TSZ9203 | Implicit `any` in error handling |
| TSZ9301 | Weak type acceptance |
| TSZ9303 | Readonly property aliasing |
| TSZ9304 | Split accessor assignability |
| TSZ9305 | Unsafe `Object.assign` |
| TSZ9307 | Missing index signature |
| TSZ9401 | Rest parameter bivariance |
| TSZ9402 | Void return exception |
| TSZ9403 | Opaque `Function` type |
| TSZ9501 | Unchecked indexed access |
| TSZ9502 | Strict array/set membership |
| TSZ9503 | Non-empty array reduction |
| TSZ9504 | Tuple length mutation |
| TSZ9601 | Enum-number assignment |
| TSZ9602 | Implicit primitive boxing |
| TSZ9801 | Switch statement exhaustiveness |
| TSZ9901 | Non-null assertion escape |
| TSZ9902 | Definite assignment assertion |
| TSZ9903 | Catch variable defaults to `unknown` |
| TSZ9904 | Exact optional property types |

### Layer B: `sound: { pedantic: true }` (Bug-Finding Heuristics)

These checks are useful bug detectors but are not strictly about soundness in a structural type system. Extra properties don't cause runtime crashes — they're more of a typo-catching heuristic.

| Diagnostic | Description |
|-----------|-------------|
| TSZ9306 | Sticky freshness (unless tied to `Exact<T>`) |
| TSZ9302 | Exact types & object iteration (when not using `Exact<T>` semantics) |

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
    "sound": { "pedantic": true }
  }
}

// Granular control
{
  "compilerOptions": {
    "sound": true,
    "soundArrayVariance": true,
    "soundMethodVariance": true,
    "soundThisVariance": true,
    "soundAnyType": true,
    "soundFunctionType": true,
    "soundIndexAccess": true,
    "soundEnums": true,
    "soundGenerics": true,
    "soundBoxing": true,
    "soundExactTypes": true,
    "soundSwitchExhaustive": true,
    "soundMembershipChecks": true,
    "soundArrayReduce": true,
    "soundReadonlyAliasing": true,
    "soundTuples": true,
    "soundCatchVariables": true,
    "soundObjectAssign": true,
    "soundNonNullAssertions": true,
    "soundDefiniteAssignment": true,
    "soundExactOptionalProperties": true
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

### TSZ9xxx Diagnostic Requirements

Every TSZ9xxx diagnostic must include:

1. **"Why this is unsafe"** — a one-paragraph explanation with a minimal crash example
2. **"How to fix"** — 2–3 common refactoring patterns
3. **"How to suppress"** — with required reason string format

### CLI Explain Command

```bash
# Explain any sound mode diagnostic
tsz explain TSZ9102

# Output:
# TSZ9102: Method Parameter Bivariance
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
#   // @tsz-unsound TSZ9102: validated by runtime guard in handler.ts
```

### Sound Summary Mode

```bash
# Overview of sound mode diagnostics in a project
tsz check --sound --sound-summary src/

# Output:
# Sound Mode Summary
# ──────────────────
# TSZ9101  12 errors  (3 suppressed)   src/models/ (8), src/utils/ (4)
# TSZ9201   7 errors  (1 suppressed)   src/api/ (5), src/lib/ (2)
# TSZ9501   4 errors  (0 suppressed)   src/handlers/ (4)
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
// @tsz-unsound TSZ9202: validated by legacy runtime guard in foo.ts

// ❌ Invalid: no code specified
// @tsz-unsound: trust me

// ❌ Invalid: no reason
// @tsz-unsound TSZ9202

// ❌ Invalid: suppression without matching error (becomes an error itself)
// @tsz-unsound TSZ9101: array is readonly  ← no TSZ9101 error here
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

### Group 9100: Variance & Subtyping

#### TSZ9101: Covariant Mutable Arrays

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

#### TSZ9102: Method Parameter Bivariance

> **Implementation: LIVE.** `strict_subtype_checking` → `disable_method_bivariance = true` in `SubtypeChecker`. Activated via `RelationPolicy` in `assignability.rs:103`. Errors emit as standard TS codes (TS2322/TS2345), not TSZ9102. The `@tsz-bivariant` annotation mechanism is planned but not implemented.

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

#### TSZ9103: Covariant `this` Type

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

### Group 9200: The `any` Epidemic & Trust Boundaries

`any` is the biggest source of unsoundness in TypeScript. It infects anything it touches by acting as both a top and a bottom type.

#### TSZ9201: `any` Taint Detection

> **Implementation: LIVE (partial).** `strict_any_propagation` → `AnyPropagationMode::TopLevelOnly` restricts `any` at nested depths via `STRICT_ANY` demotion in `SubtypeChecker`. The `SoundLawyer.is_assignable()` correctly implements the full taint model (`any` only assignable to `any`/`unknown`) but is dead code — not called from the checker. The live path is thinner: `any` is restricted at depth > 0 but still works at top-level.

In Sound Mode, `any` is treated as a **taint** — it may exist, but it cannot flow into structured types without an explicit boundary step.

**TypeScript allows:**
```typescript
const x: any = "hello";
const y: number = x;      // ✅ tsc: any → number
y.toFixed(2);             // 💥 Runtime: "hello".toFixed()
```

**Sound Mode rules:**
1. Developers are banned from writing explicit `any` — use `unknown` instead
2. `any` is only assignable to `any` and `unknown` in sound mode
3. `any` member access/call/new is either:
   - An error (strongest soundness), OR
   - A separate TSZ code (so teams can stage the migration)
4. If an `any` type bypasses SoundlyTyped infrastructure, the compiler strictly treats it as a top type equivalent to `unknown`

**How to fix:**
- Replace `any` with `unknown` and add narrowing
- Use runtime validation (Zod, ArkType) at trust boundaries
- Use the sound core libraries that return `unknown` from `JSON.parse`, `fetch().json()`, etc.

#### TSZ9202: Unsafe Type Assertions

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
- If truly needed: `// @tsz-unsound TSZ9202: validated by runtime check in validator.ts`

#### TSZ9203: Implicit `any` in Error Handling

**TypeScript allows (historically):**
```typescript
try { throw { message: "oops" }; }
catch (e) {
    e.toUpperCase(); // ✅ tsc allows if useUnknownInCatchVariables is false
}
```

**Sound Mode:** Strictly enforce `unknown` for catch variables. Developers *must* type-narrow or cast `e` before interacting with it. Furthermore, Sound Mode could introduce a strictly enforced `@throws` or explicit `throws ErrorType` syntax for function signatures to allow safely typing the catch block.

#### Ecosystem Boundaries & Strategies

Sound Mode's biggest blocker is ecosystem typing hygiene. It relies on **explicit boundary enforcement** via automated structural transformations of the ecosystem's definitions.

**1. `SoundlyTyped`: Automated Ecosystem Soundness (Boundary Overlay)**
To handle the massive ecosystem of `node_modules` and DefinitelyTyped (DT), we introduce **`SoundlyTyped`** — a fully automated infrastructure layer acting as a boundary-aware overlay, rather than a naive token-replacement fork.
- **Boundary-Aware Transformation:** Instead of replacing every `any` (which breaks DT's internal type-level machinery), it maps `any` to `unknown` specifically at **value flow boundaries** — return types, readable properties, and callback arguments flowing into consumer code.
- **Pattern Rewrites over Bans:** Rewrites bivariance hacks (e.g., React patterns) and method-to-property variances to sound equivalents.
- **Cached Overlays:** Transforms are cached by package/version/hash to keep it fast and incremental.

**2. Sound Core Libraries (`*.sound.lib.d.ts`)**
Standard libraries often return `any` for ergonomics. A true Sound Mode defaults to sound alternative core typings (e.g., `dom.sound.lib.d.ts`). For instance, `JSON.parse` and `fetch().json()` return `unknown`. This prevents `any` from entering the program in the first place.

**3. Developer Strategies**
When consuming code without `SoundlyTyped` guarantees:
- **Runtime Validation:** Use Zod or ArkType.
- **Explicit Module Augmentation:** Override unsound exports manually.
- **Deep Type Transformation:** Use utilities like `ReplaceAnyDeep<T, unknown>` at the boundary.

### Group 9300: Object Shapes & Mutation

#### TSZ9301: Weak Type Acceptance

**TypeScript allows (with warning):**
```typescript
interface Config { port?: number; host?: string; }
const opts = { timeout: 5000 };  // No overlap with Config
const config: Config = opts;     // ⚠️ tsc warns but allows
```

**Sound Mode:** Strictly reject objects with no overlapping properties.

#### TSZ9302: Exact Types & Object Iteration

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

#### TSZ9303: Readonly Property Aliasing

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

#### TSZ9304: Split Accessor Assignability

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

#### TSZ9305: Unsafe Object Mutation (`Object.assign`)

**TypeScript allows:**
```typescript
interface User { name: string; age: number; }
const user: User = { name: "Alice", age: 30 };

Object.assign(user, { age: "thirty" }); // ✅ tsc allows
user.age.toFixed(); // 💥 Runtime: user.age is a string!
```

**Sound Mode:** `Object.assign(target, ...sources)` must enforce that any overlapping properties in `sources` are deeply assignable to the corresponding properties in `target`.

#### TSZ9306: Sticky Freshness

> **Implementation: LIVE.** Sound mode skips `widen_freshness()` calls at 4 checker locations (`variable_checking/core.rs:844`, `state/state.rs:987`, `computation/call.rs:923`, `computation/identifier.rs:814`). Currently active under `sound: true` (not behind a separate pedantic flag, since pedantic layer doesn't exist yet). Freshness is preserved by keeping the `FRESH_LITERAL` object flag.

**Layer: Pedantic** (not part of core soundness)

Object literal freshness is preserved through variables to ensure excess property checks are not easily bypassed.

In a structural type system, "extra properties" are usually not a runtime crash; excess property checks are more of a typo-catcher heuristic. The existing example shows the heuristic being bypassed via indirection.

**Design decision:** Sticky Freshness belongs in the **pedantic** layer unless tied directly to `Exact<T>` semantics.

- **Option 1 (current):** Keep in pedantic layer. Available for teams who want stronger typo detection, but not part of the core soundness story.
- **Option 2 (future):** Tie freshness to `Exact<T>` — freshness matters when assigning to `Exact<T>`, not for all interface assignments. This aligns with the long-standing TypeScript exact types discussion.

#### TSZ9307: Missing Index Signature

Requires explicit definitions of index signatures where standard TypeScript infers them loosely from object structural assignments.

### Group 9400: Functions & Signatures

#### TSZ9401: Rest Parameter Bivariance

**TypeScript allows:**
```typescript
type Logger = (...args: any[]) => void;
const fn = (id: number, name: string) => {};
const logger: Logger = fn;  // ✅ tsc
logger("wrong", "types");   // 💥 Runtime: fn expects number, string
```

**Sound Mode:** Reject — `(...args: any[])` is NOT a supertype of specific signatures.

#### TSZ9402: Void Return Exception

**TypeScript allows:**
```typescript
type Callback = () => void;
const cb: Callback = () => "hello";  // ✅ tsc allows returning string
```

**Sound Mode:** Require return types to actually match.

#### TSZ9403: Opaque Function Type

**TypeScript allows:**
```typescript
const f: Function = (x: number) => x * 2;
f("not", "a", "number", 42, true);  // ✅ tsc allows any args!
```

**Sound Mode:** Reject assignment of specific function types to `Function`, or require explicit cast.

### Group 9500: Collections & Indexing

#### TSZ9501: Unchecked Index Access

> **Implementation: Planned.** Sound mode does NOT currently auto-enable `noUncheckedIndexedAccess`. The index access evaluation in `evaluate.rs:1263` reads the `no_unchecked_indexed_access` parameter from compiler options, but `sound: true` does not force this to `true`. It must be set separately. Wiring this is a one-line change.

**TypeScript allows:**
```typescript
const arr: number[] = [];
const x = arr[100];       // ✅ tsc: x is number
x.toFixed();              // 💥 Runtime: undefined.toFixed()
```

**Sound Mode:** Type index access as `T | undefined` by default. This matches TypeScript's `noUncheckedIndexedAccess` semantics exactly — sound mode should imply this behavior even if the tsconfig flag isn't set.

#### TSZ9502: Strict Array/Set Membership

**TypeScript restricts:**
```typescript
const arr: string[] = ["a", "b"];
arr.includes(1); // ❌ tsc errors: Argument of type 'number' is not assignable to parameter of type 'string'.
```

**Sound Mode:** Modify core library definitions so that `includes` and `has` accept `unknown`. This allows perfectly safe membership checks without arbitrary subtyping restrictions.

#### TSZ9503: Non-Empty Array Reduction

**TypeScript allows:**
```typescript
const arr: number[] = [];
const sum = arr.reduce((a, b) => a + b); // ✅ tsc allows, 💥 Runtime: TypeError on empty array
```

**Sound Mode:** If `reduce` is called without an initial value, the array must be proven to be non-empty (e.g., `[T, ...T[]]`), or the return type evaluates to `T | undefined`.

#### TSZ9504: Tuple Length Mutation

**TypeScript allows:**
```typescript
const tuple: [number, string] = [1, "hello"];
tuple.push(2);      // ✅ tsc allows, tuple is now [1, "hello", 2]
const len = tuple.length; // ✅ tsc says '2', 💥 Runtime: actual length is 3
```

**Sound Mode:** Tuple types omit length-mutating methods entirely, or those methods are typed to require `never` as an argument.

### Group 9600: Primitives & Enums

#### TSZ9601: Strict Nominal Enums

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

#### TSZ9602: Implicit Primitive Boxing

**TypeScript allows:**
```typescript
const o: Object = 42;     // ✅ tsc allows
const e: {} = "hello";    // ✅ tsc allows
```

**Sound Mode:** Reject primitive-to-`Object`/`{}` assignment.

### Group 9700: Generics & Type Algebra

#### TSZ9701: Conditional Type Distribution

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

#### TSZ9702: Intersection Survival

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

#### TSZ9703: Generic Constraint Confusion

**Status: Under review.** In modern TypeScript, `return t` where `T extends string` and return type is `U extends string` is already rejected with the "could be instantiated with a different subtype" error.

**Decision:**
- If modern tsc already rejects this, drop this planned item (tsz already matches tsc)
- If specific real unsoundness cases exist in current tsc behavior that tsz matches, re-scope to those specific cases

### Group 9800: Control Flow & Exhaustiveness

#### TSZ9801: Switch Statement Exhaustiveness

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

### Group 9900: Missing Soundness Gaps

> **Implementation: Planned.** None of the 9900-group codes exist in the codebase. However, feasibility is high: TSZ9901 (non-null `!`), TSZ9902 (definite assignment `!:`), TSZ9903 (catch `unknown`), and TSZ9904 (exact optional) all have existing infrastructure hooks that just need sound mode checks added. TSZ9903 is a one-line change (force `use_unknown_in_catch_variables = true`). TSZ9904 is similarly trivial (`exact_optional_property_types` already exists in `CheckerOptions`).

These additional checks complete the Sound Mode story of "make trust boundaries explicit and stop pretending runtime can't be wrong."

#### TSZ9901: Non-Null Assertion Escape

The non-null assertion operator (`!`) is an explicit unsound escape hatch, just like type assertions.

```typescript
function getUser(): User | null { return null; }

const user = getUser()!;  // ✅ tsc: trust me, it's not null
user.name;                 // 💥 Runtime: Cannot read properties of null
```

**Sound Mode:** Flag `!` as an unsound escape that requires suppression or a recognized null-check pattern.

#### TSZ9902: Definite Assignment Assertions

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

#### TSZ9903: Catch Variables Default to `unknown`

TypeScript has `useUnknownInCatchVariables` but it's not on by default. Sound mode implies this behavior.

```typescript
try {
  riskyOperation();
} catch (e) {
  // Sound mode: e is 'unknown', must narrow before use
  if (e instanceof Error) {
    console.log(e.message);  // Safe
  }
}
```

#### TSZ9904: Exact Optional Property Types

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

TypeScript handles this correctly. The issue is only in *assignability* (see TSZ9304).

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

## Implementation Considerations

### The Caching "Correctness Tax"

Caching is the hidden complexity of gradual soundness. Because Sound Mode is a policy switch over the same Judge solver, any feature that makes relation outcomes depend on policy (e.g., `soundArrayVariance`) must be reflected in the cache key.
- **Policy Bitsets:** The solver's relation cache keys must include a versioned policy ID or bitset. This ensures that a `Dog[]` to `Animal[]` relation check caches as `false` in Sound Mode, but doesn't pollute the cache for a non-sound file that expects `true`.

> **Current state:** `RelationCacheKey` in `types.rs:259` has `flags: u16` (4 compiler flags) and `any_mode: u8`. The `any_mode` field differentiates sound vs non-sound `any` propagation behavior. However, `strict_subtype_checking` (which affects `CompatChecker` behavior like method bivariance) is NOT reflected in the cache key. This is a correctness gap: sound and non-sound compat results could theoretically collide in the cache. A `FLAG_SOUND_MODE` bit should be added.

### Compatibility & Performance

Sound Mode rejects some valid TypeScript code. Migration requires running in "report only" mode and manually fixing code structures. Most sound checks are `O(1)` additions to existing checks. The main cost is tracking read/write types separately for properties and additional variance checks on generic instantiation.

---

## Trade-offs Summary

| Aspect | TypeScript (tsc) | Sound Mode (Core) | Pedantic |
|--------|------------------|-------------------|----------|
| Array variance | Covariant (unsafe) | Invariant for mutable | — |
| Method params | Bivariant | Contravariant | — |
| `this` in params | Covariant | Contravariant | — |
| `any` type | Top + Bottom | Top only (taint model) | — |
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

**The fundamental trade-off:** Sound Mode catches more bugs but requires more explicit annotations. It's not "better" — it's different. Choose based on your project's needs.

---

## References

- [TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md) - Complete list of TypeScript's intentional unsoundness
- [NORTH_STAR.md](../architecture/NORTH_STAR.md) - Judge/Lawyer architecture documentation
- [TypeScript Design Goals](https://github.com/Microsoft/TypeScript/wiki/TypeScript-Design-Goals) - Why TypeScript chose pragmatism
- [TypeScript Soundness Documentation](https://www.typescriptlang.org/docs/handbook/type-compatibility.html#a-note-on-soundness) - TS's own soundness discussion
