class MyClass {
    x: number;
}

function testGenericUnconstrained<T>(val: T) {
    if (val instanceof MyClass) {
        // val should be T & MyClass here
        const x: number = val.x; // Should work
    }
}
