// Test string literal property access
const str = "hello";
str.unknownProperty; // Should emit TS2339
str.length; // Should work
str.charAt(0); // Should work

// Test number literal property access
const num = 42;
num.unknownProperty; // Should emit TS2339
num.toFixed(2); // Should work

// Test boolean literal property access
const bool = true;
bool.unknownProperty; // Should emit TS2339
bool.toString(); // Should work

// Test bigint literal property access
const big = 123n;
big.unknownProperty; // Should emit TS2339
big.toString(); // Should work

// Test intrinsic types
type UppercaseHello = Uppercase<"hello">;
type Result = UppercaseHello["length"]; // Should work
