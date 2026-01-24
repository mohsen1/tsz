// Test generic function with explicit type argument

function generic<T>(x: T): T {
    return x;
}

// Should error: string is not assignable to number
const result = generic<number>("string");

console.log(result);
