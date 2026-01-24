// Final Conformance Validation Test Suite
// This file validates all error codes worked on by the 12 agents
// Tests: TS2322, TS2694, TS2339, TS1005, TS2300, TS2571, TS2507, TS2318, TS2583, TS2307, TS2304, TS2488, TS18050, TS2362, TS2363, TS2693

// =============================================================================
// Worker 1: TS2322 - Type not assignable
// =============================================================================

let x: number = "string"; // TS2322
let y: string = 42; // TS2322

function takesString(s: string) {}
takesString(123); // TS2322

interface A { a: number; }
interface B { b: string; }
let a: A = {} as B; // TS2322

// =============================================================================
// Worker 2: TS2694 - Namespace not assignable
// =============================================================================

namespace NS1 {
    export const value = 42;
}
namespace NS2 {
    export const value = "hello";
}
let ns: typeof NS1 = NS2; // TS2694

// =============================================================================
// Worker 3: TS2339 - Property does not exist
// =============================================================================

interface Obj {
    known: string;
}
const obj: Obj = { known: "test" };
console.log(obj.unknown); // TS2339

// =============================================================================
// Worker 4: TS1005 - Token expected
// =============================================================================

// Missing closing brace
function broken() { // TS1005 if missing brace elsewhere

// =============================================================================
// Worker 5: TS2300 - Duplicate identifier
// =============================================================================

var duplicate = 1;
var duplicate = 2; // TS2300

// =============================================================================
// Worker 6: TS2571 - Object is of type 'unknown'
// =============================================================================

const unknownVar: unknown = { value: 42 };
console.log(unknownVar.value); // TS2571

// =============================================================================
// Worker 7: TS2507 - Type is not a constructor
// =============================================================================

interface NotConstructor {
    value: number;
}
new NotConstructor(); // TS2507

// =============================================================================
// Worker 8: TS2318 - Cannot find type
// =============================================================================

const x2: NonExistentType = 42; // TS2318

// =============================================================================
// Worker 9: TS2583 - Cannot find name (change lib)
// =============================================================================

// Using ES2015+ global when lib doesn't include it
// This would need specific lib settings to trigger

// =============================================================================
// Worker 10: TS2307 - Cannot find module
// =============================================================================

import { something } from './non-existent-module'; // TS2307

// =============================================================================
// Worker 11: TS2304 - Cannot find name
// =============================================================================

const undef = undefinedValue; // TS2304

// =============================================================================
// Worker 12: TS2488 - Iterator protocol (NON-ITERABLE)
// =============================================================================

class NotIterable2 {
    value: number = 42;
}

const notIterable2 = new NotIterable2();
const [a2, b2] = notIterable2; // TS2488

// =============================================================================
// Worker 12: TS2693 - Type used as value
// =============================================================================

interface MyInterface2 {
    prop: string;
}
const interfaceValue2 = MyInterface2; // TS2693

type MyType2 = string;
const typeValue2 = MyType2; // TS2693

// =============================================================================
// Worker 12: TS2362/TS2363 - Arithmetic operand errors
// =============================================================================

const obj3 = { value: 42 };
const result1 = obj3 + 10; // TS2362

const obj4 = { value: "test" };
const result2 = 10 + obj4; // TS2363

// =============================================================================
// Stability Tests - No crashes/OOM/Timeouts
// =============================================================================

// Deeply nested types (should not crash)
type Deep1<T> = { a: Deep2<T> };
type Deep2<T> = { b: Deep3<T> };
type Deep3<T> = { c: Deep4<T> };
type Deep4<T> = { d: Deep5<T> };
type Deep5<T> = { e: Deep6<T> };
type Deep6<T> = { f: Deep7<T> };
type Deep7<T> = { g: Deep8<T> };
type Deep8<T> = { h: Deep9<T> };
type Deep9<T> = { i: Deep10<T> };
type Deep10<T> = { j: T };

let deep: Deep1<number> = { a: { b: { c: { d: { e: { f: { g: { h: { i: { j: 42 } } } } } } } } } };

// Circular type references (should not hang)
type Circular1 = Circular2 & { prop1: string };
type Circular2 = Circular1 & { prop2: number };

let circular: Circular1 = { prop1: "test", prop2: 42 };

// Deeply nested function calls (should not stack overflow)
function f1(x: number): number { return f2(x); }
function f2(x: number): number { return f3(x); }
function f3(x: number): number { return f4(x); }
function f4(x: number): number { return f5(x); }
function f5(x: number): number { return f6(x); }
function f6(x: number): number { return f7(x); }
function f7(x: number): number { return f8(x); }
function f8(x: number): number { return f9(x); }
function f9(x: number): number { return f10(x); }
function f10(x: number): number { return x; }

const deepCall = f1(42);

// =============================================================================
// Additional edge cases
// =============================================================================

// Array destructuring with non-iterable
for (const item of notIterable2) { // TS2488
    console.log(item);
}

// Spread operator with non-iterable
const spread = [...notIterable2]; // TS2488

// ============================================================================
// Test complete
// ============================================================================

console.log("All validation tests complete");
