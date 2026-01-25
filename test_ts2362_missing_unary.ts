//! Test cases for missing TS2362 errors on unary operators.
//!
//! Expected behavior:
//! - Unary +, -, ~ operators should emit TS2362 when operand is not number/bigint/any/enum
//! - Currently, these operators are NOT validated in tsz

// Test unary + operator with invalid operands
const r1 = +"hello";       // Expected: TS2362
const r2 = +true;          // Expected: TS2362
const r3 = +{};            // Expected: TS2362
const r4 = +[];            // Expected: TS2362
const r5 = +undefined;     // Expected: TS2362
const r6 = +null;          // Expected: TS2362

// Test unary - operator with invalid operands
const r7 = -"hello";       // Expected: TS2362
const r8 = -true;          // Expected: TS2362
const r9 = -{};            // Expected: TS2362
const r10 = -[];           // Expected: TS2362
const r11 = -undefined;    // Expected: TS2362
const r12 = -null;         // Expected: TS2362

// Test unary ~ operator with invalid operands
const r13 = ~"hello";      // Expected: TS2362
const r14 = ~true;         // Expected: TS2362
const r15 = ~{};           // Expected: TS2362
const r16 = ~[];           // Expected: TS2362
const r17 = ~undefined;    // Expected: TS2362
const r18 = ~null;         // Expected: TS2362

// Test unary operators with valid operands (should NOT emit TS2362)
const valid1 = +42;        // OK
const valid2 = -42;        // OK
const valid3 = ~42;        // OK
const valid4 = +42n;       // OK
const valid5 = -42n;       // OK
const valid6 = ~42n;       // OK
const valid7 = +anyVar as any;    // OK - any is always valid

// Test in expressions
const expr1 = +"5" - 1;    // Expected: TS2362 for +"5"
const expr2 = -true * 2;   // Expected: TS2362 for -true
const expr3 = ~"x" | 5;    // Expected: TS2362 for ~"x"

// Test with variables
let str = "hello";
let obj = { a: 1 };
const unary1 = +str;       // Expected: TS2362
const unary2 = -obj;       // Expected: TS2362
const unary3 = ~str;       // Expected: TS2362

// Test with function calls
function returnsString(): string { return "hello"; }
const r1a = +returnsString();  // Expected: TS2362

function returnsObject(): { a: number } { return { a: 1 }; }
const r2a = -returnsObject();  // Expected: TS2362

// Test with enums (should be valid)
enum MyEnum {
    A = 1,
    B = 2
}
const enumValid1 = +MyEnum.A;  // OK - enum is number-like
const enumValid2 = -MyEnum.B;  // OK - enum is number-like

// Test string enum (should emit TS2362 for unary ops)
enum StringEnum {
    A = "a",
    B = "b"
}
const r3a = +StringEnum.A;  // Expected: TS2362
const r4a = -StringEnum.B;  // Expected: TS2362
const r5a = ~StringEnum.A;  // Expected: TS2362

// Test in complex expressions
const complex1 = (+("x" as string)) + 1;  // Expected: TS2362 for +"x"
const complex2 = [1, 2, 3].map(x => -"y"); // Expected: TS2362 for -"y"
