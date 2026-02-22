# Sound Mode

**Status**: ✅ IMPLEMENTED (Basic)
**Last Updated**: February 2026

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
| Mutable array covariance | ✅ | TSZ9101 |
| Method bivariance | ✅ | TSZ9102 |
| `any` escape detection | ✅ | TSZ9201 |
| Unsafe type assertion | ✅ | TSZ9202 |
| Sticky freshness | ✅ | TSZ9306 |
| Missing index signature | ✅ | TSZ9307 |
| Unchecked indexed access | ✅ | TSZ9501 |
| Enum-number assignment | ✅ | TSZ9601 |

See `src/solver/sound.rs` for implementation details.

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

### Group 9100: Variance & Subtyping

#### TSZ9101: Covariant Mutable Arrays

**TypeScript allows:**
```typescript
const dogs: Dog[] = [new Dog()];
const animals: Animal[] = dogs;  // ✅ tsc
animals.push(new Cat());         // 💥 Runtime: Cat in Dog[]
```

**Sound Mode:** Reject `Dog[]` → `Animal[]` for mutable arrays. Only `readonly Dog[]` → `readonly Animal[]` is safe.

#### TSZ9102: Method Parameter Bivariance

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

### Group 9200: The `any` Epidemic & Boundaries

`any` is the biggest source of unsoundness in TypeScript. It infects anything it touches by acting as both a top and a bottom type.

#### TSZ9201: Ban Explicit `any` (and `any` as Top/Bottom)

In Sound Mode, developers are banned from writing explicit `any`. They must use `unknown`. If an `any` type still somehow bypasses the `SoundlyTyped` infrastructure, the compiler strictly treats it as a top type equivalent to `unknown`. It cannot be assigned to more specific types without an explicit cast.

**TypeScript allows:**
```typescript
const x: any = "hello";
const y: number = x;      // ✅ tsc: any → number
y.toFixed(2);             // 💥 Runtime: "hello".toFixed()
```

#### TSZ9202: Unsafe Type Assertion
Type assertions `as any` or casting to arbitrary disjoint types without overlapping checks are strictly regulated or forbidden in Sound Mode, preventing developers from manually punching holes through the type checker.

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

Sound Mode relies on **explicit boundary enforcement** via automated structural transformations of the ecosystem's definitions.

**1. `SoundlyTyped`: Automated Ecosystem Soundness**
To handle the massive ecosystem of `node_modules` and DefinitelyTyped (DT), we introduce **`SoundlyTyped`**—a fully automated infrastructure layer.
- **Zero Hand-Coded Types:** Purely mechanical transformation pipeline.
- **Automated Transformation:** Rewrites upstream `.d.ts` files: maps `any` to `unknown`, closes open numeric enums, patches bivariant method signatures.
- **Evergreen Sync:** Automatically regenerates when upstream packages update.

**2. Sound Core Libraries (`*.sound.lib.d.ts`)**
Standard libraries often return `any` for ergonomics. A true Sound Mode defaults to sound alternative core typings (e.g., `dom.sound.lib.d.ts`). For instance, `JSON.parse` and `fetch().json()` return `unknown`.

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

**Sound Mode:** Sound mode enforces "Exact Types" (opt-in `Exact<T>` or by default), where objects are strictly forbidden from containing extra un-declared properties, making it perfectly sound for `Object.keys(user)` to return `(keyof User)[]`.

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
Object literal freshness is preserved through variables to ensure excess property checks are not easily bypassed.

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

**Sound Mode:** Reject - `(...args: any[])` is NOT a supertype of specific signatures.

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

**TypeScript allows:**
```typescript
const arr: number[] = [];
const x = arr[100];       // ✅ tsc: x is number
x.toFixed();              // 💥 Runtime: undefined.toFixed()
```

**Sound Mode:** Type index access as `T | undefined` by default.

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

#### TSZ9601: Numeric Enum Openness

**TypeScript allows:**
```typescript
enum Status { Active = 0, Inactive = 1 }
const s: Status = 999;  // ✅ tsc allows any number!
```

**Sound Mode:** Make numeric enums closed - only defined values allowed.

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

**Sound Mode:** Make distribution opt-in rather than default.

#### TSZ9702: Intersection Survival

**TypeScript allows:**
```typescript
type Broken = { [key: string]: number } & { name: string };
// Impossible type - name must be both number and string
const x: Broken = ???;  // Can't create, but type exists
```

**Sound Mode:** Reduce impossible intersections to `never` immediately.

#### TSZ9703: Generic Constraint Confusion

**TypeScript allows:**
```typescript
function f<T extends string, U extends string>(t: T): U {
    return t;  // ✅ tsc allows - both extend string!
}
```

**Sound Mode:** Two different type parameters should never be subtypes of each other unless they are the same parameter. 

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

---

## What Sound Mode Would NOT Change

Some things that might seem like unsoundness are actually deliberate precision trade-offs:

### Literal Widening for `let`

```typescript
let x = "hello";  // x: string (not "hello")
```

This is **not unsound** - it's conservative. The type is wider than necessary but still correct.

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

### Compatibility & Performance

Sound Mode would reject some valid TypeScript code. Migration requires running in "report only" mode and manually fixing code structures. Most sound checks are `O(1)` additions to existing checks. The main cost is tracking read/write types separately for properties and additional variance checks on generic instantiation.

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