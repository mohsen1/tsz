# Sound Mode

<div class="alert alert-warning">
  <strong>Exploring Phase</strong> - Sound mode is an active area of research. Diagnostic codes, behavior, and coverage are subject to change. We're sharing it early to gather feedback.
</div>

**tsz** offers an opt-in **sound mode** — a stricter type-checking mode that catches real bugs TypeScript allows by design.

## The Sound Mode Contract

TypeScript explicitly lists "apply a sound or provably correct type system" as a **non-goal**. Sound Mode makes four guarantees:

1. **Within user-authored Sound Mode code**, tsz rejects the known TypeScript unsound patterns that can cause type-driven runtime exceptions — calling methods that aren't there, indexing missing keys, writing through an alias with the wrong element type.

2. **User code is `any`-less by default**: developers use `unknown`, narrowing, validators, or explicit suppressions instead of writing `any` in normal source files.

3. **Declaration files are treated as trust boundaries, not adoption blockers**: third-party `.d.ts` files may still contain legacy unsoundness, but those types are quarantined before they silently poison sound user code.

4. **Any use of unsoundness must be explicit and auditable** (`// @tsz-unsound TSZNNNN: reason`) and trackable in CI as technical debt.

## Quick Start

```bash
# CLI flag (current implementation)
tsz check --sound src/

# Planned tsconfig shape (not wired yet)
{
  "compilerOptions": {
    "sound": true
  }
}

# Planned staged-rollout shape
{
  "compilerOptions": {
    "sound": true,
    "soundReportOnly": true
  }
}

# Per-file pragma (planned - not yet parsed from source)
# // @tsz-sound
```

Or try it in the [Playground](/playground/) — select the **Sound Mode** example and check the **sound** checkbox.

## Two Layers: Core vs Pedantic (Planned)

The target design separates Sound Mode into two layers. Currently, `sound: true` is a single boolean that enables all available checks. The two-layer split and flat `sound*` companion flags are planned but not yet implemented.

The intended config rule is simple:

- `sound: true` means "turn on the default sound profile"
- `soundPedantic: true` turns on the extra bug-finding heuristics
- `soundReportOnly: true` keeps Sound Mode visible without failing the run
- future targeted knobs should follow the same flat family, such as `soundArrayVariance`
- we do **not** plan to use a nested `sound: { ... }` object as the public config shape

- **`sound: true`** — Core runtime-safety checks. Every check in this layer catches patterns where "tsc allows it; this can crash."
- **`soundPedantic: true`** (planned) — Will add bug-finding heuristics (like sticky freshness) that catch likely mistakes but aren't strictly about runtime crashes.

This prevents Sound Mode from feeling like "it hates JavaScript patterns" while keeping the strong guarantees for runtime safety.

## Why Sound Mode?

TypeScript is **intentionally unsound**. The TypeScript team prioritizes developer ergonomics over type-theoretic correctness. This means there are programs that type-check successfully but crash at runtime.

## What Sound Mode Catches

When sound mode is enabled, tsz applies stricter rules that close TypeScript's known escape hatches. Public diagnostics are grouped into topic families rather than a single 9000-series block: `TSZ1000` for trust boundaries and escape hatches, `TSZ2000` for variance, `TSZ3000` for object shapes and mutation, and so on. Today, sound mode still emits standard TypeScript diagnostic codes (TS2322, TS2345, etc.); the family-based TSZNNNN surface is the planned user-facing contract.

### TSZ2001: Covariant Mutable Arrays

Assigning `Dog[]` to `Animal[]` is safe for reads but dangerous for writes. TypeScript allows it; sound mode rejects it for mutable arrays.

```typescript
interface Animal { name: string }
interface Dog extends Animal { breed: string }
interface Cat extends Animal { indoor: boolean }

const dogs: Dog[] = [{ name: "Rex", breed: "Lab" }];

// tsc allows this - but it's unsafe!
const animals: Animal[] = dogs;

// Now we can push a Cat into what's really a Dog[]
animals.push({ name: "Whiskers", indoor: true });

// 💥 Runtime: dogs[1] has no 'breed' property
console.log(dogs[1].breed);
```

Sound mode: only `readonly Dog[]` → `readonly Animal[]` is safe.

**How to fix:**
- Accept `readonly Animal[]` in APIs that only read
- Return `readonly` from producer functions
- Break aliasing when mutability is needed: `const animals: Animal[] = [...dogs];`

### TSZ2002: Method Parameter Bivariance

TypeScript allows method parameters to be checked bivariantly — both covariant and contravariant. Even `strictFunctionTypes` intentionally does not apply to method syntax. This lets subclasses narrow parameter types unsafely.

```typescript
class Animal {
  feed(food: string | number) {}
}

class Dog extends Animal {
  // tsc allows narrowing the parameter - unsound!
  feed(food: string) {
    console.log(food.toUpperCase());
  }
}

const animal: Animal = new Dog();
// 💥 Runtime: Dog.feed receives a number, calls toUpperCase() on it
animal.feed(42);
```

Sound mode: method parameters must be contravariant. A subclass method must accept *at least* what the superclass accepts.

**How to fix:**
- Widen the subclass parameter to match the parent
- Use function-property syntax: `feed: (food: Food) => void` instead of `feed(food: Food): void`
- Use overloads for event-map patterns

### TSZ1001: `any` Escape Detection

`any` is the biggest source of unsoundness. It acts as both a top and a bottom type, silently bypassing all structural checks. Sound mode treats `any` as a **taint** — it may exist, but it cannot flow into structured types without an explicit boundary step.

```typescript
function processUser(user: { name: string; age: number }) {
  console.log(user.name.toUpperCase());
  console.log(user.age.toFixed(2));
}

// 'any' lets anything through - no structural check at all
const data: any = "this is not a user object";
processUser(data); // tsc: no error!
// 💥 Runtime: Cannot read properties of undefined (reading 'toUpperCase')
```

Sound mode: `any` is only assignable to `any` and `unknown`. Use `unknown` with proper narrowing instead:

```typescript
const data: unknown = JSON.parse(input);

// Must narrow before use
if (typeof data === "object" && data !== null
    && "name" in data && "age" in data) {
  processUser(data as { name: string; age: number });
}
```

### TSZ1011: Unsafe Type Assertions (Planned)

Type assertions (`as`) let developers punch holes through the type system. Sound mode will restrict assertions to **widening only** (going from specific to general). Narrowing casts and sideways casts will be rejected.

```typescript
interface Cat { meow(): void }
interface Dog { bark(): void }

const cat: Cat = { meow() { console.log("meow") } };

// ❌ Sideways cast - rejected even through unknown
const dog = cat as unknown as Dog;
dog.bark(); // 💥 Runtime: dog.bark is not a function

// ✅ Widening assertion is fine
const x: "hello" = "hello";
const y = x as string;  // OK: "hello" is assignable to string
```

**How to fix:** Use `instanceof`, `in`, discriminant checks, `asserts` functions, or schema parsers (Zod, ArkType) instead of type assertions.

### TSZ3006: Sticky Freshness

TypeScript checks excess properties on direct object literal assignment, but the check is bypassed through variable indirection. Sound mode preserves "freshness" so excess properties are always caught.

**Note:** This check is currently active under `sound: true`. In the planned two-layer design, it will move to the **pedantic** layer (`soundPedantic: true`) because extra properties don't cause runtime crashes -- it's a typo-catching heuristic.

```typescript
interface Point2D { x: number; y: number }

// Direct assignment - tsc catches the excess property
const p1: Point2D = { x: 1, y: 2, z: 3 }; // Error: 'z' does not exist

// Indirect assignment - tsc allows it!
const point3d = { x: 1, y: 2, z: 3 };
const p2: Point2D = point3d; // tsc: no error. Sound mode (pedantic): error!

// Same bypass in function arguments
function distance(a: Point2D, b: Point2D): number {
  return Math.sqrt((a.x - b.x) ** 2 + (a.y - b.y) ** 2);
}
const origin = { x: 0, y: 0, label: "origin" };
distance(origin, point3d); // tsc: no error. Sound mode (pedantic): error!
```

### TSZ5001: Unchecked Indexed Access (Planned)

Accessing an object or array by index may return `undefined` if the key doesn't exist. TypeScript assumes the value is always present. Sound mode will match TypeScript's `noUncheckedIndexedAccess` semantics -- this behavior will be implied even if the tsconfig flag isn't set. (Currently, `noUncheckedIndexedAccess` must be enabled separately.)

```typescript
const scores: { [name: string]: number } = {
  alice: 95,
  bob: 87,
};

// tsc says this is 'number', but it's actually undefined
const charlieScore: number = scores["charlie"];
console.log(charlieScore.toFixed(2)); // 💥 Runtime: Cannot read properties of undefined

// Same issue with arrays
const items: string[] = ["a", "b", "c"];
const tenth: string = items[10]; // undefined at runtime
console.log(tenth.toUpperCase()); // 💥 Runtime error
```

Sound mode: indexed access returns `T | undefined`, forcing a check before use.

### TSZ6001: Enum-Number Assignment (Planned)

TypeScript enums are freely assignable to and from `number`, which can lead to invalid enum values.

```typescript
enum Status {
  Active = 0,
  Inactive = 1,
  Suspended = 2,
}

// tsc allows any number - even invalid ones
const status: Status = 999; // No error in tsc!

function handleStatus(s: Status) {
  switch (s) {
    case Status.Active: return "active";
    case Status.Inactive: return "inactive";
    case Status.Suspended: return "suspended";
    // 💥 999 falls through - no exhaustive check catches it
  }
}
```

Sound mode: enum values cannot be assigned to/from `number` without explicit conversion.

**Canonical escape patterns:**
```typescript
const status = parseEnum(Status, userInput);  // Status | undefined
if (status !== undefined) {
  handleStatus(status);  // Safe!
}
```

### Additional Core Checks (Planned)

Sound mode will also address these soundness gaps (infrastructure hooks exist for all four -- see plan doc for feasibility notes):

| Diagnostic | Description |
|-----------|-------------|
| TSZ1021 | **Non-null assertions** (`!`) flagged as unsound escape hatches |
| TSZ1022 | **Definite assignment** (`!:`) flagged on class fields |
| TSZ1031 | **Catch variables** always `unknown` (implied `useUnknownInCatchVariables`) |
| TSZ3008 | **Exact optional properties** — distinguishes "missing" vs "present but `undefined`" |

## Planned: Generics & Type Algebra

Sound mode will also address unsoundness in TypeScript's generic type system.

### Conditional Type Distribution

TypeScript automatically distributes conditional types over unions, which surprises many developers:

```typescript
type IsString<T> = T extends string ? true : false;

type A = IsString<string | number>;
// You might expect: false (the union is not a string)
// Actual result: boolean (true | false) - it distributes!
```

Sound mode: keep TypeScript's behavior but add a diagnostic that flags surprising distributions and suggests the non-distributive pattern: `[T] extends [U] ? ...`

### Impossible Intersections

TypeScript allows intersections that can never be satisfied:

```typescript
type Broken = { [key: string]: number } & { name: string };
// 'name' must be both number (from index sig) and string - impossible!
// TypeScript keeps this type alive instead of reducing to never
```

Sound mode: reduce impossible intersections to `never` immediately.

### Exact Types

TypeScript's structural typing means any object with extra properties satisfies an interface. Sound mode introduces `Exact<T>` to opt into strict shape matching:

```typescript
// Exact<T> is opt-in and non-infectious
function getKeys<T>(obj: Exact<T>): (keyof T)[] {
  return Object.keys(obj) as (keyof T)[];  // Sound!
}

// Exact<T> is assignable to T, but not the reverse without proof
const exact: Exact<User> = { name: "Alice" };  // OK
const open: User = exact;  // OK (exact → open)
const back: Exact<User> = open;  // Error (open → exact needs proof)
```

## Suppression: Auditable Technical Debt (Planned)

The target suppression design is adoptable at scale while keeping sound mode honest. The `@tsz-unsound` comment directive is not yet implemented.

```typescript
// ✅ Valid: targeted, with reason
// @tsz-unsound TSZ1011: validated by legacy runtime guard in foo.ts

// ❌ Invalid: no code specified
// @tsz-unsound: trust me

// ❌ Invalid: no reason
// @tsz-unsound TSZ1011

// ❌ Stale suppression becomes an error
// @tsz-unsound TSZ2001: array is readonly  ← no TSZ2001 here → error!
```

Rules:
- **Targeted** — must name a specific TSZ code
- **Sticky** — if the error disappears, the suppression itself becomes an error
- **Reason required** — enforced formatting with explanation
- **Diagnostic-only** — suppresses output, not semantics (the compiler still checks soundly)

## Diagnostics CLI (Planned)

```bash
# Explain any sound mode diagnostic (planned)
tsz explain TSZ2002

# Sound mode summary for a project (planned)
tsz check --sound --sound-summary src/
```

## Making Sound Mode Practical

Catching unsoundness is only useful if you can actually enable it without drowning in false positives from third-party code. We're building several mechanisms to make sound mode practical for real codebases.

### SoundlyTyped: Automated Ecosystem Soundness (Planned)

The biggest obstacle to sound typing is the ecosystem. Libraries ship `.d.ts` files full of `any`, bivariant methods, and open enums. **SoundlyTyped** is a planned fully automated transformation pipeline that will rewrite upstream type definitions:

- Maps `any` to `unknown` at module boundaries
- Closes open numeric enums
- Patches bivariant method signatures to use contravariant parameters (via method-to-property conversion)
- Regenerates automatically when upstream packages update

No hand-coded type patches — purely mechanical transformations applied to `node_modules` and DefinitelyTyped.

**Auditability:** SoundlyTyped outputs go in a deterministic cache directory (`.tsz/soundlytyped/<pkg>@<version>/`) with a lockfile recording upstream version, transform version, and output hashes. Run `tsz soundlytyped diff <pkg>` for a human-readable summary of what changed.

### Sound Core Libraries (Planned)

Standard library types like `JSON.parse()`, `fetch().json()`, and `document.getElementById()` return `any` for ergonomics. Sound mode will ship alternative core typings (`*.sound.lib.d.ts`) where these return `unknown` instead, forcing explicit narrowing at every external data boundary.

```typescript
// Standard lib: JSON.parse returns 'any'
const config = JSON.parse(text); // any - no checks needed
startServer(config); // silently passes

// Sound lib: JSON.parse returns 'unknown'
const config = JSON.parse(text); // unknown - must narrow
if (isConfig(config)) {
  startServer(config); // safe!
}
```

### Runtime Validation Integration

Sound mode pairs naturally with runtime validation libraries. When data crosses a trust boundary — user input, API responses, file reads — you need runtime checks regardless of your type system. Sound mode makes this explicit:

```typescript
import { z } from "zod";

const UserSchema = z.object({
  name: z.string(),
  age: z.number(),
});

// Sound mode encourages this pattern at every boundary
const user = UserSchema.parse(await response.json());
// 'user' is now safely typed as { name: string; age: number }
```

### Gradual Adoption

Sound mode supports gradual adoption so you don't have to convert your entire codebase at once:

- **Per-file pragma** (`// @tsz-sound`) — enable sound mode one file at a time (planned -- not yet parsed)
- **Per-directory** — use `tsconfig.json` extends to enable sound mode in specific directories
- **Suppression comments** — `// @tsz-unsound TSZNNNN: reason` to acknowledge and suppress specific diagnostics during migration (planned)
- **Sound summary** — `tsz check --sound --sound-summary` to track progress by diagnostic code and file (planned)
