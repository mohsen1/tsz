// Test case for TS2345 fix: Array literal argument error elaboration

// Case 1: Array literal with wrong element types
// TypeScript emits TS2322 for each element, NOT TS2345 for the argument
function foo2(x: number[]) { }
foo2(["s", "t"]);  // Should emit TS2322 for "s" and "t", NOT TS2345

// Case 2: Non-array-literal argument with wrong type
// Should emit TS2345 for the whole argument
const wrongArray: string[] = ["a", "b"];
foo2(wrongArray);  // Should emit TS2345

// Case 3: Function parameter with callback
// Should emit TS2345 for the callback parameter
function foo3(x: (n: number) => number) { }
foo3((s: string) => { return 0; });  // Should emit TS2345

// Case 4: Empty array
foo2([]);  // Should NOT emit any error (can be inferred as number[])

// Case 5: Mixed types in array
function foo4(x: (string | number)[]) { }
foo4([1, "hello", true]);  // Should emit TS2322 for the boolean element
