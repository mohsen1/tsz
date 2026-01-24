// Test file for TS2488 edge cases

// Test 1: any type (should not error - any is permissive)
const [a1] = {} as any; // Should NOT error - any is permissive

// Test 2: unknown type (should not error - unknown is permissive)
const [a2] = {} as unknown; // Should NOT error - unknown is permissive

// Test 3: never type (should not error)
const [a3] = {} as never; // Should NOT error - never is special

// Test 4: Array type (should work)
const arr: number[] = [1, 2, 3];
const [a4] = arr; // Should NOT error - arrays are iterable

// Test 5: Tuple type (should work)
const tuple: [string, number] = ["hello", 42];
const [a5, a6] = tuple; // Should NOT error - tuples are iterable

// Test 6: String type (should work)
const str: string = "hello";
const [a7] = str; // Should NOT error - strings are iterable

// Test 7: String literal (should work)
const [a8] = "world"; // Should NOT error - string literals are iterable

// Test 8: Readonly array (should work if readonly is unwrapped)
const readonlyArr: readonly number[] = [1, 2, 3];
const [a9] = readonlyArr; // Should NOT error - readonly arrays are iterable

// Test 9: Spread with tuple
const tuple2: [number, string, boolean] = [1, "x", true];
const arr1 = [...tuple2]; // Should NOT error - tuples are iterable

// Test 10: Spread with readonly array
const readonlyArr2: readonly string[] = ["a", "b"];
const arr2 = [...readonlyArr2]; // Should NOT error - readonly arrays are iterable

// Test 11: Nested destructuring with valid types
const arr3: number[][] = [[1, 2], [3, 4]];
const [[b1, b2]] = arr3; // Should NOT error

// Test 12: Rest pattern with valid iterable
const numbers = [1, 2, 3, 4, 5];
const [first, ...rest] = numbers; // Should NOT error

// Test 13: Object with Symbol.iterator (should work)
const iterableObj = {
    [Symbol.iterator]: function* () {
        yield 1;
        yield 2;
    }
};
const [c1] = iterableObj; // Should NOT error - has Symbol.iterator

// Test 14: Object with 'next' method (iterator-like)
const iteratorObj = {
    next: function() {
        return { value: 1, done: false };
    }
};
const [c2] = iteratorObj; // Should NOT error - has 'next' method

// Test 15: Destructuring in function parameter
function foo([a, b]: number[]) {
    return a + b;
}
foo([1, 2]); // Should NOT error
// foo(42); // Would error if called with non-iterable

// Test 16: Destructuring in for-of with valid iterable
const nums = [1, 2, 3];
for (const [x, y] of [nums] as [number[]]) {
    console.log(x, y);
}

// Test 17: Empty array destructuring
const empty: [] = [];
const [] = empty; // Should NOT error

// Test 18: Omitted elements in destructuring
const [d1, , d3] = [1, 2, 3]; // Should NOT error
