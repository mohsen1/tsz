// Test TypeScript's behavior with unconstrained type parameters
function test1<T>(x: T) {
    for (const item of x) {  // What does TS say here?
        console.log(item);
    }
}

// Test with constrained type parameters
function test2<T extends any[]>(x: T) {
    for (const item of x) {  // Should be OK
        console.log(item);
    }
}

// Test with extends unknown
function test3<T extends unknown>(x: T) {
    for (const item of x) {  // What does TS say here?
        console.log(item);
    }
}
