// =============================================================================
// Type Narrowing Tests
// =============================================================================
// This file tests the type narrowing implementation based on control flow analysis

// =============================================================================
// 1. Typeof Narrowing
// =============================================================================

function test_typeof_string(x: string | number) {
    if (typeof x === "string") {
        // x should be narrowed to string
        return x.length; // OK
    } else {
        // x should be narrowed to number
        return x.toFixed(2); // OK
    }
}

function test_typeof_number(x: string | number) {
    if (typeof x === "number") {
        return x.toFixed(2); // OK - x is number
    } else {
        return x.length; // OK - x is string
    }
}

function test_typeof_boolean(x: boolean | number) {
    if (typeof x === "boolean") {
        return x; // OK - x is boolean
    } else {
        return x.toFixed(2); // OK - x is number
    }
}

// =============================================================================
// 2. Instanceof Narrowing
// =============================================================================

function test_instanceof_date(x: Date | string) {
    if (x instanceof Date) {
        return x.getTime(); // OK - x is Date
    } else {
        return x.length; // OK - x is string
    }
}

function test_instanceof_error(x: Error | number) {
    if (x instanceof Error) {
        return x.message; // OK - x is Error
    } else {
        return x.toFixed(2); // OK - x is number
    }
}

// =============================================================================
// 3. Property Access Narrowing
// =============================================================================

function test_property_access_optional(x: { a?: string }) {
    if (x.a) {
        // x.a should be narrowed to string (not string | undefined)
        return x.a.length; // OK
    }
    return 0;
}

function test_property_access_nullish(x: { a: string | null }) {
    if (x.a) {
        // x.a should be narrowed to string
        return x.a.length; // OK
    }
    return 0;
}

// =============================================================================
// 4. Equality Narrowing
// =============================================================================

function test_equality_string(x: string | number, y: string) {
    if (x === y) {
        // x should be narrowed to string
        return x.length; // OK
    }
    return 0;
}

function test_equality_literal(x: "a" | "b" | number) {
    if (x === "a") {
        // x should be narrowed to "a"
        return x; // OK
    } else {
        return x; // OK - "b" | number
    }
}

// =============================================================================
// 5. In Operator Narrowing
// =============================================================================

function test_in_operator(x: { a: string } | { b: number }) {
    if ("a" in x) {
        return x.a; // OK - x is { a: string }
    } else {
        return x.b.toFixed(2); // OK - x is { b: number }
    }
}

function test_in_operator_union(x: { name: string } | { age: number } | { active: boolean }) {
    if ("name" in x) {
        return x.name.toUpperCase(); // OK - x is { name: string }
    } else if ("age" in x) {
        return x.age.toFixed(2); // OK - x is { age: number }
    } else {
        return x.active; // OK - x is { active: boolean }
    }
}

// =============================================================================
// 6. Discriminated Union Narrowing
// =============================================================================

type Action =
    | { type: "add"; value: number }
    | { type: "remove"; id: string }
    | { type: "clear" };

function test_discriminated_union(action: Action) {
    if (action.type === "add") {
        return action.value.toFixed(2); // OK - action is { type: "add"; value: number }
    } else if (action.type === "remove") {
        return action.id.toUpperCase(); // OK - action is { type: "remove"; id: string }
    } else {
        return action.type; // OK - action is { type: "clear" }
    }
}

// =============================================================================
// 7. Truthiness Narrowing
// =============================================================================

function test_truthiness_null(x: string | null) {
    if (x) {
        return x.length; // OK - x is string
    }
    return 0;
}

function test_truthiness_undefined(x: number | undefined) {
    if (x) {
        return x.toFixed(2); // OK - x is number
    }
    return 0;
}

// =============================================================================
// 8. Logical Operator Narrowing
// =============================================================================

function test_logical_and(x: string | null, y: number | undefined) {
    if (x && y) {
        // x should be narrowed to string
        // y should be narrowed to number
        return x.length + y.toFixed(2).length; // OK
    }
    return "";
}

function test_logical_or(x: string | null) {
    const y = x || "default";
    return y.length; // OK - y is string
}

// =============================================================================
// 9. Nested Control Flow
// =============================================================================

function test_nested_narrowing(x: string | number | boolean) {
    if (typeof x === "string") {
        return x.length; // OK - x is string
    } else if (typeof x === "number") {
        return x.toFixed(2); // OK - x is number
    } else {
        return x; // OK - x is boolean
    }
}

function test_nested_switch(x: "a" | "b" | "c") {
    switch (x) {
        case "a":
            return x.toUpperCase(); // OK - x is "a"
        case "b":
            return x.toUpperCase(); // OK - x is "b"
        default:
            return x.toUpperCase(); // OK - x is "c"
    }
}

// =============================================================================
// 10. Complex Scenarios
// =============================================================================

function test_complex_narrowing(value: unknown) {
    if (typeof value === "string") {
        return value.length; // OK - value is string
    } else if (typeof value === "number") {
        return value.toFixed(2); // OK - value is number
    } else if (typeof value === "boolean") {
        return value; // OK - value is boolean
    }
    return null;
}

function test_array_narrowing(arr: (string | number)[]) {
    const first = arr[0];
    if (typeof first === "string") {
        return first.length; // OK - first is string
    }
    return 0;
}

console.log("All type narrowing tests compiled successfully!");
