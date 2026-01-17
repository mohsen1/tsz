// =================================================================
// TS2322 ASSIGNMENT TYPE MISMATCH TESTS
// Tests for assignment type compatibility (Type 'X' is not assignable to type 'Y')
//
// These tests verify that changing the solver fallback from Any to Unknown
// properly emits TS2322 errors for assignment type mismatches.
// =================================================================

// @strict: true
// @noEmit: true

// =================================================================
// SECTION 1: PRIMITIVE TYPE ASSIGNMENTS (TS2322)
// The most basic assignment type mismatches
// =================================================================

// @ts-expect-error - Type 'number' is not assignable to type 'string'
let numToStr: string = 123;

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let strToNum: number = "hello";

// @ts-expect-error - Type 'boolean' is not assignable to type 'string'
let boolToStr: string = true;

// @ts-expect-error - Type 'string' is not assignable to type 'boolean'
let strToBool: boolean = "true";

// Valid assignments should NOT error
let validNum: number = 42;
let validStr: string = "hello";
let validBool: boolean = true;

// =================================================================
// SECTION 2: GENERIC TYPE ASSIGNMENTS WITH WRONG TYPES (TS2322)
// Test that generic types with incorrect type parameters emit errors
// =================================================================

interface Box<T> {
    value: T;
}

// @ts-expect-error - Type 'Box<string>' is not assignable to type 'Box<number>'
let boxWrong: Box<number> = { value: "string" };

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let boxGenericWrong: Box<number> = { value: 42 } as Box<string>;

// Valid generic assignment
let boxValid: Box<string> = { value: "test" };

// =================================================================
// SECTION 3: GENERIC FUNCTION TYPE MISMATCH (TS2322)
// Test that generic functions with wrong type arguments emit errors
// =================================================================

function identity<T>(value: T): T {
    return value;
}

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let genericWrong: number = identity<string>("hello");

// Valid generic usage
let genericValid: string = identity<string>("test");

// =================================================================
// SECTION 4: COMPLEX GENERIC CONSTRAINTS (TS2322)
// Test generic constraints with wrong types
// =================================================================

interface WithLength {
    length: number;
}

function getLength<T extends WithLength>(arg: T): number {
    return arg.length;
}

// @ts-expect-error - Argument of type 'number' is not assignable to parameter of type 'T'
getLength(42);

// Valid constraint usage
getLength("hello");
getLength([1, 2, 3]);

// =================================================================
// SECTION 5: UNION TYPE ASSIGNMENTS (TS2322)
// Test that non-union members emit errors
// =================================================================

type StringOrNumber = string | number;

// @ts-expect-error - Type 'boolean' is not assignable to type 'StringOrNumber'
let unionWrong: StringOrNumber = true;

// @ts-expect-error - Type 'undefined' is not assignable to type 'StringOrNumber'
let unionWrong2: StringOrNumber = undefined;

// Valid union assignments
let unionValid1: StringOrNumber = "hello";
let unionValid2: StringOrNumber = 42;

// =================================================================
// SECTION 6: INTERSECTION TYPE ASSIGNMENTS (TS2322)
// Test that partial intersections emit errors
// =================================================================

type A = { a: string };
type B = { b: number };
type AB = A & B;

// @ts-expect-error - Property 'b' is missing
let intersectionWrong: AB = { a: "test" };

// @ts-expect-error - Property 'a' is missing
let intersectionWrong2: AB = { b: 42 };

// Valid intersection
let intersectionValid: AB = { a: "test", b: 42 };

// =================================================================
// SECTION 7: ARRAY TYPE ASSIGNMENTS (TS2322)
// Test array type mismatches
// =================================================================

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let arrayWrong: number[] = [1, 2, "three"];

// @ts-expect-error - Type 'string[]' is not assignable to type 'number[]'
let arrayWrong2: number[] = ["1", "2", "3"];

// Valid array assignments
let arrayValid: number[] = [1, 2, 3];
let arrayValid2: string[] = ["a", "b", "c"];

// =================================================================
// SECTION 8: OBJECT TYPE ASSIGNMENTS (TS2322)
// Test object property type mismatches
// =================================================================

interface Point {
    x: number;
    y: number;
}

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let pointWrong: Point = { x: 10, y: "20" };

// @ts-expect-error - Property 'y' is missing
let pointWrong2: Point = { x: 10 };

// Valid object assignment
let pointValid: Point = { x: 10, y: 20 };

// =================================================================
// SECTION 9: FUNCTION TYPE ASSIGNMENTS (TS2322)
// Test function parameter/return type mismatches
// =================================================================

type NumToStr = (x: number) => string;

// @ts-expect-error - Type '(x: string) => string' is not assignable to type 'NumToStr'
let funcWrong: NumToStr = (x: string) => x;

// @ts-expect-error - Type '(x: number) => number' is not assignable to type 'NumToStr'
let funcWrong2: NumToStr = (x: number) => x;

// Valid function type assignment
let funcValid: NumToStr = (x: number) => x.toString();

// =================================================================
// SECTION 10: TUPLE TYPE ASSIGNMENTS (TS2322)
// Test tuple type mismatches
// =================================================================

type Tuple = [string, number];

// @ts-expect-error - Type 'number' is not assignable to type 'string'
let tupleWrong: Tuple = [42, "hello"];

// @ts-expect-error - Property '1' is missing
let tupleWrong2: Tuple = ["hello"];

// Valid tuple assignment
let tupleValid: Tuple = ["hello", 42];

// =================================================================
// SECTION 11: LITERAL TYPE ASSIGNMENTS (TS2322)
// Test literal type mismatches
// =================================================================

type Direction = "north" | "south" | "east" | "west";

// @ts-expect-error - Type '"northeast"' is not assignable to type 'Direction'
let literalWrong: Direction = "northeast";

// @ts-expect-error - Type 'string' is not assignable to type 'Direction'
let literalWrong2: Direction = "north" as string;

// Valid literal assignment
let literalValid: Direction = "north";

// =================================================================
// SECTION 12: ASYNC/AWAIT TYPE ASSIGNMENTS (TS2322)
// Test Promise unwrapping type mismatches
// =================================================================

async function asyncFunc(): Promise<number> {
    return 42;
}

// @ts-expect-error - Type 'Promise<number>' is not assignable to type 'number'
let promiseWrong: number = asyncFunc();

// Valid async usage
async function validAsync() {
    let num: number = await asyncFunc(); // Valid
}

// =================================================================
// SECTION 13: CLASS TYPE ASSIGNMENTS (TS2322)
// Test class hierarchy type mismatches
// =================================================================

class Animal {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
}

class Dog extends Animal {
    breed: string;
    constructor(name: string, breed: string) {
        super(name);
        this.breed = breed;
    }
}

// @ts-expect-error - Type 'Animal' is not assignable to type 'Dog'
let classWrong: Dog = new Animal("Generic");

// Valid class hierarchy assignment
let classValid: Animal = new Dog("Buddy", "Golden");

// =================================================================
// SECTION 14: TYPE PREDICATE ASSIGNMENTS (TS2322)
// Test type predicate function mismatches
// =================================================================

function isString(value: unknown): value is string {
    return typeof value === "string";
}

// @ts-expect-error - Type '(value: unknown) => boolean' is not assignable to type 'Predicate'
let predicateWrong: (value: unknown) => value is string = (value) => typeof value === "number";

// Valid type predicate
let predicateValid: (value: unknown) => value is string = isString;

// =================================================================
// SECTION 15: UNKNOWN TYPE ASSIGNMENTS (TS2322)
// Test that Unknown is NOT assignable without check (unlike Any)
// =================================================================

let unknownValue: unknown = 42;

// @ts-expect-error - Type 'unknown' is not assignable to type 'number'
let unknownWrong: number = unknownValue;

// Valid with type guard
function useUnknown(u: unknown) {
    if (typeof u === "number") {
        let n: number = u; // Valid after narrowing
    }
}

// =================================================================
// SECTION 16: NEVER TYPE ASSIGNMENTS (valid case)
// Test that Never IS assignable to everything (should NOT error)
// =================================================================

function neverReturns(): never {
    throw new Error();
}

// Valid - never is assignable to everything
let neverValid: string = neverReturns();
let neverValid2: number = neverReturns();
let neverValid3: boolean = neverReturns();

// =================================================================
// SECTION 17: NULL/UNDEFINED ASSIGNMENTS WITH STRICTNULLCHECKS (TS2322)
// Test null/undefined assignments
// =================================================================

// @ts-expect-error - Type 'null' is not assignable to type 'number'
let nullWrong: number = null;

// @ts-expect-error - Type 'undefined' is not assignable to type 'string'
let undefinedWrong: string = undefined;

// Valid with proper type
let nullValid: number | null = null;
let undefinedValid: string | undefined = undefined;

// =================================================================
// SECTION 18: TEMPLATE LITERAL TYPE ASSIGNMENTS (TS2322)
// Test template literal type mismatches
// =================================================================

type Greeting = `hello ${string}`;

// @ts-expect-error - Type '"goodbye world"' is not assignable to type 'Greeting'
let templateWrong: Greeting = "goodbye world";

// @ts-expect-error - Type '"hello"' is not assignable to type 'Greeting'
let templateWrong2: Greeting = "hello";

// Valid template literal
let templateValid: Greeting = "hello world";

// =================================================================
// SECTION 19: KEYOF TYPE ASSIGNMENTS (TS2322)
// Test keyof type mismatches
// =================================================================

interface User {
    name: string;
    age: number;
}

type UserKeys = keyof User;

// @ts-expect-error - Type '"email"' is not assignable to type 'UserKeys'
let keyofWrong: UserKeys = "email";

// Valid keyof
let keyofValid: UserKeys = "name";

// =================================================================
// SECTION 20: SATISFIES OPERATOR (TS2322)
// Test that satisfies still checks type compatibility
// =================================================================

type Required = { name: string; age: number };

// @ts-expect-error - Property 'age' is missing
let satisfiesWrong: Required = { name: "test" } satisfies Required;

// Valid satisfies
let satisfiesValid: Required = { name: "test", age: 25 } satisfies Required;

// =================================================================
// SECTION 21: BRANDED TYPES (TS2322)
// Test branded type mismatches
// =================================================================

type USD = number & { readonly __brand: unique symbol };
type EUR = number & { readonly __brand: unique symbol };

const usd = (value: number): USD => value as USD;
const eur = (value: number): EUR => value as EUR;

// @ts-expect-error - Type 'EUR' is not assignable to type 'USD'
let brandedWrong: USD = eur(100);

// Valid branded type
let brandedValid: USD = usd(100);

// =================================================================
// SECTION 22: CONDITIONAL TYPE ASSIGNMENTS (TS2322)
// Test conditional type mismatches
// =================================================================

type IsString<T> = T extends string ? true : false;

// @ts-expect-error - Type 'false' is not assignable to type 'true'
let conditionalWrong: IsString<string> = false as IsString<number>;

// Valid conditional type
let conditionalValid: IsString<string> = true;

// =================================================================
// SECTION 23: MAPPED TYPE ASSIGNMENTS (TS2322)
// Test mapped type mismatches
// =================================================================

type Readonly<T> = {
    readonly [P in keyof T]: T[P];
};

type Mutable = {
    name: string;
    age: number;
};

// @ts-expect-error - Cannot assign to 'name' because it is read-only
const readonlyObj: Readonly<Mutable> = { name: "test", age: 25 };
// readonlyObj.name = "changed"; // Should error

// Valid readonly
const readonlyValid: Readonly<Mutable> = { name: "test", age: 25 };

// =================================================================
// SECTION 24: INFERRED TYPE ASSIGNMENTS (TS2322)
// Test that inferred types are checked
// =================================================================

const inferred = { name: "test", age: 25 };

// @ts-expect-error - Type '{ name: string; age: number; }' is not assignable to type '{ name: string; }'
let inferredWrong: { name: string } = { name: "test", age: 25 };

// Valid with proper type
let inferredValid: { name: string; age?: number } = { name: "test", age: 25 };

// =================================================================
// SECTION 25: DISCRIMINATED UNION TYPE ASSIGNMENTS (TS2322)
// Test discriminated union mismatches
// =================================================================

type Shape = { kind: "circle"; radius: number } | { kind: "square"; side: number };

// @ts-expect-error - Type '{ kind: "triangle"; side: number; }' is not assignable
let discriminatedWrong: Shape = { kind: "triangle", side: 3 } as any;

// Valid discriminated union
let discriminatedValid: Shape = { kind: "circle", radius: 5 };

// =================================================================
// SECTION 26: ARRAY/TUPLE READONLY ASSIGNMENTS (TS2322)
// Test readonly array assignments
// =================================================================

// @ts-expect-error - Type 'number[]' is not assignable to type 'readonly number[]'
let readonlyWrong: readonly number[] = [1, 2, 3] as number[];

// Valid readonly
let readonlyValid: readonly number[] = [1, 2, 3] as const;

// =================================================================
// SECTION 27: OPTIONAL CHAINING TYPE ASSIGNMENTS (TS2322)
// Test optional chaining type mismatches
// =================================================================

interface Data {
    user?: {
        name?: string;
    };
}

const data: Data = {};

// @ts-expect-error - Type 'string | undefined' is not assignable to type 'string'
let optionalWrong: string = data.user?.name;

// Valid optional chaining
let optionalValid: string | undefined = data.user?.name;

// =================================================================
// SECTION 28: REST PARAMETER TYPE ASSIGNMENTS (TS2322)
// Test rest parameter type mismatches
// =================================================================

function sum(...nums: number[]) {
    return nums.reduce((a, b) => a + b, 0);
}

// @ts-expect-error - Argument of type 'string' is not assignable to parameter of type 'number'
sum(1, 2, "3");

// Valid rest parameters
sum(1, 2, 3);

// =================================================================
// SECTION 29: DESTRUCTURING TYPE ASSIGNMENTS (TS2322)
// Test destructuring type mismatches
// =================================================================

const point = { x: 10, y: 20 };

// @ts-expect-error - Type 'string' is not assignable to type 'number'
const { x, y }: { x: number; y: string } = point;

// Valid destructuring
const { x: xValid, y: yValid }: { x: number; y: number } = point;

// =================================================================
// SECTION 30: GENERIC DEFAULT TYPE ASSIGNMENTS (TS2322)
// Test generic default type mismatches
// =================================================================

interface BoxDefault<T = string> {
    value: T;
}

// @ts-expect-error - Type 'number' is not assignable to type 'string'
let boxDefaultWrong: BoxDefault = { value: 42 };

// Valid generic default
let boxDefaultValid: BoxDefault = { value: "test" };

// Explicit type parameter
let boxDefaultValid2: BoxDefault<number> = { value: 42 };

console.log("TS2322 assignment type mismatch tests complete");
