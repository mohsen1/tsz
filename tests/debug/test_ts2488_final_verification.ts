// Final verification test for TS2488 iterator protocol error checks
// This test verifies that TS2488 is emitted in all required iteration contexts

// ============================================================================
// Test Case from Task Description
// ============================================================================

const notIterable = { x: 1 };

// 1. Spread operator
const arr = [...notIterable];  // Should error TS2488

// 2. For-of loops
for (const x of notIterable) {}  // Should error TS2488

// 3. Destructuring
const [a, b] = notIterable;  // Should error TS2488

// ============================================================================
// Additional Verification Tests
// ============================================================================

// 4. Spread in function calls (also needs TS2488)
function testFn(a: number, b: number) {}
testFn(...notIterable);  // Should error TS2488

// 5. Multiple spreads
const arr2 = [...notIterable, ...notIterable];  // Should error TS2488 (twice)

// 6. Nested destructuring
const [[c, d]] = [notIterable];  // Should error TS2488

// 7. Rest element in destructuring
const [first, ...rest] = notIterable;  // Should error TS2488 (twice - once for pattern, once for initializer)

// 8. Null cases
const arr3 = [...null];  // Should error TS2488
for (const y of null) {}  // Should error TS2488
const [e, f] = null;  // Should error TS2488 (twice)

// 9. Valid iterables should NOT error
const validArray = [1, 2, 3];
const arr4 = [...validArray];  // OK - no error
for (const z of validArray) {}  // OK - no error
const [g, h, i] = validArray;  // OK - no error

// 10. String is iterable (should NOT error)
const str = "hello";
const arr5 = [...str];  // OK - no error
for (const ch of str) {}  // OK - no error
const [j, k] = str;  // OK - no error

console.log("TS2488 verification complete");
