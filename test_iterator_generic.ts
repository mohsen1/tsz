// Test TS2488 for type parameters
function testGeneric<T>(x: T) {
    for (const item of x) {  // Should emit TS2488: Type 'T' must have a '[Symbol.iterator]()' method
        console.log(item);
    }
}

// Test with constrained type parameter that IS iterable
function testGenericArray<T extends any[]>(x: T) {
    for (const item of x) {  // Should NOT emit TS2488
        console.log(item);
    }
}

// Test with constrained type parameter that is NOT iterable
function testGenericNumber<T extends number>(x: T) {
    for (const item of x) {  // Should emit TS2488
        console.log(item);
    }
}

// Test with intersection
type IterableNumber = number & Iterable<number>;
function testIntersection(x: IterableNumber) {
    for (const item of x) {  // Should NOT emit TS2488 (at least one member is iterable)
        console.log(item);
    }
}

// Test with plain intersection of non-iterables
type Intersection = { a: number } & { b: string };
function testNonIterableIntersection(x: Intersection) {
    for (const item of x) {  // Should emit TS2488
        console.log(item);
    }
}
