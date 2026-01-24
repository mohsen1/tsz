// Test file for TS2488 - Type must have Symbol.iterator
// Array destructuring of non-iterable types

// Test 1: Number is not iterable
const [a1] = 5; // TS2488

// Test 2: Boolean is not iterable
const [a2] = true; // TS2488

// Test 3: Object without iterator is not iterable
const [a3] = {}; // TS2488

// Test 4: Class instance without iterator is not iterable
class Foo {}
const foo = new Foo();
const [a4] = foo; // TS2488

// Test 5: Null is not iterable
const [a5] = null; // TS2488

// Test 6: Undefined is not iterable
const [a6] = undefined; // TS2488

// Test 7: Function is not iterable
function bar() {}
const [a7] = bar; // TS2488

// Test 8: Object literal without iterator
const obj = { x: 1 };
const [a8] = obj; // TS2488

// Test 9: Generic object type
interface NotIterable {
    prop: string;
}
declare const notIterable: NotIterable;
const [a9] = notIterable; // TS2488

// Test 10: Custom class without iterator
class CustomClass {
    value: number;
    constructor(v: number) {
        this.value = v;
    }
}
const custom = new CustomClass(42);
const [a10] = custom; // TS2488

// Test 11: Nested destructuring with non-iterable
const [, [b1]] = 123; // TS2488

// Test 12: Rest pattern with non-iterable
const [...rest] = 456; // TS2488

// Test 13: Multiple destructuring of non-iterable
const [c1, c2, c3] = false; // TS2488
