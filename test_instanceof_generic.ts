// Test instanceof narrowing with generic type parameters
// This verifies the fix for the TypeParameter handling bug in are_object_like

class MyClass {
    x: number;
}

function testGenericConstraint<T extends { a: string }>(val: T) {
    // Before fix: val would be NEVER (are_object_like returned false for TypeParameter)
    // After fix: val should be T & MyClass (intersection)
    if (val instanceof MyClass) {
        // val should be T & MyClass here
        const x: number = val.x; // Should work - MyClass has x
        const a: string = val.a; // Should work - T constraint has a
    }
}

function testGenericUnconstrained<T>(val: T) {
    // Unconstrained generic - should still narrow to intersection
    if (val instanceof MyClass) {
        // val should be T & MyClass here
        const x: number = val.x; // Should work
    }
}

// Test with multiple type parameters
function testMultipleGenerics<T extends { foo: string }, U extends { bar: number }>(
    val1: T,
    val2: U
) {
    if (val1 instanceof MyClass && val2 instanceof MyClass) {
        // Both should be intersections
        const x1: number = val1.x;
        const x2: number = val2.x;
        const foo: string = val1.foo;
        const bar: number = val2.bar;
    }
}
