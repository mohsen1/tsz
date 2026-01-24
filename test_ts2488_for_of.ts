// Test file for TS2488 - for-of loops with non-iterable types

// Test 1: for-of with number
for (const x of 5) { // TS2488
    console.log(x);
}

// Test 2: for-of with boolean
for (const x of true) { // TS2488
    console.log(x);
}

// Test 3: for-of with object
for (const x of {}) { // TS2488
    console.log(x);
}

// Test 4: for-of with null
for (const x of null) { // TS2488
    console.log(x);
}

// Test 5: for-of with undefined
for (const x of undefined) { // TS2488
    console.log(x);
}

// Test 6: for-of with function
function foo() {}
for (const x of foo) { // TS2488
    console.log(x);
}

// Test 7: for-of with class instance
class Bar {}
const bar = new Bar();
for (const x of bar) { // TS2488
    console.log(x);
}

// Test 8: for-of with object literal
const obj = { a: 1 };
for (const x of obj) { // TS2488
    console.log(x);
}

// Test 9: for-of with array (should work - no error)
const arr = [1, 2, 3];
for (const x of arr) { // Should NOT error
    console.log(x);
}

// Test 10: for-of with string (should work - no error)
const str = "hello";
for (const x of str) { // Should NOT error
    console.log(x);
}

// Test 11: for-of with custom iterable (should work - no error)
const iterable = {
    [Symbol.iterator]: function* () {
        yield 1;
        yield 2;
        yield 3;
    }
};
for (const x of iterable) { // Should NOT error
    console.log(x);
}

// Test 12: for-await-of with non-async-iterable
async function testAsync() {
    for await (const x of 5) { // TS2504 (similar to TS2488 for async)
        console.log(x);
    }
}

// Test 13: Destructuring in for-of with non-iterable
for (const [a, b] of {}) { // TS2488
    console.log(a, b);
}

// Test 14: Nested for-of with non-iterable
for (const x of 42) {
    for (const y of x) { // Additional TS2488 on inner loop
        console.log(y);
    }
}

// Test 15: for-of with union type containing non-iterable
type MaybeIterable = number | string[];
for (const x of {} as MaybeIterable) { // TS2488
    console.log(x);
}
