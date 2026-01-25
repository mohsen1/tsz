// Test for Rule #27: Homomorphic Mapped Types over Primitives
// This tests that mapped types over primitive types use their apparent types (String/Number/Boolean interfaces)

// Test 1: Mapped type over string primitive
type StringKeys = keyof string;
// Should include: "length" | "toString" | "charAt" | "charCodeAt" | "indexOf" | ...
// Plus NUMBER (for numeric index access)

type MappedString = { [K in keyof string]: boolean };
// Should create an object with boolean values for all String interface methods

// Test 2: Mapped type over number primitive
type NumberKeys = keyof number;
// Should include: "toString" | "toFixed" | "toExponential" | "toPrecision" | "valueOf" | ...

type MappedNumber = { [K in keyof number]: string };
// Should create an object with string values for all Number interface methods

// Test 3: Mapped type over boolean primitive
type BooleanKeys = keyof boolean;
// Should include: "toString" | "valueOf" | ...

type MappedBoolean = { [K in keyof boolean]: number };
// Should create an object with number values for all Boolean interface methods

// Test 4: Homomorphic mapped type over primitives
// Preserves property modifiers from the apparent type
type StringProperties = { [K in keyof string]: string[K] };
// Should preserve that 'length' is a number and methods are callable

// Test 5: Combination with utility types
type StringMethods = Pick<string, keyof string>;
// Should pick all methods/properties from String interface

// Test 6: Omit with primitives
type StringWithoutLength = Omit<string, "length">;
// Should create a type like String but without the length property

// Test 7: Partial with primitives
type PartialString = Partial<string>;
// Should make all String interface properties optional

// Test 8: Readonly with primitives
type ReadonlyString = Readonly<string>;
// Should make all String interface properties readonly

// Test 9: Required with primitives
type RequiredString = Required<string>;
// Should make all String interface properties required (none are optional in String)

// Test 10: Record with primitive keys
type StringRecord = Record<keyof string, boolean>;
// Should create a mapped type with all string keys and boolean values

// Type assertions to verify behavior
const test1: MappedString = {
  length: true,
  toString: true,
  charAt: true,
  charCodeAt: true,
  toUpperCase: true,
  toLowerCase: true,
  trim: true,
};

const test2: MappedNumber = {
  toString: "",
  toFixed: "",
  toExponential: "",
  toPrecision: "",
  valueOf: "",
};

const test3: MappedBoolean = {
  toString: 0,
  valueOf: 0,
};

// Verify that specific properties exist
type HasLength = { length: any };
type HasToString = { toString: any };

const test4: HasLength = { length: 42 };
const test5: HasToString = { toString: () => "" };

// Verify that mapped types over primitives include all expected keys
type AllStringKeys = "length" | "toString" | "charAt" | "charCodeAt" | "indexOf" | "lastIndexOf" | 
  "substring" | "substr" | "slice" | "toUpperCase" | "toLowerCase" | "trim" | 
  "trimLeft" | "trimRight" | "padStart" | "padEnd" | "concat" | "endsWith" | 
  "includes" | "indexOf" | "lastIndexOf" | "match" | "matchAll" | "replace" | 
  "search" | "slice" | "split" | "startsWith" | "substring" | "substr" | "toLocaleLowerCase" | 
  "toLocaleUpperCase" | "localeCompare" | "normalize" | "repeat" | "codePointAt";

console.log("Rule #27 homomorphic mapped types over primitives tests passed!");
