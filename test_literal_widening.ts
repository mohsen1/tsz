// Test literal widening - should NOT emit TS2322 errors

// Test 1: String literal to string
let str: string = "hello";  // Should be OK - literal widens to string

// Test 2: Number literal to number
let num: number = 42;  // Should be OK - literal widens to number

// Test 3: Boolean literal to boolean
let bool: boolean = true;  // Should be OK - literal widens to boolean

// Test 4: Literal in function call
function takesString(s: string) {
    return s;
}

takesString("hello");  // Should be OK - "hello" widens to string

// Test 5: Multiple number literals
function takesNumber(n: number) {
    return n;
}

takesNumber(42);  // Should be OK - 42 widens to number

// Test 6: Template literal
let template: string = `hello`;  // Should be OK

// Test 7: Const assertion keeps literal type
const x = "hello" as const;  // Type is "hello", not string
let y: "hello" = x;  // Should be OK - same literal type

// Test 8: Const declaration widens
const z = "hello";  // Type is "hello" (const keeps literal)
let w: string = z;  // Should be OK - "hello" widens to string

// Test 9: Let declaration should widen
let a = "hello";  // Type should be string, not "hello"
let b: string = a;  // Should be OK

// Test 10: Function return type
function returnString(): string {
    return "hello";  // Should be OK - literal widens to string
}

// Test 11: Array of literals
function stringArray(): string[] {
    return ["a", "b", "c"];  // Should be OK - literals widen to string
}

// Test 12: Union with literal
function stringOrNumber(x: string | number) {
    return x;
}

stringOrNumber("hello");  // Should be OK
stringOrNumber(42);  // Should be OK

console.log("All tests passed");
