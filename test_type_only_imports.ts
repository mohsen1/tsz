// Test file for type-only import errors (TS2693)
// These test cases verify that type-only imports/types are rejected when used as values

// ============================================================================
// Test 1: Type-only import used as value
// ============================================================================

import type { Foo } from './type-only-module';
const x = new Foo(); // TS2693: 'Foo' only refers to a type, but is being used as a value here.

// ============================================================================
// Test 2: Interface used as value
// ============================================================================

interface Bar {
    x: number;
}

const y = new Bar(); // TS2693: 'Bar' only refers to a type, but is being used as a value here.

// ============================================================================
// Test 3: Type alias used as value
// ============================================================================

type Baz = string;
const z = new Baz(); // TS2693: 'Baz' only refers to a type, but is being used as a value here.

// ============================================================================
// Test 4: Class as type (should work)
// ============================================================================

class Qux {
    constructor() {}
}

const w = new Qux(); // Should work - Qux has a value

// ============================================================================
// Test 5: typeof to get type of class
// ============================================================================

type Quux = typeof Qux;
const v = new Quux(); // TS2693: 'Quux' only refers to a type, but is being used as a value here.

// ============================================================================
// Test 6: Enum used as type and value (should work for value access)
// ============================================================================

enum Enum {
    A,
    B
}

const u = Enum.A; // Should work - enums have both type and value

// ============================================================================
// Test 7: Namespace used as value (should work)
// ============================================================================

namespace NS {
    export const value = 42;
}

const t = NS.value; // Should work - namespaces have values

// ============================================================================
// Test 8: Function type used as value
// ============================================================================

type FuncType = (x: number) => string;
const fn: FuncType = (x) => x.toString(); // OK - using the type
// const bad = FuncType(5); // TS2693 - FuncType only refers to a type

// ============================================================================
// Test 9: Interface in value position
// ============================================================================

interface Interface {
    method(): void;
}

// const obj: Interface = new Interface(); // TS2693: Interface only refers to a type
// Correct way: implement the interface
class Implementation implements Interface {
    method() {}
}

const obj: Interface = new Implementation(); // Should work

// ============================================================================
// Test 10: Generic type as value
// ============================================================================

type Generic<T> = {
    value: T;
};

// const g = new Generic<number>(); // TS2693: Generic only refers to a type

// ============================================================================
// Test 11: typeof type used incorrectly
// ============================================================================

class MyClass {
    value: number;
}

type MyType = typeof MyClass;
// const instance = new MyType(); // TS2693: MyType only refers to a type

// ============================================================================
// Test 12: Import type with default
// ============================================================================

// import type Default from './some-module';
// const d = new Default(); // TS2693

// ============================================================================
// Test 13: Mixed type and value imports
// ============================================================================

// import { TypeOnly, ValueOnly } from './module';
// const a = TypeOnly; // TS2693
// const b = ValueOnly; // Should work

// ============================================================================
// Test 14: typeof on interface
// ============================================================================

interface AnotherInterface {
    prop: string;
}

type YetAnother = typeof AnotherInterface; // Error in the typeof itself
// const c = new YetAnother(); // Also TS2693

// ============================================================================
// Test 15: Type parameter in function used as value
// ============================================================================

function factory<T>() {
    // return new T(); // TS2693: T only refers to a type
}

console.log("Type-only import tests complete");
