// Test to verify narrowing behavior more precisely

// Test 1: Verify typeof exclusion (!==) narrowing is broken
function test_typeof_exclusion_broken() {
    let x: string | number = (Math as any).random() > 0.5 ? "hello" : 42;
    if (typeof x !== "string") {
        // x should be number here (exclusion of string)
        let y: number = x; // tsz ERRORs, tsc OK
    }
}

// Test 2: Verify typeof positive (===) narrowing works
function test_typeof_positive_works() {
    let x: string | number = (Math as any).random() > 0.5 ? "hello" : 42;
    if (typeof x === "string") {
        // x should be string here
        let y: string = x; // Both OK
    } else {
        // x should be number here
        let z: number = x; // tsz ERRORs, tsc OK (because exclusion is broken)
    }
}

console.log("Done");
