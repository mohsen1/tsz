# Sound Mode

<div class="alert alert-warning">
  <strong>Exploring Phase</strong> - Sound mode is an active area of research. Diagnostic codes, behavior, and coverage are subject to change. We're sharing it early to gather feedback.
</div>

**tsz** offers an opt-in **sound mode** - a stricter type-checking mode that catches real bugs TypeScript allows by design.

## Quick Start

```bash
# CLI flag
tsz check --sound src/

# tsconfig.json
# { "compilerOptions": { "sound": true } }

# Per-file pragma
# // @tsz-sound
```

Or try it in the [Playground](/playground/) - select the **Sound Mode** example and check the **sound** checkbox.

## Why Sound Mode?

TypeScript is **intentionally unsound**. The TypeScript team prioritizes developer ergonomics over type-theoretic correctness. This means there are programs that type-check successfully but crash at runtime.

## What Sound Mode Catches

When sound mode is enabled, tsz applies stricter rules that close TypeScript's known escape hatches. Each check has a dedicated diagnostic code in the `TSZ9xxx` range.

### TSZ9101: Covariant Mutable Arrays

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

### TSZ9102: Method Parameter Bivariance

TypeScript allows method parameters to be checked bivariantly - both covariant and contravariant. This lets subclasses narrow parameter types unsafely.

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

### TSZ9201: `any` Escape Detection

`any` is the biggest source of unsoundness. It acts as both a top and a bottom type, silently bypassing all structural checks.

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

Sound mode: `any` cannot bypass structural checks. Use `unknown` with proper narrowing instead:

```typescript
const data: unknown = JSON.parse(input);

// Must narrow before use
if (typeof data === "object" && data !== null
    && "name" in data && "age" in data) {
  processUser(data as { name: string; age: number });
}
```

### TSZ9202: Unsafe Type Assertions

Type assertions (`as`) let developers punch holes through the type system. Sound mode flags disjoint casts.

```typescript
interface Cat { meow(): void }
interface Dog { bark(): void }

const cat: Cat = { meow() { console.log("meow") } };

// tsc allows this - types are completely unrelated!
const dog = cat as unknown as Dog;
dog.bark(); // 💥 Runtime: dog.bark is not a function
```

### TSZ9306: Sticky Freshness

TypeScript checks excess properties on direct object literal assignment, but the check is bypassed through variable indirection. Sound mode preserves "freshness" so excess properties are always caught.

```typescript
interface Point2D { x: number; y: number }

// Direct assignment - tsc catches the excess property
const p1: Point2D = { x: 1, y: 2, z: 3 }; // Error: 'z' does not exist

// Indirect assignment - tsc allows it!
const point3d = { x: 1, y: 2, z: 3 };
const p2: Point2D = point3d; // tsc: no error. Sound mode: error!

// Same bypass in function arguments
function distance(a: Point2D, b: Point2D): number {
  return Math.sqrt((a.x - b.x) ** 2 + (a.y - b.y) ** 2);
}
const origin = { x: 0, y: 0, label: "origin" };
distance(origin, point3d); // tsc: no error. Sound mode: error!
```

### TSZ9501: Unchecked Indexed Access

Accessing an object or array by index may return `undefined` if the key doesn't exist. TypeScript assumes the value is always present.

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

### TSZ9601: Enum-Number Assignment

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

Sound mode: make distribution opt-in rather than the default behavior.

### Impossible Intersections

TypeScript allows intersections that can never be satisfied:

```typescript
type Broken = { [key: string]: number } & { name: string };
// 'name' must be both number (from index sig) and string - impossible!
// TypeScript keeps this type alive instead of reducing to never
```

Sound mode: reduce impossible intersections to `never` immediately.

### Generic Constraint Confusion

TypeScript allows different type parameters with the same constraint to be treated as interchangeable:

```typescript
function f<T extends string, U extends string>(t: T): U {
  return t; // tsc allows - both extend string!
  // But T and U could be different literal types!
}

const result = f<"hello", "world">("hello");
// result is typed as "world" but is actually "hello"
```

Sound mode: two different type parameters are never subtypes of each other unless they are the same parameter.

### Exact Types

TypeScript's structural typing means any object with extra properties satisfies an interface. Sound mode introduces `Exact<T>` to opt into strict shape matching, making `Object.keys()` return `(keyof T)[]` safely:

```typescript
// With Exact<T>, this is sound:
function getKeys<T>(obj: Exact<T>): (keyof T)[] {
  return Object.keys(obj) as (keyof T)[];
}

// Without it, Object.keys may return keys not in T
```

## Making Sound Mode Practical

Catching unsoundness is only useful if you can actually enable it without drowning in false positives from third-party code. We're building several mechanisms to make sound mode practical for real codebases.

### SoundlyTyped: Automated Ecosystem Soundness

The biggest obstacle to sound typing is the ecosystem. Libraries ship `.d.ts` files full of `any`, bivariant methods, and open enums. **SoundlyTyped** is a fully automated transformation pipeline that rewrites upstream type definitions:

- Maps `any` to `unknown` at module boundaries
- Closes open numeric enums
- Patches bivariant method signatures to use contravariant parameters
- Regenerates automatically when upstream packages update

No hand-coded type patches - purely mechanical transformations applied to `node_modules` and DefinitelyTyped.

### Sound Core Libraries

Standard library types like `JSON.parse()`, `fetch().json()`, and `document.getElementById()` return `any` for ergonomics. Sound mode ships alternative core typings (`*.sound.lib.d.ts`) where these return `unknown` instead, forcing explicit narrowing at every external data boundary.

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

Sound mode pairs naturally with runtime validation libraries. When data crosses a trust boundary - user input, API responses, file reads - you need runtime checks regardless of your type system. Sound mode makes this explicit:

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

- **Per-file pragma** (`// @tsz-sound`) - enable sound mode one file at a time
- **Per-directory** - use `tsconfig.json` extends to enable sound mode in specific directories
- **Suppression comments** - `// @tsz-unsound` to acknowledge and suppress specific diagnostics during migration
