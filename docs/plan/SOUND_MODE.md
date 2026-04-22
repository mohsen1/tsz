# Sound Mode

**Status**: Experimental / Partially Implemented
**Last Updated**: April 2026

## Doc Status

This file currently mixes four jobs:

1. current implementation status
2. a narrow product contract
3. design notes for possible future checks
4. a roadmap / backlog

That is useful during exploration, but it is also a drift risk. Until this file is split, read it in this order of authority:

1. **Implementation Status** is the source of truth for what works today.
2. **The Narrow Contract** plus **First Shippable Slice** are the source of truth for the first stable target.
3. The later check inventory and later phases are design notes, not commitments.

Once the contract stabilizes, this doc should split into separate status, contract, roadmap, and boundary-design documents.

## Quick Start

```bash
# Enable sound mode via CLI
tsz check --sound src/
```

```json
{
  "compilerOptions": {
    "sound": true
  }
}
```

Per-file pragmas and broader `sound*` option families are still not wired, and the full public config shape remains intentionally open.

## Implementation Status

Today, sound mode is a project-wide boolean exposed through both CLI (`--sound`) and tsconfig (`compilerOptions.sound`). Per-file pragmas and server/LSP exposure are not wired yet. The live implementation works by tightening the existing Lawyer (`CompatChecker`) with `strict_subtype_checking` and `strict_any_propagation` flags on `RelationPolicy`. Errors are still emitted as standard TS diagnostic codes (TS2322, TS2345, etc.), not dedicated TSZ-family sound diagnostics yet.

**Note on diagnostic codes:** The codebase defines `SoundDiagnosticCode` with codes TS9001–TS9005 in `crates/tsz-solver/src/sound.rs`. Those should be treated as temporary implementation placeholders, not the public contract. This doc uses the target **family-based TSZNNNN taxonomy** (`TSZ1000`, `TSZ2000`, etc.), which has not yet been applied to the implementation. A `DiagnosticDomain::Sound` infrastructure exists but is not yet used by the checker.

| Feature | Target Code | Impl Code | Status | Mechanism |
|---------|------------|-----------|--------|-----------|
| Method bivariance | TSZ2002 | TS9003 | LIVE | `strict_subtype_checking` disables method bivariance |
| `any` restriction | TSZ1001 | TS9004 | LIVE | `strict_any_propagation` → `TopLevelOnly` mode |
| Sticky freshness | TSZ3006 | TS9001 | LIVE (temporary) | Skips `widen_freshness()` in 4 checker locations even though the target design treats it as pedantic |
| Mutable array covariance | TSZ2001 | TS9002 | Dead code | `SoundLawyer.check_array_covariance()` exists but not wired into pipeline |
| Enum-number assignment | TSZ6001 | TS9005 | Dead code | `SoundModeConfig.strict_enums` defined but unused |
| Unsafe type assertion | TSZ1011 | — | Planned | No implementation |
| Unchecked indexed access | TSZ5001 | — | Planned | Not auto-enabled by `sound: true`; uses standard `noUncheckedIndexedAccess` flag separately |
| Missing index signature | TSZ3007 | — | Planned | Currently uses standard TS2329 |

**Known implementation gaps:**
- `SoundLawyer` struct in `sound.rs` is fully defined but never called from the checker pipeline (dead code)
- `SoundModeConfig` struct with granular flags is defined but never consumed
- cache correctness still needs a precise sound-policy audit: method-bivariance disablement is now represented in relation flags, but `strict_any_propagation` still piggybacks on `FLAG_STRICT_FUNCTION_TYPES`, and some query-cache helper paths still construct keys with `any_mode = 0`
- Only `compilerOptions.sound` is currently wired; broader flat `sound*` family options remain planned
- `tsz-server` / LSP currently hardcodes `sound_mode: false`
- CLI help / dead prototype code still reference older `TS900x` semantics and should not be treated as the product contract

See `crates/tsz-solver/src/sound.rs` for sound mode definitions and `crates/tsz-checker/src/query_boundaries/assignability.rs` for the live integration.

---

## The Narrow Contract

TypeScript explicitly lists "apply a sound or provably correct type system" as a **non-goal**. tsz can still use the word `sound`, but only if the document is narrow and boringly precise about what that means.

### Stable Target Contract

The first stable meaning of `sound` should be something close to:

> Within user-authored TypeScript implementation files checked under sound scope, excluding explicit auditable escapes and treating declaration files as trust boundaries, tsz rejects a small listed set of TypeScript assignability patterns and precision holes known to cause type-driven runtime failures or to hide unknown-at-runtime values.

Important limits:

1. This is **not** full language soundness.
2. This is **not** a theorem or formal proof.
3. This does **not** mean `.d.ts` files are truthful.
4. This does **not** promise declaration-boundary quarantine in the first shipping slice.

### Current Reality

Today, even that narrow target is not fully implemented:

1. Sound mode is currently a project-wide boolean from CLI (`--sound`) or tsconfig (`compilerOptions.sound`).
2. Method bivariance tightening is live.
3. `any` handling is only partial: nested restrictions exist, but top-level `any` still behaves too permissively.
4. Declaration-boundary quarantine is **not** implemented.
5. Dedicated TSZ sound diagnostics and auditable suppressions are **not** implemented.
6. Sticky freshness is currently active under `--sound` even though the target design treats it as pedantic.

### First Stable Target

The first stable target should stay intentionally small:

1. Scope only user-authored `.ts` / `.tsx` / `.mts` / `.cts` implementation code.
2. Ban explicit `any` in that code.
3. Disable method bivariance in sound-scoped assignability.
4. Imply `useUnknownInCatchVariables`, `noUncheckedIndexedAccess`, and `exactOptionalPropertyTypes`.
5. Emit dedicated TSZ sound diagnostics plus auditable suppressions.
6. Treat declaration files as trust boundaries, but do **not** promise general quarantine yet.

### Scope Model

The key product decision is that Sound Mode should optimize for **developer life in application code**, not for perfectly purifying the entire npm ecosystem on day one.

Default scope:

1. **Checked as sound user code**: user-authored `.ts`, `.tsx`, `.mts`, `.cts` implementation code that is part of the project and is not being treated as declaration surface.
2. **Treated as trust-boundary inputs**: `.d.ts`, `.d.mts`, `.d.cts`, `.d.tsx`, default libs, third-party declaration files under `node_modules`, ambient/declaration surfaces, and declaration outputs consumed through project references / composite builds.
3. **Out of first stable scope**: JS/JSDoc, `allowJs` / `checkJs`, and mixed per-file opt-in/out semantics.
4. **Not the primary ergonomics target**: soundness inside declaration internals themselves. The first priority is what user code can rely on after crossing the boundary.

Scope notes:

1. The current implementation is still effectively a project-wide boolean, not a per-file scope system.
2. Ambient declarations inside ordinary `.ts` files should follow the declaration-boundary story by default, not the "ban explicit `any` in user implementation code" story.
3. First-party emitted `.d.ts` from composite projects should also be treated as declaration-boundary input unless and until a stricter opt-in mode is enabled.

Planned follow-on options:

1. `soundPedantic: true` for bug-finding heuristics in user code
2. `soundCheckDeclarations: true` for teams that also want first-party declaration files checked as sound source
3. `soundReportOnly: true` for staged rollout without failing CI immediately

This default keeps the product promise simple:

1. **Your source files stop writing `any`.**
2. **Third-party and referenced-project declarations can still exist as they are today.**
3. **What crosses from those declarations into your code is treated as a boundary problem.**
4. **General `any` quarantine at that boundary is planned, but not part of the first stable promise.**

### What Sound Mode Does NOT Guarantee

- Runtime immutability (JavaScript is fundamentally mutable; type erasure applies)
- Protection against code outside Sound Mode scope (non-sound files, native modules)
- That all possible runtime errors are caught (only *type-driven* ones)
- That third-party declaration files are themselves internally perfect or truthful
- That external JavaScript implementations honor their declarations without validation

---

## Two-Layer Configuration

> **Status: Planned design, still intentionally unresolved.** Currently sound mode is a single CLI `bool` (`CheckerOptions.sound_mode`). Before blessing any tsconfig shape, the project needs an explicit coexistence story with vanilla `tsc`, editor tooling, schema validation, and mixed `tsc` / `tsz` workflows.

Sound Mode is a **dial**, not a single switch. But the first stable bundle should stay much smaller than the full design inventory.

### Public Config Decision Gate

Reasonable options still on the table:

1. Keep Sound Mode CLI-only while semantics are experimental.
2. Add a `tszOptions`-style config surface separate from `compilerOptions`.
3. Add a `compilerOptions.sound` family only if coexistence with `tsc` is acceptable.

If the project chooses a `compilerOptions`-owned surface, the preferred shape is still:

1. one `sound` master switch
2. flat sibling `sound*` options rather than a nested `sound: { ... }` object
3. no competing parallel config philosophies

### Layer A: First Stable Sound Bundle

This is the bundle the project should actually hold itself accountable to first. It intentionally includes only checks we can defend either as soundness-critical or as precision flags required to make the mode coherent.

Core Layer defaults:

1. Ban explicit `any` in user-authored non-declaration code
2. Disable method bivariance in sound-scoped assignability
3. Force `useUnknownInCatchVariables`
4. Force `noUncheckedIndexedAccess`
5. Force `exactOptionalPropertyTypes`
6. Keep declaration-file internals out of the primary diagnostic surface unless explicitly opted in
7. Do **not** promise general declaration-boundary `any` quarantine in the first stable bundle

| Diagnostic | Description |
|-----------|-------------|
| TSZ2002 | Method parameter bivariance |
| TSZ1001 | Explicit `any` in sound-scoped user code |
| TSZ1031 | Catch variables default to `unknown` |
| TSZ5001 | Unchecked indexed access |
| TSZ3008 | Exact optional property types |

### Layer B: Pedantic / Research Candidates

These checks may still be useful, but they should not be marketed as the first stable sound contract:

| Diagnostic | Description |
|-----------|-------------|
| TSZ3006 | Sticky freshness / excess-property hardening |
| TSZ3002 | `Exact<T>` and object-iteration semantics |
| TSZ2001 | Mutable array covariance once actually wired |
| TSZ1011 / TSZ1021 / TSZ1022 | Unsafe assertions and explicit escape hatches |
| TSZ5002 / TSZ5003 | Membership ergonomics and non-empty reductions |
| TSZ6002 / TSZ8001 / TSZ4002 | Primitive boxing, exhaustiveness, and strict `void` matching |

### Planned Config Defaults

If Sound Mode eventually lives in a tsconfig-managed surface, the conceptual defaults should look like:

```jsonc
{
  "sound": true,
  "soundPedantic": false,
  "soundCheckDeclarations": false,
  "soundReportOnly": false
}
```

Default profile semantics:

1. `sound: true`: enable the default sound profile
2. `soundPedantic`: add bug-finding heuristics that go beyond strict runtime-safety checks
3. `soundCheckDeclarations`: opt first-party declaration files into sound checking too
4. `soundReportOnly`: report sound diagnostics without making them fail the run
5. The default sound profile itself still bans explicit `any` in user source, treats declaration unsoundness as a boundary problem, and may later swap in sound library surfaces where available

### Configuration Examples

Illustrative only: these examples show the preferred *shape* if Sound Mode eventually lives in a config-managed surface. They are not implemented today, and the exact owning config object is still open.

```jsonc
// Core soundness only (recommended starting point)
{
  "sound": true
}

// Core + pedantic bug-finding heuristics
{
  "sound": true,
  "soundPedantic": true
}

// Core soundness + also check first-party declaration files
{
  "sound": true,
  "soundCheckDeclarations": true
}

// Migration mode: report but do not fail CI yet
{
  "sound": true,
  "soundReportOnly": true
}

// Targeted sound rollout
{
  "sound": true,
  "soundPedantic": true,
  "soundReportOnly": true,
  "soundArrayVariance": true,
  "soundMethodVariance": true
}
```

This keeps the `sound` naming family coherent without prematurely committing to one tsconfig embedding strategy.

---

## Executive Summary

TypeScript is **intentionally unsound**. The TypeScript team made deliberate design choices to prioritize developer ergonomics over type-theoretic correctness. These choices are documented in [docs/specs/TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md).

tsz's **Judge/Lawyer architecture** separates concerns:
- **Judge (Core Solver)**: Implements strict, sound set-theory semantics
- **Lawyer (Compatibility Layer)**: Applies TypeScript-specific rules to match tsc behavior

This architecture enables an experimental Sound Mode path. **Today it does not bypass the Lawyer layer.** The live implementation tightens the existing compat / relation path with stricter `RelationPolicy` flags. A future architecture might lean more directly on stricter solver semantics, but that is not current behavior and should not be described as such.

---

## Diagnostics as a Product Surface

> **Status: Planned design.** The `DiagnosticDomain::Sound` and `SoundDiagnosticCode` enum exist in code but are not used by the checker. None of the CLI features below (`tsz explain`, `--sound-summary`) or suppression mechanisms (`@tsz-unsound`) are implemented yet.

Before the richer UX in this section matters, the real MVP diagnostic bar is smaller:

1. one dedicated sound diagnostic path
2. one public code format
3. code-aware suppressions with required reasons
4. stale-suppression checking

Everything else here should be treated as follow-on UX, not as a blocker for the first credible release.

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

### CLI Explain Command (Later UX)

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

### Sound Summary Mode (Later UX)

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

**Suppression semantics**: `@tsz-unsound` should remain **diagnostic-only (suppress-only)**. It can locally mute error output, but it should not silently switch the checker back into TypeScript-compatible semantics for that node.

That said, diagnostic-only suppression is probably **not enough** as the only escape hatch. Before Sound Mode is considered truly user-facing, tsz likely needs one separate **auditable semantic escape primitive** (for example an `unsafeAssume` / `tsz.unsafe.cast`-style boundary) so users can express intentional unsoundness without pretending that a comment is enough.

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

## Checks and Candidate Checks

This section is an inventory, not the stable contract. Read every subsection through its status label:

1. **LIVE** means reachable from the current implementation.
2. **TARGET** means intended for the first stable sound bundle.
3. **RESEARCH / UNDER REVIEW** means later candidate, design note, or open question.

If a subsection does not say otherwise, treat it as design discussion rather than current product behavior.

### Variance & Subtyping

#### TSZ2001: Covariant Mutable Arrays

> **Implementation: PARTIAL / not part of the first stable contract.** `SoundLawyer.check_array_covariance()` in `sound.rs` can detect covariant array assignments, but `SoundLawyer` is not wired into the checker pipeline. The live `SubtypeChecker` still treats arrays covariantly in sound mode. The diagnostic helper exists but is disconnected from the assignability flow.

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
- compiler-managed declaration overlays can mechanically convert method signatures to function-valued properties
- Support a JSDoc tag on method parameters: `/** @tsz-bivariant */`
- A practical mechanical transformation: convert `foo(x: T): R` → `foo: (x: T) => R` in object types

**Library patch strategy:** compiler-managed declaration overlays can automatically rewrite bivariant methods in upstream `.d.ts` files to use function-property syntax where safe.

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

**Target rules:**
1. In user-authored non-declaration files, developers are banned from writing explicit `any` — use `unknown` instead
2. `any` is never allowed to behave like a bottom type inside sound user code
3. Declaration-origin `any` is tolerated as an ecosystem input, but declaration-boundary quarantine is **later work**, not part of the first stable promise
4. `any` member access / call / `new` in sound user code is a dedicated error unless the value has been validated, narrowed, or explicitly escaped through an auditable mechanism

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

Do **not** overload one umbrella code with unrelated migration stories. A more credible split is:

1. **TSZ1001** — explicit `any` in sound-scoped user code
2. **TSZ1002** — declaration-boundary `any` exposed to user code (later, once quarantine exists)
3. **TSZ1003** — unsafe operation on a tainted / unvalidated boundary value

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

Sound Mode's biggest blocker is ecosystem typing hygiene. The right product story is **not** "install a second universe of declarations." It is: **tsz consumes ordinary `.d.ts` files and exposes a sound view at the boundary.**

The central strategy is:

1. Do **not** make developers clean npm by hand before they can use Sound Mode.
2. Do **not** try to make `TypeId::ANY` provenance-aware across the whole solver.
3. Prefer **compiler-managed boundary projection** and small internal overlays over a public transformed-package product.
4. Keep `SoundlyTyped` only as an internal codename, if it survives at all.

**1. Compiler-managed declaration boundary projection (recommended general mechanism)**

This is the right direction, but it is **not yet a full spec**. Before boundary-projection implementation starts, tsz should write a separate boundary-design document that formalizes:

1. overloads
2. generics and inferred type parameters
3. conditional, mapped, indexed-access, and `infer` positions
4. higher-order callbacks and `this` parameters
5. aliases, re-exports, type-only imports, and namespace / declaration merging
6. module augmentation and project-reference boundaries
7. mutable properties with split read / write surfaces

For arbitrary third-party `.d.ts`, tsz should compute a **sound-facing projection** when sound user code observes declaration-owned types.

- Detect declaration-owned symbols through existing file ownership / declaration-file metadata.
- Project types with **polarity**, not with blind token replacement.
- Positive/read positions should quarantine `any` to `unknown`: return types, readable properties/getters, yielded/awaited values, and callback parameters supplied by the library to user code.
- Negative/write positions can usually stay permissive: ordinary function parameters where the user is only passing values *into* the library do not need quarantine.
- Mixed surfaces should use split read/write types where the solver already models them (`PropertyInfo.type_id` vs `write_type`).

Examples of the target user-facing experience:

1. `declare function parse(text: string): any` → sound user code experiences `(text: string) => unknown`
2. `declare const box: { value: any }` → reading `box.value` yields `unknown`, while the write surface can stay permissive
3. `declare function onValue(cb: (value: any) => void): void` → sound user code experiences `(cb: (value: unknown) => void) => void`

Why polarity matters:

1. `handler(cb: (x: any) => void)` is unsafe because the library pushes `x` into user code.
2. `consume(x: any): void` is a different problem: the user is sending values into the library, not receiving an unsound value back.
3. A blind deep `any -> unknown` rewrite loses this distinction and will either over-restrict APIs or fail to protect the dangerous cases.

This is the strongest first general mechanism because it keeps the promise "use normal `.d.ts` files" while avoiding solver-wide provenance tracking.

**2. Compiler-managed internal declaration overlays (recommended curated mechanism)**
Some fixes are clearer as declaration rewrites than as on-demand projections:

1. Core library surfaces like `JSON.parse(): unknown` and `Response.json(): Promise<unknown>`
2. Method-to-property rewrites for deliberate bivariance hacks
3. Selected DOM / React / Node patterns where a structural patch is easier to reason about than a per-use projection

If tsz materializes these rewrites internally and caches them, that is fine, but it should remain an **implementation detail of the compiler** rather than a separate product users must reason about.

**Projection vs overlay decision rule**

1. Prefer **boundary projection** when the problem is:
   - about values crossing from declarations into sound user code
   - polarity-sensitive
   - expressible on demand without inventing new declaration text
   - needed for arbitrary third-party `.d.ts` inputs
2. Prefer a **curated internal overlay** when the problem is:
   - attached to a small, known API surface
   - stable enough to precompute as declaration text
   - awkward or too expensive to express as per-use projection
   - something we want every consumer to experience consistently before ordinary type resolution begins
3. Start with projection as the general mechanism and treat overlays as targeted exceptions.
4. Do not let overlays become a backdoor for broad ecosystem rewriting before the boundary projector is proven.

**3. What should not be the first mechanism**
Do **not** try to solve the problem initially by:

1. Threading declaration provenance through every solver relation
2. Blindly replacing every `any` in every `.d.ts`
3. Rewriting pure type-level helper aliases unless they leak into observable value surfaces
4. Making generated overlays a prerequisite for the first useful Sound Mode release

**4. Developer Strategies**
When a library boundary still needs human help:
- **Runtime Validation:** Use Zod or ArkType.
- **Explicit Module Augmentation:** Override unsound exports manually.
- **Local Wrapper Helpers:** Validate and narrow once at the edge, then expose a clean internal type.

**First-party `.d.ts` policy (planned):**

1. Default: first-party declaration files are exempt from the explicit-`any` ban, just like third-party declaration files
2. Reason: forcing declaration cleanup before source adoption makes the first rollout much harder
3. Future opt-in: `soundCheckDeclarations: true` can apply the same no-`any` discipline to first-party `.d.ts` once a team is ready

**Composite / project-reference policy (planned):**

1. Emitted `.d.ts` from referenced composite projects is a **primary** trust-boundary input, not a corner case
2. A sound consumer project must be able to depend on a non-sound referenced project through its normal declaration output
3. The consumer project gets the projected sound view; the referenced project's declaration output does **not** need to be manually rewritten first
4. If source-of-project-reference redirect behavior is later expanded in tsz, its sound-mode semantics should be defined explicitly rather than quietly changing the `.d.ts` contract story

### Object Shapes & Mutation

#### TSZ3001: Weak Type Acceptance

> **Status: UNDER REVIEW.** Modern TypeScript already errors for many "no properties in common" weak-type assignments. Do not treat this as a first-stable Sound Mode differentiator until the current baseline behavior is re-verified and the remaining delta is clearly identified.

**Historically, TypeScript could allow or weakly diagnose patterns like:**
```typescript
interface Config { port?: number; host?: string; }
const opts = { timeout: 5000 };  // No overlap with Config
const config: Config = opts;     // ⚠️ tsc warns but allows
```

**Sound Mode (if kept at all):** Reject objects with no overlapping properties. This is better understood as a stricter object-shape rule than as a core soundness theorem.

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
mut.id = 2;               // Violates the readonly expectation through an alias
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

**Sound Mode:** `Object.assign(target, ...sources)` should at most enforce **property-wise shallow assignability** for overwritten keys. This area is more subtle than a simple structural check because accessors, descriptors, optional properties, and aliasing all matter.

#### TSZ3006: Sticky Freshness

> **Implementation: LIVE.** Sound mode skips `widen_freshness()` calls at 4 checker locations (`variable_checking/core.rs:844`, `state/state.rs:987`, `computation/call.rs:923`, `computation/identifier.rs:814`). Currently active under `sound: true` (not behind a separate pedantic flag, since pedantic layer doesn't exist yet). Freshness is preserved by keeping the `FRESH_LITERAL` object flag.

**Layer: Pedantic** (not part of the first stable sound contract)

Object literal freshness is preserved through variables to ensure excess property checks are not easily bypassed.

In a structural type system, "extra properties" are usually not a runtime crash; excess property checks are more of a typo-catcher heuristic. The existing example shows the heuristic being bypassed via indirection.

**Design decision:** Sticky Freshness belongs in the **pedantic** layer unless tied directly to `Exact<T>` semantics. Its current activation under `--sound` should be treated as a temporary implementation mismatch, not as the target product contract.

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

**Sound Mode (candidate strictness check, not first-stable core):** Require return types to actually match. This is a stricter callable-surface rule, but it is not as clear-cut a "tsc allows this and it crashes" soundness case as method bivariance or unchecked index access.

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

**Sound Mode (library ergonomics candidate, not core soundness):** Modify core library definitions so that `includes` and `has` accept `unknown`. This may be a useful library-surface improvement, but it does not belong in the first stable soundness contract.

#### TSZ5003: Non-Empty Array Reduction

**TypeScript allows:**
```typescript
const arr: number[] = [];
const sum = arr.reduce((a, b) => a + b); // ✅ tsc allows, 💥 Runtime: TypeError on empty array
```

**Sound Mode:** If `reduce` is called without an initial value, the array must be proven to be non-empty (for example `[T, ...T[]]`) or the call must be rejected. Typing this as `T | undefined` would be wrong because JavaScript throws on empty arrays rather than returning `undefined`.

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

**Library strategy:** compiler-managed declaration overlays can "close open numeric enums" in upstream `.d.ts` files where we decide the compatibility trade-off is worth it.

#### TSZ6002: Implicit Primitive Boxing

**TypeScript allows:**
```typescript
const o: Object = 42;     // ✅ tsc allows
const e: {} = "hello";    // ✅ tsc allows
```

**Sound Mode (later strictness candidate, not first-stable core):** Reject primitive-to-`Object`/`{}` assignment.

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

**Sound Mode (bug-finding / strictness candidate, not first-stable core):** Enforce native switch exhaustiveness. This is valuable, but it should not be marketed as a foundational soundness guarantee unless tied to a stricter return-type contract.

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

## Compiler-Managed Declaration Overlays: Auditability & Trust

> **Implementation: Planned.** No declaration-overlay pipeline exists in the codebase yet. There is no projected-type cache, no pre-parse transform cache, no `*.sound.lib.d.ts` set, and no debug diff surface for transformed declarations.

If tsz rewrites or overlays declarations internally, trust and reproducibility still matter. The important product change is that users should not have to think in terms of a separate `SoundlyTyped` universe. This should feel like **compiler infrastructure**, not a second package manager.

### Deterministic Cache

```
.tsz/sound-overlays/
  objects/
    <entry_hash>/
      manifest.json
      files/
        index.d.ts
        ...
  locks/
    <package_scope>.lock
  index.json
```

`objects/<entry_hash>/` is the authoritative immutable output tree. `index.json` is only an advisory lookup table that points from "this exact package + resolution profile + transform profile + upstream declaration closure" to a committed object. Readers must ignore `tmp/` staging directories and any object without a valid manifest.

### Metadata

```jsonc
// .tsz/sound-overlays/index.json
{
  "schema_version": 1,
  "entries": {
    "react@18.3.1#npm:sha512-abc...#node16#types": {
      "entry_hash": "sha256:entry123...",
      "subject": {
        "kind": "package",
        "name": "react",
        "version": "18.3.1",
        "package_manager": "npm",
        "integrity": "sha512-abc...",
        "package_json_hash": "sha256:pkg123..."
      },
      "resolution_profile_hash": "sha256:res123...",
      "transform_profile_hash": "sha256:tx123...",
      "upstream_declaration_closure_hash": "sha256:src123...",
      "output_tree_hash": "sha256:out123...",
      "state": "ready"
    }
  }
}
```

```jsonc
// .tsz/sound-overlays/objects/sha256:entry123.../manifest.json
{
  "schema_version": 1,
  "entry_hash": "sha256:entry123...",
  "subject": {
    "kind": "package",
    "name": "react",
    "version": "18.3.1",
    "package_manager": "npm",
    "integrity": "sha512-abc...",
    "package_json_hash": "sha256:pkg123..."
  },
  "resolution_profile_hash": "sha256:res123...",
  "transform_profile_hash": "sha256:tx123...",
  "upstream_declaration_closure_hash": "sha256:src123...",
  "output_tree_hash": "sha256:out123...",
  "transforms_applied": [
    {
      "id": "any_to_unknown_boundaries",
      "impl_version": "1.2.0",
      "options_hash": "sha256:opt001..."
    },
    {
      "id": "method_to_property_variance",
      "impl_version": "1.2.0",
      "options_hash": "sha256:opt002..."
    },
    {
      "id": "enum_closing",
      "impl_version": "1.2.0",
      "options_hash": "sha256:opt003..."
    }
  ],
  "upstream_declaration_closure": {
    "entrypoints": [
      "./index.d.ts"
    ],
    "files": [
      {
        "path": "index.d.ts",
        "sha256": "sha256:file001...",
        "bytes": 48213
      }
    ],
    "package_metadata": {
      "types": "./index.d.ts",
      "exports_hash": "sha256:exp123...",
      "types_versions_hash": null
    }
  },
  "output_files": [
    {
      "path": "files/index.d.ts",
      "sha256": "sha256:out001...",
      "bytes": 47902
    }
  ],
  "state": "ready"
}
```

The object store should be **shared** between external packages and referenced-project emitted declarations. They should use the same `objects/<entry_hash>/` layout, the same manifest schema, and the same atomic commit protocol. The thing that differs is the tagged `subject` identity and the lock scope, not the storage backend. This keeps cache GC, debug tooling, and correctness rules unified while still preventing package/project collisions.

See `docs/plan/SOUND_OVERLAY_CACHE_NOTE.md` for the exact `resolution_profile_hash` canonical payload, the package-vs-project subject split, and a minimal Rust schema prototype.

The current placeholder shape (`react@18.3.1` + `transform_version` + one `output_hash`) is not strong enough for real reuse. It can alias patched installs, stale `exports` maps, different module-resolution contexts, and partial writes.

### Exact `transform_profile_hash` Inputs

`transform_profile_hash` should be `sha256(canonical_json(...))` over exactly these inputs:

1. `overlay_schema_version`
2. Ordered transform pipeline
3. Each transform's stable `id`
4. Each transform's implementation version or build fingerprint
5. Each transform's canonicalized option payload
6. Sound-policy flags that change emitted declaration text
7. Printer or text-normalization settings if output bytes depend on them

That means the hash should change when:

1. We add or remove a transform
2. We reorder transforms
3. A transform's behavior changes
4. A transform-specific knob changes
5. The overlay printer changes in a way that affects bytes on disk

It should **not** include upstream file contents, package identity, or resolution-selected entrypoints. Those belong to the upstream declaration closure and resolution profile hashes, not the transform profile.

A concrete canonical payload should look like:

```jsonc
{
  "overlay_schema_version": 1,
  "pipeline": [
    {
      "id": "any_to_unknown_boundaries",
      "impl_version": "1.2.0",
      "options": {
        "callback_parameter_policy": "project_to_unknown",
        "readable_property_policy": "project_to_unknown",
        "top_level_sink_parameter_policy": "preserve_any"
      }
    },
    {
      "id": "method_to_property_variance",
      "impl_version": "1.2.0",
      "options": {
        "skip_overloads": true
      }
    },
    {
      "id": "enum_closing",
      "impl_version": "1.2.0",
      "options": {
        "close_numeric_enums": true
      }
    }
  ],
  "printer": {
    "line_endings": "lf",
    "trailing_newline": true
  }
}
```

### What Counts As the "Upstream Declaration Closure"

For one resolved package under one resolution profile, the upstream declaration closure is:

1. The subset of `package.json` fields that can affect declaration entrypoint selection or type-surface shape:
   - `name`
   - `version`
   - `types` / `typings`
   - `exports`
   - `imports`
   - `typesVersions`
   - `main` / `module` only if declaration resolution can fall back to them
2. Every package-owned declaration file reachable from every exported type entrypoint selected by the active resolution profile
3. Reachability through:
   - relative imports and re-exports
   - `import("...")` type queries
   - triple-slash references
   - package-owned module augmentations
   - package-owned `declare module` fragments that contribute to exported surfaces
4. The canonical list of selected entrypoints after `exports` / `typesVersions` evaluation

The closure must be canonicalized by package-relative path and file bytes. It should hash the **raw bytes** of each package-owned declaration file plus the normalized metadata payload above.

The closure must **exclude**:

1. Files outside the package root, even if the package references them
2. Transitive dependency declarations in other packages
3. Default libs
4. Consumer-owned augmentations
5. Previously generated overlay outputs

For composite-project declaration overlays, the analogous closure is the emitted `.d.ts` output tree for the referenced project plus the emission-affecting compiler options that determine those output bytes.

### Atomic Write / Read Protocol

The overlay cache should use a commit protocol, not blind "write files then trust the lockfile" reuse.

**Write protocol**

1. Resolve `package_identity`, `resolution_profile_hash`, `transform_profile_hash`, and `upstream_declaration_closure_hash`
2. Compute `entry_hash = sha256(package_identity, resolution_profile_hash, transform_profile_hash, upstream_declaration_closure_hash)`
3. Acquire `locks/<package_scope>.lock`
4. Re-read `index.json` under the lock and reuse an existing `ready` object only if its manifest still matches the expected hashes
5. Otherwise write the overlay into a fresh staging directory such as `.tsz/sound-overlays/tmp/<entry_hash>.<pid>.<nonce>/`
6. Write all transformed declaration files, then `manifest.json`
7. `fsync` staged files and directories
8. Atomically rename the staging directory to `objects/<entry_hash>/`
9. `fsync` the parent directory
10. Rewrite `index.json` via `index.json.tmp` + `rename`
11. `fsync` the cache root and release the lock

**Read protocol**

1. Compute the same expected hashes for the current request
2. Read `index.json` only as a hint
3. Load `objects/<entry_hash>/manifest.json`
4. Require:
   - `state == "ready"`
   - exact match for package identity, resolution profile, transform profile, and upstream declaration closure hashes
5. Verify every file listed in `output_files` exists and matches its recorded hash at least once per process
6. Memoize verified `entry_hash` values in memory for the rest of the process
7. If any check fails, ignore the cache entry and regenerate

**Crash / race behavior**

1. Crash before staging-directory rename: only orphaned temp files exist, and readers ignore them
2. Crash after object rename but before index rewrite: the object is valid but unreachable; later GC can clean it up
3. Crash during index rewrite: atomic rename leaves the previous `index.json` intact
4. Concurrent writers serialize on the package lock, so they cannot publish mixed manifests or partially overlapping output trees

This is the minimum bar for making compiler-managed declaration overlays auditable and safe enough to trust.

### Optional Debug Command

```bash
# Human-readable summary of what the compiler overlay changed
tsz debug sound-overlays react

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

If people can audit it quickly, they'll actually trust it.

---

## Detailed Rollout Plan

This is the execution plan that best matches the "any-less user code, tolerant external declarations" model.

Important discipline:

1. **Phase 1 is the real product target.**
2. Everything after that is Phase 2+ engineering and research backlog.
3. The existence of a later phase does **not** mean the product contract should promise it early.

### Validation Result

This plan has been checked against the current codebase rather than written as a pure wish list.

1. `cargo run --quiet --bin audit-unsoundness -- --summary` is an **inventory signal**, not a product milestone.
2. "44/44 rules implemented somewhere" does **not** prove those rules are reachable from Sound Mode, scoped correctly, diagnosed correctly, cached correctly, or safe for mixed `.ts` / `.d.ts` programs.
3. The useful conclusion is narrower: the solver likely has enough raw machinery for the MVP, and the remaining work is mostly about **policy exposure, scope control, diagnostics, suppressions, cache correctness, and declaration-boundary ergonomics**.

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

6. **Medium feasibility as architecture work, low feasibility as a "small patch": declaration-origin `any` quarantine**
   Evidence: there is currently no general declaration-origin provenance threading in the solver, but symbol→file ownership, declaration-file metadata, string-based declaration loading, and project-reference `.d.ts` freshness tracking already exist.
   Why: the plan's "external `.d.ts` may contain `any`, but user code sees `unknown`" story is correct, and it can plausibly be implemented as a boundary projection layer or internal overlay system that also covers composite-project declaration outputs.
   Main caveat: it should **not** be attempted as ad hoc `TypeId::ANY` special-casing inside relation checks. It needs a dedicated boundary projection/cache or transform layer.

7. **High strategic value but high implementation cost: compiler-managed declaration overlays**
   Evidence: there is no existing package transform/cache/metadata/debug pipeline for rewritten declaration overlays.
   Current spike status: the cache subject model, hash inputs, and manifest shape now have a concrete prototype, but writer/reader integration, GC, and debug tooling are still unbuilt.
   Why: this is likely the right long-term internal mechanism for curated fixes that a generic projector cannot express elegantly.
   Main caveat: it is greenfield work and should follow a smaller boundary-projection pilot first.

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

The sharper sequencing is:

1. Lock the narrow contract and the exact Phase 1 scope.
2. Ship the first stable sound bundle for user-authored TypeScript source.
3. Fix real `any` semantics and cache policy.
4. Write a formal boundary-spec document before implementing declaration projection.
5. Pilot declaration-boundary projection.
6. Only then expand into curated overlays and optional stricter surfaces.

Most importantly:

1. Do **not** promise full declaration-boundary quarantine as a near-term patch.
2. Do **not** make internal declaration overlays a prerequisite for the first useful version of Sound Mode.
3. Do **not** treat dead prototype code as evidence that a feature is shipped.

### Dogfooding Order

The intended adoption sequence should stay explicit:

1. **Direct sound-owned diagnostics first**
   - explicit `any` in user-authored TS source
   - `TSZ...` display path
   - `@tsz-unsound` for direct sound diagnostics
   - `soundReportOnly` for direct sound diagnostics
2. **Boundary projection pilot second**
   - narrow declaration-owned return/property/callback surfaces
   - project-reference `.d.ts` fixtures
   - projected fallout can remain ordinary TS2322/TS2345 while attribution is immature
3. **Curated overlays third**
   - `JSON.parse(): unknown`
   - `Response.json(): Promise<unknown>`
   - a few high-value DOM or ecosystem surfaces

This ordering matters because it keeps the first useful release centered on user-authored source, while still giving the team a realistic path toward declaration-boundary ergonomics later.

### Boundary Pilot Enablement

The future declaration-boundary pilot should be enabled **separately** from base `sound`.

Recommended rule:

1. `sound: true` keeps the first stable scope and contract.
2. Boundary projection pilots should require an additional explicit experimental switch such as `soundBoundaryPilot: true` or an equivalent hidden CLI flag.
3. `sound` alone must not quietly begin projecting declaration boundaries once the pilot exists.
4. `soundReportOnly` should keep its direct-diagnostic-first meaning even when the pilot switch is present.

Why this separation matters:

1. It lets teams dogfood the base user-source contract without declaration-boundary churn.
2. It keeps bug reports interpretable because pilot fallout is clearly opt-in.
3. It prevents the meaning of `sound: true` from drifting during the rollout.
4. It matches the broader flat `sound*` family direction without forcing the pilot into the default bundle too early.

### Early `reportOnly` Expectations

Before boundary projection is mature, teams should expect `soundReportOnly` to be intentionally narrow:

1. It is primarily a migration aid for **direct sound-owned diagnostics** in user-authored source.
2. It is **not** a promise that declaration-boundary fallout will be softened or reclassified early.
3. If a boundary pilot is enabled, projected declaration fallout may still appear as ordinary TS2322 / TS2345 errors until attribution and policy are ready.
4. That means the recommended first dogfooding path is:
   - start with `sound: true` + `soundReportOnly: true`
   - clean up direct user-authored sound debt first
   - add boundary pilots only when the team is ready for sharper declaration friction

This should be communicated clearly so teams do not mistake `reportOnly` for a full “ecosystem compatibility mode.”

### First Shippable Slice

The first credible release should have hard acceptance criteria:

1. `--sound` works in CLI and the server / editor path.
2. The documented scope is exactly user-authored `.ts` / `.tsx` / `.mts` / `.cts` implementation code.
3. `.d.ts` files and JS/JSDoc stay out of the first stable scope by default.
4. `sound` implies `useUnknownInCatchVariables`, `exactOptionalPropertyTypes`, and `noUncheckedIndexedAccess`.
5. Explicit `any` is banned in sound-scoped user source.
6. Method bivariance is disabled in sound-scoped assignability paths.
7. The checker emits dedicated TSZ sound diagnostics.
8. `@tsz-unsound` is code-aware, requires a reason, and stale suppressions error.
9. Sticky freshness is no longer part of the core sound contract; either gate it behind pedantic behavior or stop advertising it as core.
10. Array covariance is either wired or explicitly not promised by the MVP.
11. Declaration-boundary quarantine is explicitly documented as later work, not implied by the MVP contract.
12. The feature ships with before/after fixtures and non-sound-mode parity coverage.

Why this slice is right:

1. It is already materially useful for application teams
2. It avoids turning npm and JSDoc cleanup into a prerequisite
3. It keeps project-reference adoption realistic for monorepos that already rely on emitted declarations
4. It gives the team real adoption feedback before we commit to harder boundary architecture
5. It keeps the public story honest: "any-less TS user code first" is a clearer promise than "entire ecosystem soundness soon"

### Decision Gates

These are the remaining product choices worth locking deliberately before implementation starts:

1. **Sound-implied safety flags**
   Recommendation: in the first release, treat `useUnknownInCatchVariables`, `exactOptionalPropertyTypes`, and `noUncheckedIndexedAccess` as effective **hard-on semantics** of `sound: true`, not user-overridable defaults. This keeps the product contract crisp while the feature is still settling.

2. **Config / `tsc` coexistence**
   Recommendation: do not finalize a public tsconfig surface until the project chooses an explicit coexistence story with vanilla `tsc`. If the config ends up living in a shared compiler-options-style surface, keep the flat `sound*` family and avoid a competing nested-object form.

3. **JS/JSDoc scope**
   Recommendation: keep JS/JSDoc out of the first release. Revisit only after TS-source sound mode is working and adoption feedback is real.

4. **Auditable semantic escape hatch**
   Recommendation: keep `@tsz-unsound` diagnostic-only, but add a separate tracked semantic escape primitive before calling the feature user-facing. Comments alone are not enough for intentional unsafe boundaries.

5. **`reportOnly` behavior**
   Recommendation: downgrade sound diagnostics at the final reporting boundary rather than returning success with "Found N errors". This matches the current reporter architecture better and gives a cleaner user experience.

6. **`@tsz-unsound` availability**
   Recommendation: allow it only in user-authored non-declaration source at first. Do not design the first version around editing vendor `.d.ts` files.

7. **`SoundLawyer` dead code**
   Recommendation: do not leave it in semantic limbo. Either wire it, rename it as an explicit prototype, or delete it. Dead code that looks authoritative is worse than no code.

8. **Sound core-lib packaging**
   Recommendation: start by reusing the existing lib-replacement path, even if it needs one small selector extension, instead of designing a brand-new package/discovery system up front.

### Phase 0: Lock the Product Semantics

Goal: make the contract unambiguous before adding more checks.

Deliverables:

1. Document that Sound Mode's primary scope is user-authored non-declaration source files
2. Document that declaration files are trust boundaries by default
3. Decide the exact config surface and `tsc` coexistence story
4. Decide whether explicit `any`, boundary `any`, and unsafe boundary operations share a family or split into distinct codes
5. Decide explicitly that the first shipped scope is TS source, not JS/JSDoc
6. Decide the tracked unsafe semantic escape primitive

Exit criteria:

1. Docs, website copy, and CLI help all describe the same scope model
2. There is a single source of truth for defaults
3. The docs do not imply JS/JSDoc coverage that the first implementation will not enforce
4. The docs do not promise declaration quarantine before the relevant implementation exists

### Phase 1: Ship the First Stable Sound Bundle

Goal: ship the narrow MVP rather than a grab bag of half-wired checks.

Implementation sketch:

1. Make `sound_mode` imply the effective behavior of `useUnknownInCatchVariables`
2. Make `sound_mode` imply the effective behavior of `exactOptionalPropertyTypes`
3. Make `sound_mode` imply the effective behavior of `noUncheckedIndexedAccess`
4. Ban explicit `any` in sound-scoped user source
5. Keep method bivariance tightening as an explicit part of the documented core bundle
6. Route the MVP through dedicated TSZ diagnostics
7. Add code-aware `@tsz-unsound` with required reasons and stale-directive checking
8. Expose the feature through both CLI and server/editor paths
9. Remove sticky freshness from the core promise unless it is explicitly behind pedantic behavior
10. Keep the behavior scoped to TypeScript source semantics; do not accidentally expand JS/JSDoc enforcement in the same patch

Primary touchpoints:

1. `crates/tsz-core/src/config.rs`
2. `crates/tsz-cli/src/driver/core.rs`
3. `crates/tsz-cli/src/driver/check.rs`
4. `crates/tsz-solver/src/evaluation/evaluate.rs`
5. `crates/tsz-solver/src/relations/subtype/rules/objects.rs`
6. `crates/tsz-checker/src/types/type_node.rs`
7. `crates/tsz-cli/src/driver/check_utils.rs`
8. `crates/tsz-cli/src/bin/tsz_server/check.rs`

Tests:

1. Catch variables are treated as `unknown` under sound mode
2. Optional property assignability follows exact-optional semantics under sound mode
3. Indexed access returns `T | undefined` under sound mode
4. Explicit `any` errors in sound-scoped TS source
5. Dedicated TSZ diagnostics and code-aware suppressions behave correctly
6. Existing non-sound behavior remains unchanged
7. JS/checkJs behavior does not regress unintentionally
8. Server/editor paths do not silently drop sound mode

Exit criteria:

1. The first stable contract is fully represented by live behavior
2. The docs stop over-promising on checks that are not actually part of the MVP

### Phase 2: Remove `any` Bottom-Type Behavior from Sound User Code

Goal: make the runtime safety story match the narrowed contract rather than just nested-structure checks.

Implementation sketch:

1. Replace the current `TopLevelOnly` behavior with a true sound-mode `any` policy for user code
2. Either wire `SoundLawyer` into the assignability path or port its semantics into the unified relation policy
3. Ensure top-level `any -> T` no longer succeeds in sound user code except for `T = any | unknown`
4. Keep an explicit legacy path for non-sound mode
5. Remove or make explicit the accidental coupling between `strict_any_propagation` and `FLAG_STRICT_FUNCTION_TYPES`
6. Decide whether dead `SoundLawyer` code is being promoted into product behavior, renamed as prototype code, or deleted

Primary touchpoints:

1. `crates/tsz-solver/src/relations/relation_queries.rs`
2. `crates/tsz-solver/src/relations/subtype/helpers.rs`
3. `crates/tsz-solver/src/types.rs`
4. `crates/tsz-solver/src/sound.rs`
5. `crates/tsz-solver/src/caches/query_cache.rs`

Tests:

1. `const x: any = ...; const y: number = x;` fails in sound user code
2. Function returns and top-level assignments behave consistently with nested cases
3. Cache correctness tests cover sound/non-sound toggles
4. Identity / redeclaration checks still behave correctly when strict `any` is active
5. Non-sound mode remains unchanged

Exit criteria:

1. Top-level `any` no longer silently reintroduces unsoundness into sound-scoped user code
2. The checker and docs agree on the narrowed `any` semantics

### Phase 3: Formalize Boundary Design Before Implementing It

Goal: stop boundary projection from being a slogan and turn it into an implementable design.

Implementation sketch:

1. Write a separate boundary-spec document before coding the projector
2. Define traversal rules for overloads, generics, mapped / conditional / indexed-access types, higher-order callbacks, aliases, merges, augmentations, and project references
3. Define the projected cache key precisely enough to include observed symbol/type identity, polarity, and instantiation context
4. Define what is and is not diagnostic at the boundary
5. Decide how the tracked unsafe semantic escape interacts with boundary values

Tests:

1. The spec has executable fixtures attached to every rule
2. Ambiguous cases are resolved in writing before implementation starts

Exit criteria:

1. Boundary projection is no longer a hand-wavy direction; it is a precise design

### Phase 4: Pilot Declaration-Boundary Projection

Goal: prove the boundary design on a narrow real surface before committing to a broader ecosystem story.

Implementation sketch:

1. Add declaration-owned symbol detection using existing symbol→file ownership plus declaration-file metadata
2. Prototype a `DeclarationBoundaryProjector` on a narrow set of surfaces: imported symbol types, readable properties, return types, and callback parameters supplied from libraries into user code
3. Keep projected types in a dedicated cache instead of overwriting base symbol/type caches
4. Build dedicated project-reference fixtures: referenced project emits `.d.ts` with `any`; sound consumer sees the projected view
5. Keep declaration quarantine off the public contract until the pilot is proven

Primary touchpoints:

1. `crates/tsz-checker/src/context/core.rs`
2. `crates/tsz-checker/src/state/type_analysis/core.rs`
3. `crates/tsz-checker/src/context/compiler_options.rs`
4. projected boundary cache / transform plumbing

Tests:

1. Normal `.d.ts` files work with sound user code on a narrow pilot surface
2. Project-reference `.d.ts` outputs work with sound downstream consumers without upstream sound adoption
3. Non-sound projects remain unaffected

Exit criteria:

1. The team has one concrete, working quarantine strategy beyond prose

### Phase 5: Curated Overlays, Core Libs, and Optional Surfaces

Goal: expand beyond the MVP and boundary pilot without pretending these are Phase 1 requirements.

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

Overlay/cache infrastructure rules for this phase:

1. Reuse one shared object store for package overlays and referenced-project emitted declaration outputs.
2. Distinguish them with tagged `subject` identity and subject-scoped locks, not separate cache implementations.
3. Treat exact hash inputs, atomic publish, and GC rules as part of the phase contract rather than leaving them to implementation cleanup later.
4. Keep overlay cache infrastructure explicitly post-MVP; it should unblock ecosystem adoption work, not hold the first stable sound slice hostage.
5. Keep the product rule simple: projection is the default boundary mechanism; curated overlays are for small, high-value surfaces where projection is not the clearest implementation.

Exit criteria:

1. Common trust-boundary APIs stop producing `any` before user code even sees them
2. Overlay cache identity and publish rules are concrete enough that implementation work no longer has to invent product semantics mid-flight

Follow-ons in this phase:

1. config / tsconfig / `reportOnly` surface once coexistence is settled
2. sound core-lib pilots such as `JSON.parse(): unknown`
3. curated internal overlays where projection is not expressive enough
4. stricter optional surfaces like `soundCheckDeclarations`
5. later strictness candidates such as switch exhaustiveness, primitive boxing, non-empty reductions, or `Exact<T>`

### Verification Plan

Every phase should ship with:

1. unit tests for checker and solver behavior
2. integration tests covering `.ts` vs `.d.ts` boundaries
3. fixtures with representative third-party declaration patterns
4. project-reference fixtures where one composite project emits `.d.ts` and another sound project consumes it
5. website/doc updates
6. conformance snapshots to prove non-sound mode parity is unaffected
7. `cargo run --quiet --bin audit-unsoundness -- --summary` recorded only as an inventory / regression signal for compat behavior, not as evidence that Sound Mode is close to done

### Explicit Non-Goals for the First Useful Version

1. Full npm ecosystem purification is **not** required before Sound Mode is useful
2. General declaration-origin provenance tracking in the solver is **not** required for the first shipping slice
3. Renaming every internal `TS900x` placeholder in one shot is **not** required before the public TSZ family taxonomy is stable
4. Upstream referenced composite projects do **not** need to enable Sound Mode before downstream consumers can benefit

---

## Implementation Considerations

### The Caching "Correctness Tax"

Caching is the hidden complexity of gradual soundness. Because Sound Mode is a policy switch over the same Judge solver, any feature that makes relation outcomes depend on sound policy (for example, `soundArrayVariance` or `any` behavior within the active sound profile) must be reflected in the cache key.
- **Policy Bitsets:** The solver's relation cache keys must include a versioned policy ID or bitset. This ensures that a `Dog[]` to `Animal[]` relation check caches as `false` in Sound Mode, but doesn't pollute the cache for a non-sound file that expects `true`.
- **Boundary Projection Caches:** projected declaration views should live in a cache separate from the base symbol/type caches. File identity/hash is only part of the invalidation story; the actual cache key will also need to reflect the observed symbol/type, polarity, and relevant instantiation context. This matters especially for project references, where downstream rebuild decisions already track the freshness of emitted `.d.ts` files.
- **Overlay Object Cache:** the persistent overlay object store should stay separate from projected boundary caches. They may share low-level freshness inputs such as declaration-closure hashes, but they should not share one invalidation table or one storage model because they serve different layers of the system.

> **Current state:** `RelationCacheKey` in `types.rs:259` already carries more sound-relevant state than an earlier version of this doc claimed: method-bivariance disablement is represented in packed relation flags and `any_mode` differentiates `any` propagation modes. The remaining cache risk is more specific: `RelationPolicy::from_flags()` still derives strict-`any` behavior from `FLAG_STRICT_FUNCTION_TYPES`, and some query-cache helper paths still construct keys with `any_mode = 0`. The right next step is a cache-entrypoint audit, not a vague "add one sound bit" claim.

### Compatibility & Performance

Sound Mode rejects some valid TypeScript code. Migration requires running in "report only" mode and manually fixing code structures. Most sound checks are `O(1)` additions to existing checks. The main cost is not raw relation complexity; it is **policy plumbing, caching correctness, and boundary transformation infrastructure**.

Performance strategy:

1. Keep the base mode cheap by banning explicit `any` in user code before introducing heavier boundary machinery
2. Solve ecosystem adoption first with projected sound views, then with small cached internal overlays where projection is not enough
3. Cache projected types and overlays separately and make both deterministic so repeated builds stay fast
4. Make sure projected-type invalidation lines up with referenced-project `.d.ts` invalidation so composite builds do not pay avoidable recomputation costs

---

## Trade-offs Summary

| Aspect | TypeScript (tsc) | First Stable Sound Bundle | Later / Research |
|--------|------------------|---------------------------|------------------|
| Method params | Bivariant | Contravariant in sound-scoped checks | — |
| Explicit `any` in user TS | Allowed | Rejected | — |
| Catch variables | Config-dependent | Strict `unknown` | — |
| Index access | `T` | `T \| undefined` | — |
| Optional properties | Missing ≈ undefined | Exact distinction | — |
| Declaration boundaries | Ordinary `.d.ts` as-is | Trust boundaries, but no general quarantine promised yet | Projected sound views / curated overlays |
| Sticky freshness | Currently live under `--sound` | Not part of the core promise | Pedantic |
| Mutable array covariance | tsc-compatible | Not promised until actually wired | Candidate later core tightening |
| Unsafe assertions / non-null / definite assignment | Allowed | Not part of MVP contract yet | Auditable escape-hatch design |
| JS/JSDoc | Supported by TS modes | Out of first stable scope | Later evaluation |

**The fundamental trade-off:** Sound Mode catches more bugs but requires more explicit boundaries. The crucial adoption choice is to spend that explicitness budget in **user code where it buys safety**, while using compiler-managed declaration boundary projection and internal overlays to avoid making the ecosystem somebody's manual cleanup project.

---

## References

- [TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md) - Complete list of TypeScript's intentional unsoundness
- [NORTH_STAR.md](../architecture/NORTH_STAR.md) - Judge/Lawyer architecture documentation
- [TypeScript Design Goals](https://github.com/Microsoft/TypeScript/wiki/TypeScript-Design-Goals) - Why TypeScript chose pragmatism
- [TypeScript Soundness Documentation](https://www.typescriptlang.org/docs/handbook/type-compatibility.html#a-note-on-soundness) - TS's own soundness discussion
