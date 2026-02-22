# Sound Mode

**Status**: ✅ IMPLEMENTED (Basic)
**Last Updated**: January 2026

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

# Or per-file pragma
// @ts-sound
```

## Implemented Features

| Feature | Status | Diagnostic |
|---------|--------|------------|
| Sticky freshness | ✅ | TSZ0001 |
| Mutable array covariance | ✅ | TSZ0002 |
| Method bivariance | ✅ | TSZ0003 |
| `any` escape detection | ✅ | TSZ0004 |
| Enum-number assignment | ✅ | TSZ0005 |
| Missing index signature | ✅ | TSZ0006 |
| Unsafe type assertion | ✅ | TSZ0007 |
| Unchecked indexed access | ✅ | TSZ0008 |

See `src/solver/sound.rs` for implementation details.

---

---

## Executive Summary

TypeScript is **intentionally unsound**. The TypeScript team made deliberate design choices to prioritize developer ergonomics over type-theoretic correctness. These choices are documented in [docs/specs/TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md).

tsz's **Judge/Lawyer architecture** separates concerns:
- **Judge (Core Solver)**: Implements strict, sound set-theory semantics
- **Lawyer (Compatibility Layer)**: Applies TypeScript-specific rules to match tsc behavior

This architecture enables a potential **Sound Mode** - an opt-in flag that bypasses the Lawyer layer, exposing the Judge's stricter checking. Sound Mode would catch real bugs that TypeScript allows by design, at the cost of rejecting some valid TypeScript patterns.

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

Sound Mode would reject the problematic assignment, catching the bug at compile time.

**Use cases for Sound Mode:**
- Safety-critical code (financial, medical, infrastructure)
- Library authors wanting stronger guarantees
- Teams willing to trade ergonomics for correctness
- Gradual migration to stricter typing

---

## What Sound Mode Would Catch

### Category 1: Variance Violations

#### 1.1 Covariant Mutable Arrays

**TypeScript allows:**
```typescript
const dogs: Dog[] = [new Dog()];
const animals: Animal[] = dogs;  // ✅ tsc
animals.push(new Cat());         // 💥 Runtime: Cat in Dog[]
```

**Sound Mode:** Reject `Dog[]` → `Animal[]` for mutable arrays. Only `readonly Dog[]` → `readonly Animal[]` is safe.

#### 1.2 Method Parameter Bivariance

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

#### 1.3 Covariant `this` Type

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

### Category 2: The `any` Epidemic and Ecosystem Integration

`any` is the biggest source of unsoundness in TypeScript. It infects anything it touches by acting as both a top and a bottom type.

#### 2.1 Developer Strategies for Sound Boundaries

When developers consume external code that isn't inherently sound, they must build an explicit "Soundness Wall" to prevent `any` and other unsound types from leaking into their codebase. This is achieved through:

1.  **Runtime Validation (The Gold Standard):** Whenever external data crosses the boundary (e.g., API responses, loosely typed callbacks), developers should use schema validation libraries like Zod or ArkType. This converts `unknown` or `any` into a structurally validated, sound type at runtime.
2.  **Explicit Module Augmentation (Shadowing):** If an external library's `.d.ts` file is unsound but the behavior is known, developers can use declaration merging to override the specific exports with strict types:
    ```typescript
    // sound-overrides.d.ts
    declare module "unsafe-lib" {
        export function processData(payload: unknown): void; // Overriding 'any'
    }
    ```
3.  **Strict Adapter Layers:** Wrap external libraries in internal functions or classes that strictly define the inputs and outputs, ensuring the rest of the application only interacts with the sound adapter, not the raw external API.
4.  **Deep Type Transformation (`ReplaceAnyDeep`):** For complex nested structures imported from third-party libraries, developers can utilize a generic utility type (like `ReplaceAnyDeep<T, unknown>`) to statically map all instances of `any` to `unknown` at the point of ingestion, effectively quarantining the type before it spreads.

#### 2.2 Sound Core Libraries (`*.sound.lib.d.ts`)

Standard libraries often return `any` for ergonomics. A true Sound Mode defaults to sound alternative core typings (e.g., `dom.sound.lib.d.ts`, `es2022.sound.lib.d.ts`).

**TypeScript allows:**
```typescript
const data = JSON.parse('{"a": 1}');
const val: number = data.b.c.d; // ✅ tsc allows, crashes at runtime
```

**Sound Mode (`sound.lib.d.ts`):**
In the sound core libs:
- `JSON.parse` returns `unknown`
- `Response.json()` (from `fetch`) returns `Promise<unknown>`
Developers must explicitly validate the data (e.g., using Zod or `typeof` checks) before using it.

#### 2.3 `SoundlyTyped`: Automated Ecosystem Soundness

To handle the massive ecosystem of `node_modules` and DefinitelyTyped (DT), we will introduce **`SoundlyTyped`**—a fully automated infrastructure layer that sits on top of npm package types.

**The `SoundlyTyped` Architecture:**
- **Zero Hand-Coded Types:** Unlike DT, `SoundlyTyped` is purely mechanical. It does not accept manual PRs for type definitions.
- **Automated Transformation:** A pipeline ingests upstream `.d.ts` files (from packages or DT) and outputs sound equivalents. It systematically rewrites `any` to `unknown`, closes open numeric enums, patches bivariant method signatures to contravariant properties, and enforces strict array bounds.
- **Evergreen Sync:** Whenever an upstream package updates, `SoundlyTyped` automatically regenerates and publishes the sound definitions.

This guarantees an explicit, mathematically sound boundary without requiring the core compiler to guess, silently mangle types, or slow down during resolution.

#### 2.4 Ban Explicit `any` (and `any` as Top/Bottom)

In Sound Mode, developers are banned from writing explicit `any`. They must use `unknown`. If an `any` type still somehow bypasses the `SoundlyTyped` infrastructure, the compiler strictly treats it as a top type equivalent to `unknown`. It cannot be assigned to more specific types without an explicit cast.

### Category 3: Unchecked Access

#### 3.1 Index Access Without Undefined

**TypeScript allows:**
```typescript
const arr: number[] = [];
const x = arr[100];       // ✅ tsc: x is number
x.toFixed();              // 💥 Runtime: undefined.toFixed()
```

**Sound Mode:** Type index access as `T | undefined` by default.

#### 3.2 Rest Parameter Bivariance

**TypeScript allows:**
```typescript
type Logger = (...args: any[]) => void;
const fn = (id: number, name: string) => {};
const logger: Logger = fn;  // ✅ tsc
logger("wrong", "types");   // 💥 Runtime: fn expects number, string
```

**Sound Mode:** Reject - `(...args: any[])` is NOT a supertype of specific signatures.

### Category 4: Structural vs Nominal Confusion

#### 4.1 Numeric Enum Openness

**TypeScript allows:**
```typescript
enum Status { Active = 0, Inactive = 1 }
const s: Status = 999;  // ✅ tsc allows any number!
```

**Sound Mode:** Make numeric enums closed - only defined values allowed.

#### 4.2 Weak Type Acceptance

**TypeScript allows (with warning):**
```typescript
interface Config { port?: number; host?: string; }
const opts = { timeout: 5000 };  // No overlap with Config
const config: Config = opts;     // ⚠️ tsc warns but allows
```

**Sound Mode:** Strictly reject objects with no overlapping properties.

### Category 5: Return Type Leniency

#### 5.1 Void Return Exception

**TypeScript allows:**
```typescript
type Callback = () => void;
const cb: Callback = () => "hello";  // ✅ tsc allows returning string
```

**Sound Mode:** Require return types to actually match.

### Category 6: Type Algebra Surprises

#### 6.1 Conditional Type Distribution

**TypeScript does:**
```typescript
type IsString<T> = T extends string ? true : false;
type A = IsString<string | number>;  
// Expected by many: false (the union is not a string)
// Actual: boolean (true | false) - distributes!
```

**Sound Mode:** Make distribution opt-in rather than default.

#### 6.2 Intersection Survival

**TypeScript allows:**
```typescript
type Broken = { [key: string]: number } & { name: string };
// Impossible type - name must be both number and string
const x: Broken = ???;  // Can't create, but type exists
```

**Sound Mode:** Reduce impossible intersections to `never` immediately.

### Category 7: Object Assignability

#### 7.1 Split Accessor Assignability

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

### Category 8: Generic Type Safety

#### 8.1 Generic Constraint Confusion

**TypeScript allows:**
```typescript
function f<T extends string, U extends string>(t: T): U {
    return t;  // ✅ tsc allows - both extend string!
}

const result = f<"hello", "world">("hello");
// result type: "world"
// actual value: "hello" 💥
```

Two generics sharing a constraint doesn't mean they're the same type. `T` could be `"hello"` and `U` could be `"world"`.

**Sound Mode:** Two different type parameters should never be subtypes of each other unless they are the same parameter. Constraints should only be used when checking against concrete types.

### Category 9: The `Function` Escape Hatch

#### 9.1 Opaque Function Type

**TypeScript allows:**
```typescript
const f: Function = (x: number) => x * 2;
f("not", "a", "number", 42, true);  // ✅ tsc allows any args!
```

The global `Function` type is an untyped supertype for all callables - effectively `any` for function calls.

**Sound Mode:** Reject assignment of specific function types to `Function`, or require explicit cast. `Function` should not silently erase parameter/return type information.

### Category 10: Primitive Boxing

#### 10.1 Implicit Primitive-to-Object Assignment

**TypeScript allows:**
```typescript
const o: Object = 42;     // ✅ tsc allows
const e: {} = "hello";    // ✅ tsc allows

// But this is rejected (correctly):
const obj: object = 42;   // ❌ tsc rejects
```

Primitives can be assigned to `Object` and `{}` because they have apparent members (via boxing). But this blurs the primitive/object distinction.

**Sound Mode:** Reject primitive-to-`Object`/`{}` assignment. Primitives should only be assignable to their intrinsic types, `unknown`, or their explicit wrapper types (`Number`, `String`, etc.).

### Category 11: Exact Types & Object Iteration

#### 11.1 Structural Subtyping and Iteration

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

Because TypeScript allows excess properties on variables (though not directly on object literals), `Object.keys()` is notoriously unsafe, returning `string[]` instead of `(keyof User)[]`.

**Sound Mode:**
Sound mode could introduce "Exact Types" (a highly requested feature, see TypeScript #12936), either as an opt-in `Exact<T>` or by default, where objects are strictly forbidden from containing extra un-declared properties. This ensures that downcasting or iterating over objects is perfectly sound. If Exact Types are enforced, `Object.keys(user)` can safely return `(keyof User)[]`.

### Category 12: Switch Statement Exhaustiveness

#### 12.1 Implicit Switch Fall-through to `undefined`

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

Unless a function explicitly requires a non-void return, TypeScript does not enforce that a `switch` statement over a union type handles all possible cases.

**Sound Mode:**
Enforce native switch exhaustiveness. If a switch statement operates over a discriminated union or finite literal union, all cases must be handled explicitly (or via a `default` case).

### Category 13: Strict Array/Set Membership

#### 13.1 `includes` and `has` Type Constraints

**TypeScript restricts:**
```typescript
const arr: string[] = ["a", "b"];
arr.includes(1); // ❌ tsc errors: Argument of type 'number' is not assignable to parameter of type 'string'.
```
Ironically, TypeScript is *too* restrictive here in an unsound way. `Array.prototype.includes` accepts `any` value at runtime (returning `false` if it doesn't match). By enforcing `T` for the parameter, TypeScript limits flexibility but doesn't actually prevent runtime crashes.

**Sound Mode:**
Modify core library definitions (or compiler overrides) so that:
- `Array<T>.prototype.includes(searchElement: unknown)`
- `Set<T>.prototype.has(value: unknown)`
This allows perfectly safe membership checks without arbitrary subtyping restrictions.

### Category 14: Non-Empty Array Reduction

#### 14.1 `reduce` Without Initial Value

**TypeScript allows:**
```typescript
const arr: number[] = [];
const sum = arr.reduce((a, b) => a + b); // ✅ tsc allows, 💥 Runtime: TypeError on empty array
```

**Sound Mode:**
If `reduce` is called without an initial value, the array must be proven to be non-empty (e.g., `[T, ...T[]]`), or the return type evaluates to `T | undefined` forcing the developer to handle the potential empty state.

### Category 15: Readonly Property Aliasing

#### 15.1 Aliasing Immutable to Mutable

**TypeScript allows:**
```typescript
interface Immutable { readonly id: number; }
interface Mutable { id: number; }

const ro: Immutable = { id: 1 };
const mut: Mutable = ro;  // ✅ tsc allows dropping 'readonly'
mut.id = 2;               // 💥 Runtime: ro.id is now 2!
```
Because TypeScript checks properties structurally and ignores the `readonly` modifier during assignment compatibility, a readonly reference can be trivially aliased to a mutable one, destroying the immutability guarantee.

**Sound Mode:**
A type with a `readonly` property cannot be assigned to a type where that property is mutable. This enforces that `readonly` guarantees hold true across aliases (a highly requested fix, see TypeScript #13347).

### Category 16: Tuple Length Mutation

#### 16.1 Tuples Inheriting Array Methods

**TypeScript allows:**
```typescript
const tuple: [number, string] = [1, "hello"];
tuple.push(2);      // ✅ tsc allows, tuple is now [1, "hello", 2]
const len = tuple.length; // ✅ tsc says '2', 💥 Runtime: actual length is 3
```
Tuples in TypeScript inherit from `Array.prototype`, meaning methods like `push`, `pop`, `shift`, and `splice` are perfectly valid to call on them, completely desynchronizing the runtime length from the type system's known length.

**Sound Mode:**
Tuple types should either omit length-mutating methods entirely, or those methods should be typed to require `never` as an argument, preventing mutation of a fixed-length structure (see TypeScript #3336, #32063).

### Category 17: Implicit `any` in Error Handling

#### 17.1 Unenforced Catch Variables

**TypeScript allows (historically):**
```typescript
try { throw { message: "oops" }; } 
catch (e) { 
    e.toUpperCase(); // ✅ tsc allows if useUnknownInCatchVariables is false
}
```
While modern TypeScript introduced `useUnknownInCatchVariables`, many codebases still have it disabled or use older `strict` configurations where `e` is `any`.

**Sound Mode:**
Strictly enforce `unknown` for catch variables. Developers *must* type-narrow or cast `e` before interacting with it. Furthermore, Sound Mode could introduce a strictly enforced `@throws` or explicit `throws ErrorType` syntax for function signatures to allow safely typing the catch block.

### Category 18: Unsafe Object Mutation

#### 18.1 `Object.assign` Target Poisoning

**TypeScript allows:**
```typescript
interface User { name: string; age: number; }
const user: User = { name: "Alice", age: 30 };

// Mutating a property to an incompatible type!
Object.assign(user, { age: "thirty" }); // ✅ tsc allows

user.age.toFixed(); // 💥 Runtime: user.age is a string!
```
Because `Object.assign(target, source)` is typed to return an intersection (`T & U`), it doesn't enforce that the `source` object is a valid patch for the `target` object's existing properties. It assumes you are creating a *new* combined type, ignoring that the `target` reference is being unsafely mutated in place.

**Sound Mode:**
`Object.assign(target, ...sources)` must enforce that any overlapping properties in `sources` are deeply assignable to the corresponding properties in `target`. The target mutation must be type-safe.

---

## What Sound Mode Would NOT Change

Some things that might seem like unsoundness are actually deliberate precision trade-offs:

### Literal Widening for `let`

```typescript
let x = "hello";  // x: string (not "hello")
```

This is **not unsound** - it's conservative. TypeScript could track never-reassigned `let` bindings, but chose not to for simplicity. The type is wider than necessary but still correct.

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

TypeScript handles this correctly. The issue is only in *assignability* (see 7.1 above).

### Control Flow Analysis

TypeScript already does excellent CFA for narrowing. Sound Mode wouldn't change this - it would focus on the structural type system rules.

---

## Implementation Considerations

### Flag Design

```typescript
// tsconfig.json
{
  "compilerOptions": {
    "sound": true,           // Enable all sound checks
    // Or granular control:
    "soundArrayVariance": true,  // Invariant mutable arrays
    "soundMethodVariance": true, // Contravariant method params
    "soundThisVariance": true,   // Contravariant this type
    "soundAnyType": true,        // any as top-only (Unknown conversion)
    "soundFunctionType": true,   // Strict Function type
    "soundIndexAccess": true,    // T | undefined for index access
    "soundEnums": true,          // Closed numeric enums
    "soundGenerics": true,       // Strict generic identity
    "soundBoxing": true,         // No implicit primitive boxing
    "soundExactTypes": true,     // Opt-in Exact Object checking
    "soundSwitchExhaustive": true, // Exhaustive switch for unions
    "soundMembershipChecks": true, // Allows unknown in .includes/.has
    "soundArrayReduce": true,    // Non-Empty arrays required for reduce
    "soundReadonlyAliasing": true, // Reject assigning readonly to mutable
    "soundTuples": true,         // Omit array mutation methods on tuples
    "soundCatchVariables": true, // Always enforce unknown in catch blocks
    "soundObjectAssign": true,   // Enforce target compatibility on assign
  }
}
```

### Compatibility

Sound Mode would reject some valid TypeScript code. Migration strategy:
1. Run in "report only" mode to see what would break
2. Fix or annotate problematic patterns
3. Enable enforcement

### Performance

Most sound checks are O(1) additions to existing checks. The main cost is:
- Tracking read/write types separately for properties
- Additional variance checks on generic instantiation

### Implementation in tsz

The Judge/Lawyer architecture makes this straightforward. Add flags to `SubtypeChecker`:

```rust
// src/solver/subtype.rs
pub struct SubtypeChecker<'a, R: TypeResolver = NoopResolver> {
    // ... existing flags
    pub sound_array_variance: bool,
    pub sound_method_variance: bool,
    pub sound_this_variance: bool,
    pub sound_any_type: bool,
    pub sound_function_type: bool,
    pub sound_enums: bool,
    pub sound_generics: bool,
    pub sound_boxing: bool,
}
```

The `CompatChecker` (Lawyer) would configure these based on compiler options:

```rust
// src/solver/compat.rs
impl CompatChecker {
    pub fn from_options(options: &CompilerOptions) -> Self {
        if options.sound_mode {
            // All sound flags enabled
            Self { all_sound: true, .. }
        } else {
            // TypeScript-compatible defaults
            Self::default()
        }
    }
}
```

Key implementation locations:
- **Array variance**: `src/solver/subtype_rules/intrinsics.rs`
- **Method variance**: `src/solver/subtype_rules/functions.rs`
- **Enum strictness**: `src/solver/compat.rs` (`enum_assignability_override`)
- **Generic identity**: `src/solver/subtype_rules/generics.rs`
- **Primitive boxing**: `src/solver/subtype_rules/intrinsics.rs` (`is_boxed_primitive_subtype`)

---

## Trade-offs Summary

| Aspect | TypeScript (tsc) | Sound Mode |
|--------|------------------|------------|
| Array variance | Covariant (unsafe) | Invariant for mutable |
| Method params | Bivariant | Contravariant |
| `this` in params | Covariant | Contravariant |
| `any` type | Top + Bottom | Top only (or quarantine wall) |
| `Function` type | Accepts all callables | Requires explicit cast |
| Index access | `T` | `T \| undefined` |
| Numeric enums | Open to any number | Closed to defined values |
| Void returns | Any return OK | Strict matching |
| Weak types | Warning | Error |
| Conditional distribution | Automatic | Opt-in |
| Generic identity | Constraint-based | Structural identity |
| Primitive boxing | `number` → `Object` OK | Explicit wrapper required |
| Split accessors | Assignment allowed | Check setter accepts write type |
| Structural Excess | Accepted on variables | Rejected/Opt-in `Exact<T>` |
| Switch Exh. | Checked via return type | Checked implicitly |
| Array.includes | Subtype match required | `unknown` type allowed |
| Array.reduce | Permissive | `NonEmptyArray` or `T \| undefined` |
| Readonly Props | Assignable to mutable | Rejected (strict aliasing) |
| Tuples | Inherit Array mutations | Mutation methods typed `never` |
| Error handling | Implicit `any` | Strict `unknown` |
| Object.assign | Intersects `T & U` | Enforces structural compatibility |

**The fundamental trade-off:** Sound Mode catches more bugs but requires more explicit annotations. It's not "better" - it's different. Choose based on your project's needs.

---

## References

- [TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md) - Complete list of TypeScript's intentional unsoundness
- [NORTH_STAR.md](../architecture/NORTH_STAR.md) - Judge/Lawyer architecture documentation
- [TypeScript Design Goals](https://github.com/Microsoft/TypeScript/wiki/TypeScript-Design-Goals) - Why TypeScript chose pragmatism
