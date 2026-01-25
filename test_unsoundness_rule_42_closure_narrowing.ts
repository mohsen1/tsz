// Unsoundness Rule #42: CFA Invalidation in Closures
// Mutable variables (let/var) should NOT preserve narrowing from outer scope when accessed in closures

// Test 1: let variable should NOT preserve narrowing in closure
function testLetVariableInClosure() {
    let x: string | number;

    if (typeof x === "string") {
        // At this point, x is narrowed to string
        // But in the closure, it should revert to string | number
        const fn = () => {
            // x should NOT be narrowed here - it's mutable
            // This should ERROR because x might be number
            // @ts-expect-error - x is not guaranteed to be string in closure
            console.log(x.length);
        };
    }
}

// Test 2: const variable SHOULD preserve narrowing in closure
function testConstVariableInClosure() {
    const x: string | number = Math.random() > 0.5 ? "hello" : 42;

    if (typeof x === "string") {
        // At this point, x is narrowed to string
        // In the closure, it should remain narrowed to string (const is immutable)
        const fn = () => {
            // x SHOULD be narrowed here - it's const and immutable
            // This should NOT error
            console.log(x.length); // OK - x is definitely string
        };
    }
}

// Test 3: var variable should NOT preserve narrowing in closure
function testVarVariableInClosure() {
    var x: string | number;

    if (typeof x === "string") {
        const fn = () => {
            // x should NOT be narrowed - var is mutable
            // @ts-expect-error - x is not guaranteed to be string in closure
            console.log(x.length);
        };
    }
}

// Test 4: Nested closures
function testNestedClosures() {
    let x: string | number;

    if (typeof x === "string") {
        const outer = () => {
            const inner = () => {
                // x should NOT be narrowed in either closure
                // @ts-expect-error - x is not guaranteed to be string
                console.log(x.length);
            };
        };
    }
}

// Test 5: Const in nested closures should preserve narrowing
function testConstInNestedClosures() {
    const x: string | number = Math.random() > 0.5 ? "hello" : 42;

    if (typeof x === "string") {
        const outer = () => {
            const inner = () => {
                // x SHOULD be narrowed in both closures
                console.log(x.length); // OK - x is definitely string
            };
        };
    }
}

// Test 6: Closure that modifies the variable
function testClosureModifiesVariable() {
    let x: string | number = "hello";

    if (typeof x === "string") {
        const fn = () => {
            // This is why we can't trust narrowing for let:
            // The closure could reassign x
            x = 42; // This is valid!

            // @ts-expect-error - x might have been reassigned to number
            console.log(x.length);
        };
    }
}

// Test 7: const cannot be reassigned, so narrowing is safe
function testConstCannotBeReassigned() {
    const x: string | number = "hello";

    if (typeof x === "string") {
        const fn = () => {
            // x = 42; // This would be a compile error - const can't be reassigned

            // Since x can't be reassigned, narrowing is safe
            console.log(x.length); // OK
        };
    }
}

// Test 8: Arrow functions
function testArrowFunctionNarrowing() {
    let x: string | number;

    if (typeof x === "string") {
        // Arrow function is a closure
        const fn = () => {
            // @ts-expect-error - x is not guaranteed to be string
            console.log(x.length);
        };
    }
}

// Test 9: Function expressions
function testFunctionExpressionNarrowing() {
    let x: string | number;

    if (typeof x === "string") {
        // Function expression is also a closure
        const fn = function() {
            // @ts-expect-error - x is not guaranteed to be string
            console.log(x.length);
        };
    }
}

// Test 10: Immediately invoked closure
function testIIFENarrowing() {
    let x: string | number;

    if (typeof x === "string") {
        // IIFE is a closure too
        (() => {
            // @ts-expect-error - x is not guaranteed to be string
            console.log(x.length);
        })();
    }
}

console.log("Unsoundness Rule #42 tests complete");
