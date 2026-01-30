# Sound Mode Aspirations

**Status**: Draft / Future Feature
**Last Updated**: January 2026

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

### Category 2: The `any` Escape Hatch

#### 2.1 `any` as Both Top and Bottom Type

**TypeScript allows:**
```typescript
const x: any = "hello";
const y: number = x;      // ✅ tsc: any → number
y.toFixed(2);             // 💥 Runtime: "hello".toFixed()
```

**Sound Mode:** Treat `any` as only a top type (like `unknown`). Require explicit casts to use as a specific type.

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
    "soundAnyType": true,        // any as top-only
    "soundFunctionType": true,   // Strict Function type
    "soundIndexAccess": true,    // T | undefined for index access
    "soundEnums": true,          // Closed numeric enums
    "soundGenerics": true,       // Strict generic identity
    "soundBoxing": true,         // No implicit primitive boxing
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
| `any` type | Top + Bottom | Top only |
| `Function` type | Accepts all callables | Requires explicit cast |
| Index access | `T` | `T \| undefined` |
| Numeric enums | Open to any number | Closed to defined values |
| Void returns | Any return OK | Strict matching |
| Weak types | Warning | Error |
| Conditional distribution | Automatic | Opt-in |
| Generic identity | Constraint-based | Structural identity |
| Primitive boxing | `number` → `Object` OK | Explicit wrapper required |
| Split accessors | Assignment allowed | Check setter accepts write type |

**The fundamental trade-off:** Sound Mode catches more bugs but requires more explicit annotations. It's not "better" - it's different. Choose based on your project's needs.

---

## References

- [TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md) - Complete list of TypeScript's intentional unsoundness
- [NORTH_STAR.md](../architecture/NORTH_STAR.md) - Judge/Lawyer architecture documentation
- [TypeScript Design Goals](https://github.com/Microsoft/TypeScript/wiki/TypeScript-Design-Goals) - Why TypeScript chose pragmatism
