// Test file for TS2488 - Spread operations on non-iterable types

// Test 1: Spread in array literal - number
const arr1 = [...5]; // TS2488

// Test 2: Spread in array literal - boolean
const arr2 = [...true]; // TS2488

// Test 3: Spread in array literal - object
const arr3 = [{}]; // TS2488

// Test 4: Spread in array literal - null
const arr4 = [...null]; // TS2488

// Test 5: Spread in array literal - undefined
const arr5 = [...undefined]; // TS2488

// Test 6: Spread in array literal - function
function foo() {}
const arr6 = [...foo]; // TS2488

// Test 7: Spread in array literal - object literal
const obj = { x: 1 };
const arr7 = [...obj]; // TS2488

// Test 8: Spread in function call - number
function test1(...args: number[]) {}
test1(...5); // TS2488

// Test 9: Spread in function call - object
function test2(...args: string[]) {}
test2(...{}); // TS2488

// Test 10: Spread in function call - boolean
function test3(...args: boolean[]) {}
test3(...false); // TS2488

// Test 11: Spread in function call - null
function test4(...args: any[]) {}
test4(...null); // TS2488

// Test 12: Spread in function call - undefined
function test5(...args: number[]) {}
test5(...undefined); // TS2488

// Test 13: Multiple spreads in array literal
const arr8 = [1, ...true, ...{}]; // TS2488 for both spreads

// Test 14: Class instance without iterator
class MyClass {
    value: number;
}
const instance = new MyClass();
const arr9 = [...instance]; // TS2488

// Test 15: Custom iterable (should work - no error)
const iterable = {
    [Symbol.iterator]: function* () {
        yield 1;
        yield 2;
    }
};
const arr10 = [...iterable]; // Should NOT error - this is iterable

// Test 16: Array spread (should work - no error)
const validArray = [1, 2, 3];
const arr11 = [...validArray]; // Should NOT error - arrays are iterable

// Test 17: String spread (should work - no error)
const str = "hello";
const arr12 = [...str]; // Should NOT error - strings are iterable

// Test 18: Spread in new expression
function createArray<T>(...items: T[]): T[] {
    return items;
}
const result = createArray(...42); // TS2488
