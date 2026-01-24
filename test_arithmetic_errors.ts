// Test file for TS2362/TS2363 - Arithmetic operations on non-numeric types

// ============================================================================
// Test 1: Subtraction on strings
// ============================================================================

const a = "hello" - "world"; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 2: Multiplication on strings
// ============================================================================

const b = "hello" * 5; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 3: Division on strings
// ============================================================================

const c = "hello" / 5; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 4: Modulo on strings
// ============================================================================

const d = "hello" % 5; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 5: Exponentiation on strings
// ============================================================================

const e = "hello" ** 2; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 6: Right-hand side errors
// ============================================================================

const f = 5 - "hello"; // TS2363: Right-hand side must be number/bigint/any/enum

// ============================================================================
// Test 7: Both operands wrong type
// ============================================================================

const g = "hello" - "world"; // TS2362 on left, TS2363 on right

// ============================================================================
// Test 8: Object types
// ============================================================================

interface Foo {
    x: number;
}

const h: Foo = { x: 1 };
const i = h - 5; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 9: Array types
// ============================================================================

const arr = [1, 2, 3];
const j = arr - 5; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 10: Boolean types
// ============================================================================

const k = true - false; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 11: Null and undefined
// ============================================================================

const l = null - 5; // TS2362: Left-hand side must be number/bigint/any/enum
const m = undefined - 5; // TS2362: Left-hand side must be number/bigint/any/enum

// ============================================================================
// Test 12: Interface types
// ============================================================================

interface Bar {
    value: number;
}

function bar(x: Bar) {
    return x - 5; // TS2362: Left-hand side must be number/bigint/any/enum
}

// ============================================================================
// Test 13: Type parameters
// ============================================================================

function generic<T>(x: T) {
    return x - 5; // Should error if T is not constrained to number
}

// ============================================================================
// Test 14: Union types with strings
// ============================================================================

type StringOrNumber = string | number;

const n: StringOrNumber = "hello";
const o = n - 5; // Should this error? In TS, unions are checked element-wise

// ============================================================================
// Test 15: Enum types (should work)
// ============================================================================

enum Color {
    Red,
    Green,
    Blue
}

const p = Color.Red - 1; // Should work - enums are numeric

// ============================================================================
// Test 16: BigInt operations (should work with bigint)
// ============================================================================

const q = 100n - 50n; // Should work - bigint operations

// ============================================================================
// Test 17: Mixed number and bigint (should error with TS2365)
// ============================================================================

const r = 5 - 100n; // TS2365: Operator '-' cannot be applied to types 'number' and 'bigint'

// ============================================================================
// Test 18: Any type (should work)
// ============================================================================

const s: any = "something";
const t = s - 5; // Should work - any allows all operations

console.log("Arithmetic error tests complete");
