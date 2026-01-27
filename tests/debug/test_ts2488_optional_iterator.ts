// Test TS2488 for optional Symbol.iterator
// Optional Symbol.iterator should NOT make a type iterable

// Case 1: Optional Symbol.iterator in object type
declare var iterableWithOptionalIterator: {
    [Symbol.iterator]?(): Iterator<string>
};

for (const v of iterableWithOptionalIterator) {} // Should emit TS2488

// Case 2: Required Symbol.iterator (should NOT emit TS2488)
declare var iterableWithRequiredIterator: {
    [Symbol.iterator](): Iterator<string>
};

for (const v of iterableWithRequiredIterator) {} // Should NOT emit TS2488 (if Iterator is properly defined)

// Case 3: Optional Symbol.iterator in spread
const arr1 = [...iterableWithOptionalIterator]; // Should emit TS2488

// Case 4: Optional Symbol.iterator in destructuring
const [a, b] = iterableWithOptionalIterator; // Should emit TS2488
