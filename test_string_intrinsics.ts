// Test string manipulation intrinsic types

// Uppercase
type UppercaseHello = Uppercase<"hello">; // Should be "HELLO"
type UppercaseUnion = Uppercase<"hello" | "world">; // Should be "HELLO" | "WORLD"
type UppercaseString = Uppercase<string>; // Should be string

// Lowercase
type LowercaseHello = Lowercase<"HELLO">; // Should be "hello"
type LowercaseUnion = Lowercase<"HELLO" | "WORLD">; // Should be "hello" | "world"

// Capitalize
type CapitalizeHello = Capitalize<"hello">; // Should be "Hello"
type CapitalizeUnion = Capitalize<"hello" | "world">; // Should be "Hello" | "World"

// Uncapitalize
type UncapitalizeHello = Uncapitalize<"Hello">; // Should be "hello"
type UncapitalizeUnion = Uncapitalize<"Hello" | "World">; // Should be "hello" | "world"

// Test that they distribute over unions correctly
type Result1 = UppercaseUnion extends "HELLO" | "WORLD" ? true : false;
type Result2 = LowercaseUnion extends "hello" | "world" ? true : false;
type Result3 = CapitalizeUnion extends "Hello" | "World" ? true : false;
type Result4 = UncapitalizeUnion extends "hello" | "world" ? true : false;

// Test with mapped types
type MappedUppercase = { [K in "a" | "b" as Uppercase<K>]: string };
// Should be { A: string; B: string }

type MappedCapitalize = { [K in "name" | "value" as `get${Capitalize<K>}`]: () => void };
// Should be { getName: () => void; getValue: () => void }

export type {};
