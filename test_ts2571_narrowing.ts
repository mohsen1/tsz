// Test cases for TS2571 narrowing fixes

// Test 1: typeof narrowing on unknown
function testTypeofNarrowing(val: unknown) {
    if (typeof val === "string") {
        console.log(val.toUpperCase()); // Should NOT error - narrowed to string
        console.log(val.length); // Should NOT error
    }
    if (typeof val === "number") {
        console.log(val.toFixed(2)); // Should NOT error - narrowed to number
    }
    if (typeof val === "boolean") {
        console.log(val ? "yes" : "no"); // Should NOT error - narrowed to boolean
    }
}

// Test 2: instanceof narrowing on unknown
try {
    throw new Error("test");
} catch (error: unknown) {
    if (error instanceof Error) {
        console.log(error.message); // Should NOT error - narrowed to Error
        console.log(error.stack); // Should NOT error
    }
}

// Test 3: in operator narrowing on unknown
function testInOperator(obj: unknown) {
    if ("prop" in obj) {
        console.log(obj.prop); // Should NOT error - narrowed to object
    }
    if ("length" in obj) {
        console.log(obj.length); // Should NOT error
    }
}

// Test 4: Falsy narrowing on unknown
function testFalsyNarrowing(val: unknown) {
    if (val) {
        // narrowed to non-falsy types
        console.log(val); // Should NOT error
    } else {
        // narrowed to falsy types (null | undefined | false | "" | 0 | 0n)
        console.log(val); // Should NOT error
    }
}

// Test 5: Combined type guards on catch variable
try {
    throw { message: "error", code: 500 };
} catch (e: unknown) {
    if (typeof e === "object" && e !== null && "message" in e) {
        console.log(e.message); // Should NOT error
        if ("code" in e) {
            console.log(e.code); // Should NOT error
        }
    }
}

// Test 6: typeof "object" narrows unknown to object | null
function testObjectishNarrowing(val: unknown) {
    if (typeof val === "object") {
        if (val !== null) {
            console.log(val.toString()); // Should NOT error - narrowed to object
        }
    }
}

// Test 7: Array narrowing with typeof
function testArrayNarrowing(val: unknown) {
    if (typeof val === "object" && val !== null && Array.isArray(val)) {
        console.log(val.length); // Should NOT error - narrowed to array
    }
}

// Test 8: Union narrowing with unknown
function testUnionWithUnknown(val: string | unknown) {
    if (typeof val === "string") {
        console.log(val.toUpperCase()); // Should NOT error
    }
}

// Test 9: Multiple type guards
function testMultipleGuards(val: unknown) {
    if (typeof val === "object") {
        // val: object | null
        if (val !== null) {
            // val: object
            if (val instanceof Date) {
                // val: Date
                console.log(val.toISOString()); // Should NOT error
            }
        }
    }
}

// Test 10: Type guard in function parameter
function processValue(val: unknown) {
    if (typeof val === "string") {
        return val.toUpperCase();
    }
    if (typeof val === "number") {
        return val * 2;
    }
    if (typeof val === "boolean") {
        return val ? 1 : 0;
    }
    return null;
}
