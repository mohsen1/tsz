// Test file for TS2488 - Union type iterability checks
// A union type is only iterable if ALL members are iterable

// Test 1: Union of iterable types (should work - no error)
const arr1: (string[] | number[]) = [1, 2, 3];
const [x1] = arr1; // Should NOT error - both arrays are iterable

// Test 2: Union with one non-iterable member
type MaybeArray = number[] | number;
const val1: MaybeArray = 42;
const [x2] = val1; // TS2488 - number is not iterable

// Test 3: Union with object
type StringOrObject = string[] | { x: number };
const val2: StringOrObject = { x: 1 };
const [x3] = val2; // TS2488 - object is not iterable

// Test 4: Union of all iterables (should work)
type AllIterable = string[] | number[] | boolean[];
const val3: AllIterable = [true, false];
const [x4] = val3; // Should NOT error

// Test 5: Spread with union containing non-iterable
type MaybeIterable2 = number | string[];
const val4: MaybeIterable2 = 99;
const arr2 = [...val4]; // TS2488

// Test 6: for-of with union containing non-iterable
type MaybeIterable3 = Set<number> | boolean;
const val5: MaybeIterable3 = false;
for (const x of val5) { // TS2488
    console.log(x);
}

// Test 7: Multiple union members
type ComplexUnion = string[] | number[] | null;
const val6: ComplexUnion = null;
const [x5] = val6; // TS2488 - null is not iterable

// Test 8: Intersection type - at least one must be iterable
type IntersectionType = { x: number } & string[];
const val7: IntersectionType = { x: 1 } as any; // Would need proper intersection type
// const [x6] = val7; // Implementation depends on intersection handling

// Test 9: Nested union in array destructuring
type NestedUnion = (number | string[])[]; // Array of unions
const val8: NestedUnion = [[1, 2]];
const [[y1]] = val8; // Should NOT error - array is iterable

// Test 10: Union with custom iterable
interface CustomIterable {
    [Symbol.iterator](): Iterator<number>;
}
type IterableUnion = CustomIterable | number[];
const val9: IterableUnion = { [Symbol.iterator]: function* () { yield 1; } };
const [x6] = val9; // Should NOT error - both are iterable
