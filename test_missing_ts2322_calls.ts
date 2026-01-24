// Test cases for missing TS2322 errors on function call arguments

// Test 1: Function with object parameter receiving wrong type (non-overloaded)
function takesObject(obj: { a: number }) {
    return obj.a;
}

// This should emit TS2322 but might not if weak union check is too aggressive
takesObject({ a: "string" }); // Should error: string not assignable to number

// Test 2: Overloaded function with wrong argument type
interface Overloaded {
    (x: number): void;
    (x: string): void;
}

declare const overloaded: Overloaded;
overloaded({ value: 42 }); // Should error: object not assignable to number or string

// Test 3: Generic function with type mismatch
function generic<T>(x: T): T {
    return x;
}

generic<number>("string"); // Should error: string not assignable to number

// Test 4: Function with optional parameter
function withOptional(x: number, y?: string) {
    return x;
}

withOptional(42, 123); // Should error: number not assignable to string

// Test 5: Spread operator with wrong type
function takesMultiple(...args: string[]) {
    return args.join(", ");
}

const numbers = [1, 2, 3];
takesMultiple(...numbers); // Should error: number not assignable to string

// Test 6: Object literal with excess properties (should show TS2353, not TS2322)
function takesStrictObject(obj: { a: number }) {
    return obj.a;
}

takesStrictObject({ a: 42, b: "extra" }); // Should show excess property error

// Test 7: Union parameter type
function takesUnion(param: string | number) {
    return param;
}

takesUnion(true); // Should error: boolean not assignable to string | number

// Test 8: Nested object type mismatch
interface Nested {
    value: {
        nested: number
    }
}

function takesNested(obj: Nested) {
    return obj.value.nested;
}

takesNested({ value: { nested: "string" } }); // Should error

// Test 9: Array parameter type
function takesArray(arr: number[]) {
    return arr.reduce((a, b) => a + b, 0);
}

takesArray([1, 2, "three"]); // Should error: string not assignable to number

// Test 10: Tuple parameter type
function takesTuple(tuple: [number, string]) {
    return tuple[0] + tuple[1];
}

takesTuple(["wrong", 42]); // Should error
