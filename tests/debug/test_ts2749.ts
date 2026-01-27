// Test cases for TS2749 error emission

// CASE 1: Class - should NOT emit TS2749 (class has both VALUE and TYPE flags)
class MyClass {
    x: number;
}
let a: MyClass; // OK - MyClass is a type

// CASE 2: Interface - should NOT emit TS2749
interface MyInterface {
    x: number;
}
let b: MyInterface; // OK - MyInterface is a type

// CASE 3: Enum - should NOT emit TS2749
enum MyEnum {
    A, B
}
let c: MyEnum; // OK - MyEnum is a type

// CASE 4: Variable - SHOULD emit TS2749
const myVar = 42;
// let d: myVar; // ERROR - myVar is a value

// CASE 5: Function - SHOULD emit TS2749
function myFunc() {}
// let e: myFunc; // ERROR - myFunc is a value

// CASE 6: Namespace merging - function + namespace
namespace A {
    export function B<T>(x: T) { return x; } // VALUE
    export namespace B { // VALUE + MODULE
        export var x = 1;
    }
}
// var f: A.B; // ERROR - A.B refers to the function (value), not the namespace
// A.B(1); // OK - calling the function

// CASE 7: Type alias - should NOT emit TS2749
type MyType = string;
let g: MyType; // OK - MyType is a type

// CASE 8: Namespace - should NOT emit TS2749 in some contexts
namespace MyNamespace {
    export type T = number;
}
let h: MyNamespace.T; // OK - accessing type through namespace

// CASE 9: Import type - should NOT emit TS2749
// import type { TypeAlias } from "./module";
// let i: TypeAlias; // OK - type-only import

// CASE 10: Class + namespace merge
class MyMergedClass {
    x: number;
}
namespace MyMergedClass {
    export const y = 42;
}
let j: MyMergedClass; // OK - class type
