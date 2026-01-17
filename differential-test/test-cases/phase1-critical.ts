// Phase 1 Critical Test Cases for Differential Testing
// These test the fundamental unsoundness rules required for basic TypeScript compatibility

// =============================================================================
// Test 1: The "Any" Type (Catalog #1)
// any is both top and bottom - assignable to/from everything
// =============================================================================

export namespace AnyTypeTests {
  // any -> specific type
  const a1: any = "hello";
  const a2: number = a1;  // Should not error (any is assignable to number)

  // specific type -> any
  const b1: string = "test";
  const b2: any = b1;  // Should not error (string is assignable to any)

  // any in function parameters
  function acceptsAny(x: any): void { }
  acceptsAny(123);
  acceptsAny("str");
  acceptsAny({ foo: 1 });

  // any as return type
  function returnsAny(): any { return 42; }
  const anyResult: string = returnsAny();  // Should not error
}

// =============================================================================
// Test 2: Object vs object vs {} Trifecta (Catalog #20)
// =============================================================================

export namespace ObjectTrifectaTests {
  // Object (global interface) - accepts primitives
  const obj1: Object = 123;      // Should not error
  const obj2: Object = "hello";  // Should not error
  const obj3: Object = true;     // Should not error

  // {} (empty object) - also accepts primitives
  const empty1: {} = 123;        // Should not error
  const empty2: {} = "hello";    // Should not error
  const empty3: {} = { a: 1 };   // Should not error

  // object (lowercase) - ONLY non-primitives
  const lower1: object = { a: 1 };  // Should not error
  // const lower2: object = 123;    // Should error (TS2322)
  // const lower3: object = "str";  // Should error (TS2322)
}

// =============================================================================
// Test 3: Void Return Exception (Catalog #6)
// Functions returning void accept functions with non-void returns
// =============================================================================

export namespace VoidReturnTests {
  type VoidCallback = () => void;

  // Function returning string assigned to void callback - should work
  const stringReturner: VoidCallback = () => "hello";  // Should not error

  // Function returning number assigned to void callback - should work
  const numberReturner: VoidCallback = () => 42;  // Should not error

  // Array forEach expects void callback
  const arr = [1, 2, 3];
  arr.forEach((x) => x * 2);  // Returns number, but forEach expects void callback
}

// =============================================================================
// Test 4: Error Poisoning (Catalog #11)
// Error type should not cascade
// =============================================================================

export namespace ErrorPoisoningTests {
  // If a type is error, it should be compatible with everything
  // This prevents one error from cascading into many
  // (This is hard to test directly - it's about internal compiler behavior)

  // Test that undefined variables produce one error, not cascading errors
  // undefinedVar;  // Single error for "Cannot find name"
}

// =============================================================================
// Test 5: Covariant Mutable Arrays (Catalog #3)
// Dog[] is assignable to Animal[], despite mutation unsoundness
// =============================================================================

export namespace CovariantArrayTests {
  interface Animal { name: string; }
  interface Dog extends Animal { bark(): void; }

  const dogs: Dog[] = [{ name: "Fido", bark: () => {} }];
  const animals: Animal[] = dogs;  // Should not error (covariant arrays)

  // This is where unsoundness comes in (but TS allows it)
  // animals.push({ name: "Cat" });  // Would crash at runtime if called bark()
}

// =============================================================================
// Test 6: Function Bivariance (Catalog #2)
// =============================================================================

export namespace FunctionBivarianceTests {
  interface Animal { name: string; }
  interface Dog extends Animal { bark(): void; }

  // Method bivariance - should work both ways
  interface Handler {
    handle(animal: Animal): void;
  }

  // With strictFunctionTypes: false, this would be allowed
  // const dogHandler: Handler = { handle: (dog: Dog) => dog.bark() };

  // Function type (with strict) should be contravariant
  type AnimalHandler = (a: Animal) => void;
  type DogHandler = (d: Dog) => void;

  // DogHandler is NOT assignable to AnimalHandler in strict mode
  // const ah: AnimalHandler = ((d: Dog) => d.bark());  // Should error in strict
}

// =============================================================================
// Test 7: Null/Undefined with strictNullChecks (Catalog #9)
// =============================================================================

export namespace NullUndefinedTests {
  // With strictNullChecks: true
  const maybeString: string | null = null;
  // const definiteString: string = maybeString;  // Should error (TS2322)

  // Null check narrows
  if (maybeString !== null) {
    const str: string = maybeString;  // Should not error after narrowing
  }

  // Optional parameter
  function greet(name?: string): string {
    return `Hello, ${name ?? "World"}`;
  }
}

// =============================================================================
// Test 8: Literal Widening (Catalog #10)
// =============================================================================

export namespace LiteralWideningTests {
  // const keeps literal type
  const constStr = "hello";  // Type is "hello"

  // let widens
  let letStr = "hello";  // Type is string

  // Object property widens
  const obj = { name: "test" };  // Type is { name: string }, not { name: "test" }

  // as const preserves
  const frozen = { name: "test" } as const;  // Type is { readonly name: "test" }
}

// =============================================================================
// Test 9: Optionality vs Undefined (Catalog #14)
// =============================================================================

export namespace OptionalityTests {
  interface Config {
    port?: number;
    host?: string;
  }

  // Optional property can be missing
  const c1: Config = {};  // Should not error
  const c2: Config = { port: 8080 };  // Should not error

  // undefined can be assigned to optional (default behavior)
  const c3: Config = { port: undefined };  // Should not error by default
}

// =============================================================================
// Test 10: Excess Property Checks (Catalog #4)
// Fresh objects get checked, variables don't
// =============================================================================

export namespace ExcessPropertyTests {
  interface Point { x: number; y: number; }

  // Direct literal - excess property check applies
  // const p1: Point = { x: 1, y: 2, z: 3 };  // Should error (TS2353)

  // Via variable - no excess property check
  const temp = { x: 1, y: 2, z: 3 };
  const p2: Point = temp;  // Should NOT error (width subtyping)

  // Function parameter
  function usePoint(p: Point): void { }
  // usePoint({ x: 1, y: 2, z: 3 });  // Should error (TS2353)
  usePoint(temp);  // Should NOT error
}
