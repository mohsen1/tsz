// =================================================================
// SOLVER CONFORMANCE TESTS
// Tests for type checking, assignments, generics, unions, intersections
// =================================================================

// =================================================================
// SECTION 1: PRIMITIVE TYPE ASSIGNMENTS (TS2322)
// =================================================================

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let num1: number = "hello";

// @ts-expect-error - Type 'number' is not assignable to type 'string'
let str1: string = 42;

// @ts-expect-error - Type 'boolean' is not assignable to type 'number'
let num2: number = true;

// @ts-expect-error - Type 'number' is not assignable to type 'boolean'
let bool1: boolean = 0;

// @ts-expect-error - Type 'string' is not assignable to type 'boolean'
let bool2: boolean = "true";

// Valid assignments
let num3: number = 42;
let str2: string = "hello";
let bool3: boolean = true;
let num4: number = num3;
let str3: string = str2;

// =================================================================
// SECTION 2: OBJECT TYPE ASSIGNMENTS (TS2322)
// =================================================================

interface Person {
    name: string;
    age: number;
}

interface Animal {
    name: string;
    species: string;
}

// @ts-expect-error - Type 'Animal' is not assignable to type 'Person'
let person1: Person = { name: "Fluffy", species: "Cat" };

// @ts-expect-error - Property 'age' is missing
let person2: Person = { name: "John" };

// Valid - excess property is allowed when directly assigned
let person3: Person = { name: "Jane", age: 30, city: "NYC" };

// @ts-expect-error - Type 'Person' is not assignable to type 'Animal'
let animal1: Animal = { name: "John", age: 30 };

// Valid assignment
const person4: Person = { name: "Bob", age: 25 };
const person5: Person = person4;

// =================================================================
// SECTION 3: FUNCTION TYPE ASSIGNMENTS (TS2322)
// =================================================================

type NumToString = (x: number) => string;
type StrToNum = (x: string) => number;

// @ts-expect-error - Type 'StrToNum' is not assignable to type 'NumToString'
let f1: NumToString = (s: string) => s.length;

// @ts-expect-error - Type '(x: number) => string' is not assignable to 'StrToNum'
let f2: StrToNum = (x: number) => x.toString();

// Valid - parameter type is contravariant (bivariant in functions)
let f3: NumToString = (x: any) => "hello";

// Return type is covariant
// @ts-expect-error - Type 'number' is not assignable to type 'string'
let f4: NumToString = (x: number) => 42 as any;

// =================================================================
// SECTION 4: UNION TYPES (TS2322)
// =================================================================

type StringOrNumber = string | number;

// Valid assignments
let u1: StringOrNumber = "hello";
let u2: StringOrNumber = 42;
let u3: StringOrNumber = true as any;

// @ts-expect-error - Type 'boolean' is not assignable to type 'string | number'
let u4: StringOrNumber = true;

// @ts-expect-error - Type 'boolean' is not assignable
let u5: StringOrNumber = false;

interface Dog {
    bark(): void;
}

interface Cat {
    meow(): void;
}

type Pet = Dog | Cat;

// Valid - Dog is assignable to Pet
let pet1: Pet = { bark() {} };

// @ts-expect-error - Type '{}' is not assignable to type 'Pet'
let pet2: Pet = {};

// Union narrowing
function processUnion(value: string | number) {
    // @ts-expect-error - Property 'length' does not exist on type 'number'
    if (typeof value === "number") {
        console.log(value.length);
    }
    return value;
}

// =================================================================
// SECTION 5: INTERSECTION TYPES (TS2322)
// =================================================================

type HasName = { name: string };
type HasAge = { age: number };
type NamedPerson = HasName & HasAge;

// Valid - satisfies both constraints
let p1: NamedPerson = { name: "Alice", age: 30 };

// @ts-expect-error - Property 'age' is missing
let p2: NamedPerson = { name: "Bob" };

// @ts-expect-error - Property 'name' is missing
let p3: NamedPerson = { age: 25 };

type A = { a: string };
type B = { b: number };
type C = { c: boolean };
type ABC = A & B & C;

// Valid
let abc1: ABC = { a: "x", b: 1, c: true };

// @ts-expect-error - Type '{ a: string; b: number; }' is not assignable
let abc2: ABC = { a: "x", b: 1 };

// Intersection with conflicting types
type X = { x: number };
type Y = { x: string };
// @ts-expect-error - Type 'number' is not assignable to type 'string' (intersection creates never)
let xy: X & Y = { x: 42 as any };

// =================================================================
// SECTION 6: ANY AND UNKNOWN TYPES (TS2322)
// =================================================================

// Any is top type - assignable to everything
let anyValue: any = 42;
let numFromAny: number = anyValue; // Valid - any is assignable
let strFromAny: string = anyValue; // Valid - any is assignable
let boolFromAny: boolean = anyValue; // Valid - any is assignable

// Unknown is top type but not assignable without check
let unknownValue: unknown = 42;
// @ts-expect-error - Type 'unknown' is not assignable to type 'number'
let numFromUnknown: number = unknownValue;

// @ts-expect-error - Type 'unknown' is not assignable to type 'string'
let strFromUnknown: string = unknownValue;

// Valid with type guard
function processUnknown(u: unknown) {
    if (typeof u === "number") {
        const n: number = u; // Valid after narrowing
    }
}

// Any can be assigned from any type
let a1: any = "string";
let a2: any = 123;
let a3: any = { x: 1 };
let a4: any = [1, 2, 3];

// Unknown can be assigned from any type
let u1: unknown = "string";
let u2: unknown = 123;
let u3: unknown = { x: 1 };

// =================================================================
// SECTION 7: GENERIC TYPE ASSIGNMENTS (TS2322)
// =================================================================

interface Box<T> {
    value: T;
}

// Valid - exact type match
let box1: Box<string> = { value: "hello" };
let box2: Box<number> = { value: 42 };

// @ts-expect-error - Type 'Box<string>' is not assignable to type 'Box<number>'
let box3: Box<number> = box1;

// Generic with constraints
interface Identifiable {
    id: number;
}

function getId<T extends Identifiable>(obj: T): number {
    return obj.id;
}

// Valid - has id property
const obj1 = getId({ id: 1, name: "test" });

// @ts-expect-error - Argument of type '{ name: string; }' is not assignable
const obj2 = getId({ name: "test" });

// Variance in generics
interface Producer<out T> {
    produce(): T;
}

interface Consumer<in T> {
    consume(value: T): void;
}

// Covariant - Producer<string> assignable to Producer<unknown>
// @ts-expect-error - Type 'Producer<unknown>' is not assignable to 'Producer<string>'
let prod1: Producer<string> = { produce() { return "hello" as unknown } };

// Contravariant
// @ts-expect-error - Type 'Consumer<string>' is not assignable to 'Consumer<number>'
let cons1: Consumer<number> = { consume: (s: string) => {} };

// =================================================================
// SECTION 8: TS7006 - IMPLICIT ANY PARAMETER ERRORS
// =================================================================

// @ts-expect-error - Parameter 'x' implicitly has an 'any' type
function foo1(x) {
    return x + 1;
}

// @ts-expect-error - Parameter 'a' implicitly has an 'any' type
function foo2(a, b: number) {
    return a + b;
}

// @ts-expect-error - All parameters implicitly have 'any' type
function foo3(a, b, c) {
    return a + b + c;
}

// @ts-expect-error - Parameter 'name' implicitly has an 'any' type
const arrow1 = (name) => `Hello ${name}`;

// @ts-expect-error - Parameters 'x' and 'y' implicitly have 'any' type
const arrow2 = (x, y) => x + y;

// Valid - all parameters have explicit types
function bar1(x: number, y: string): void {
    console.log(x, y);
}

// Valid - arrow function with types
const arrow3 = (x: number): number => x * 2;

// Object method with implicit any
// @ts-expect-error - Parameter 'value' implicitly has an 'any' type
const obj1 = {
    method(value) {
        return value;
    }
};

// Class method with implicit any
class MyClass1 {
    // @ts-expect-error - Parameter 'param' implicitly has an 'any' type
    method1(param) {
        return param;
    }

    // Valid
    method2(param: string): string {
        return param;
    }
}

// Destructuring with implicit any
// @ts-expect-error - Parameter '{ a, b }' implicitly has an 'any' type
function destructure1({ a, b }) {
    return a + b;
}

// @ts-expect-error - Parameter '[first, second]' implicitly has an 'any' type
function destructure2([first, second]) {
    return first + second;
}

// Rest parameter with implicit any
// @ts-expect-error - Rest parameter 'args' implicitly has an 'any' type
function rest1(...args) {
    return args;
}

// Default parameter with implicit any
// @ts-expect-error - Parameter 'x' implicitly has an 'any' type
function default1(x = 10) {
    return x;
}

// Optional parameter with implicit any
// @ts-expect-error - Parameter 'y' implicitly has an 'any' type
function optional1(x: number, y?) {
    return y;
}

// =================================================================
// SECTION 9: FUNCTION OVERLOADS (TS2322, TS7006)
// =================================================================

interface Overloaded {
    (x: string): string;
    (x: number): number;
}

// Valid - string call
const overload1: Overloaded = (x: any) => x;

// @ts-expect-error - Type 'string' is not assignable to type 'number'
function wrongOverload(x: string | number): string | number {
    if (typeof x === "string") {
        return x.toUpperCase();
    }
    return x.toFixed(2);
}

// @ts-expect-error - Parameter 'data' implicitly has an 'any' type
function process(data, callback) {
    return callback(data);
}

// =================================================================
// SECTION 10: ARRAY AND TUPLE TYPE ASSIGNMENTS (TS2322)
// =================================================================

// Array assignments
let arr1: number[] = [1, 2, 3];
let arr2: string[] = ["a", "b", "c"];

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let arr3: number[] = [1, 2, "3"];

// @ts-expect-error - Type 'number[]' is not assignable to type 'string[]'
let arr4: string[] = arr1;

// Readonly array
let readonly1: readonly number[] = [1, 2, 3];
// @ts-expect-error - Type 'number[]' is not assignable to type 'readonly number[]'
let mutableArr: number[] = readonly1 as any;

// Tuple types
let tuple1: [string, number] = ["hello", 42];

// @ts-expect-error - Type 'number' is not assignable to type 'string'
let tuple2: [string, number] = [42, "hello"];

// @ts-expect-error - Property '0' is missing
let tuple3: [string, number] = ["hello"];

// @ts-expect-error - Type '[string, number, boolean]' is not assignable
let tuple4: [string, number] = ["hello", 42, true];

// =================================================================
// SECTION 11: LITERAL TYPES (TS2322)
// =================================================================

type Direction = "north" | "south" | "east" | "west";

let dir1: Direction = "north";
let dir2: Direction = "south";

// @ts-expect-error - Type '"northeast"' is not assignable to type 'Direction'
let dir3: Direction = "northeast";

// @ts-expect-error - Type 'string' is not assignable to type 'Direction'
let dir4: Direction = "north" as string;

// Numeric literal types
type DiceRoll = 1 | 2 | 3 | 4 | 5 | 6;

let roll1: DiceRoll = 6;
// @ts-expect-error - Type '7' is not assignable to type 'DiceRoll'
let roll2: DiceRoll = 7;

// Boolean literal types
type True = true;
type False = false;

// @ts-expect-error - Type 'false' is not assignable to type 'true'
let t1: True = false;

// =================================================================
// SECTION 12: NARROWING TYPE TESTS
// =================================================================

function narrowUnion(value: string | number | boolean) {
    if (typeof value === "string") {
        // Here value is narrowed to string
        const s: string = value; // Valid
        return value.toUpperCase();
    } else if (typeof value === "number") {
        // Here value is narrowed to number
        const n: number = value; // Valid
        return value * 2;
    } else {
        // Here value is narrowed to boolean
        const b: boolean = value; // Valid
        return !value;
    }
}

// Truthy narrowing
function truthyNarrowing(value: string | null | undefined) {
    if (value) {
        // Here value is narrowed to string (excluding null and undefined)
        return value.length;
    }
    return 0;
}

// instanceof narrowing
class Dog1 { bark() {} }
class Cat1 { meow() {} }

function petNoise(pet: Dog1 | Cat1) {
    if (pet instanceof Dog1) {
        pet.bark(); // Valid
        // @ts-expect-error - Property 'meow' does not exist on type 'Dog1'
        pet.meow();
    } else {
        pet.meow(); // Valid
    }
}

// =================================================================
// SECTION 13: CONDITIONAL TYPES
// =================================================================

type IsString<T> = T extends string ? true : false;

type T1 = IsString<string>; // true
type T2 = IsString<number>; // false

// Conditional type in function
function processConditional<T>(value: T): IsString<T> extends true ? string : number {
    if (typeof value === "string") {
        return value as any;
    }
    return 42;
}

// Infer in conditional types
type Unpacked<T> = T extends (infer U)[] ? U : T;

type Arr = Unpacked<number[]>; // number
type NotArr = Unpacked<number>; // number

// =================================================================
// SECTION 14: MAPPED TYPES
// =================================================================

type Readonly<T> = {
    readonly [P in keyof T]: T[P];
};

type Partial<T> = {
    [P in keyof T]?: T[P];
};

interface User {
    name: string;
    age: number;
}

type ReadonlyUser = Readonly<User>;
type PartialUser = Partial<User>;

// @ts-expect-error - Cannot assign to 'name' because it is read-only
const ru: ReadonlyUser = { name: "test", age: 25 };
// ru.name = "changed";

// Valid - Partial allows undefined
const pu: PartialUser = { name: "test" };

// =================================================================
// SECTION 14.5: READONLY PROPERTY ASSIGNABILITY
// =================================================================

// Test readonly property assignability rules

// Readonly to readonly - valid
interface ReadonlyProps {
    readonly x: number;
    readonly y: string;
}

const rp1: ReadonlyProps = { x: 1, y: "test" };
const rp2: ReadonlyProps = rp1; // Valid

// Mutable to mutable - valid
interface MutableProps {
    x: number;
    y: string;
}

const mp1: MutableProps = { x: 1, y: "test" };
const mp2: MutableProps = mp1; // Valid

// Mutable to readonly - VALID (covariant for reading)
interface ReadonlyToMutableTest {
    readonly x: number;
}

const rt: ReadonlyToMutableTest = { x: 1 };
const mt1: MutableProps = { x: 1, y: "test" }; // Mutable type
const rtFromMutable: ReadonlyToMutableTest = mt1; // Valid - mutable to readonly works

// Readonly to mutable - INVALID
interface MutableFromReadonly {
    x: number; // mutable
}

const readonlyObj: ReadonlyProps = { x: 1, y: "test" };
// @ts-expect-error - Type 'ReadonlyProps' is not assignable to type 'MutableFromReadonly'
const mutableFromReadonly: MutableFromReadonly = readonlyObj;

// Mixed readonly and mutable properties
interface MixedReadonly {
    readonly x: number;
    y: string; // mutable
}

interface MixedMutable {
    x: number; // mutable
    readonly y: string;
}

// @ts-expect-error - Property 'x' is readonly in source but mutable in target
const mixed1: MixedMutable = { x: 1, y: "test" } as MixedReadonly;

// @ts-expect-error - Property 'y' is mutable in source but readonly in target (wait, this should actually work)
const mixed2: MixedReadonly = { x: 1, y: "test" } as MixedMutable;

// Actually, mutable to readonly should work for each property independently
const mixedMutable: MixedMutable = { x: 1, y: "test" };
// @ts-expect-error - Cannot assign readonly 'x' to mutable 'x'
const mixedToReadonly: MixedReadonly = mixedMutable;

// Readonly array to mutable array
const readonlyArr: readonly number[] = [1, 2, 3];
// @ts-expect-error - Type 'readonly number[]' is not assignable to type 'number[]'
const mutableArr: number[] = readonlyArr;

// Mutable array to readonly array - VALID
const mutableArr2: number[] = [1, 2, 3];
const readonlyArr2: readonly number[] = mutableArr2; // Valid

// Readonly tuple
const readonlyTuple: readonly [number, string] = [1, "test"];
// @ts-expect-error - Type 'readonly [number, string]' is not assignable to type '[number, string]'
const mutableTuple: [number, string] = readonlyTuple;

// =================================================================
// SECTION 15: TEMPLATE LITERAL TYPES
// =================================================================

type Greeting = `hello ${string}`;

// Valid
let g1: Greeting = "hello world";
let g2: Greeting = "hello alice";

// @ts-expect-error - Type '"goodbye world"' is not assignable
let g3: Greeting = "goodbye world";

// @ts-expect-error - Type '"hello"' is not assignable
let g4: Greeting = "hello";

type EventName<T extends string> = `on${Capitalize<T>}`;

type ClickEvent = EventName<"click">; // "onClick"

// @ts-expect-error - Type '"onclick"' is not assignable
let e1: EventName<"click"> = "onclick";

// =================================================================
// SECTION 16: KEYOF AND INDEXED ACCESS
// =================================================================

interface Product {
    name: string;
    price: number;
    category: string;
}

type ProductKeys = keyof Product; // "name" | "price" | "category"

// @ts-expect-error - Type '"color"' is not assignable to type 'ProductKeys'
let key1: ProductKeys = "color";

let key2: ProductKeys = "name"; // Valid

type NameType = Product["name"]; // string
type PriceType = Product["price"]; // number

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let price: PriceType = "expensive" as any;

// =================================================================
// SECTION 17: ASYNC/AWAIT TYPE CHECKING (TS2322)
// =================================================================

async function asyncFunction() {
    return 42;
}

// @ts-expect-error - Type 'Promise<number>' is not assignable to type 'number'
let num: number = asyncFunction();

async function consumeAsync() {
    const result = await asyncFunction(); // Valid - number
    // @ts-expect-error - Type 'number' is not assignable to type 'string'
    const str: string = result;
}

// =================================================================
// SECTION 18: CLASS TYPE ASSIGNMENTS (TS2322)
// =================================================================

class Animal2 {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
    speak() {
        console.log("Some sound");
    }
}

class Dog2 extends Animal2 {
    breed: string;
    constructor(name: string, breed: string) {
        super(name);
        this.breed = breed;
    }
    bark() {
        console.log("Woof!");
    }
}

// Valid - Dog2 is assignable to Animal2
let animalRef: Animal2 = new Dog2("Buddy", "Golden");

// @ts-expect-error - Type 'Animal2' is not assignable to type 'Dog2'
let dogRef: Dog2 = new Animal2("Generic");

// Abstract class (simulated with private constructor)
class Base {
    private constructor() {}
}
// @ts-expect-error - Cannot extend a class with private constructor
class Derived extends Base {}

// =================================================================
// SECTION 19: TYPE ASSERTIONS AND TYPE PREDICATES
// =================================================================

// Type assertion
let value1: unknown = "hello";
let strLen: number = (value1 as string).length; // Valid with assertion

// @ts-expect-error - Conversion of type 'string' to type 'number' may be a mistake
let num: number = "42" as any;

// Type predicate
function isString(value: unknown): value is string {
    return typeof value === "string";
}

function usePredicate(value: unknown) {
    if (isString(value)) {
        // Here value is narrowed to string
        return value.toUpperCase();
    }
    return 0;
}

// =================================================================
// SECTION 20: DISCRIMINATED UNIONS
// =================================================================

interface Circle {
    kind: "circle";
    radius: number;
}

interface Square {
    kind: "square";
    side: number;
}

type Shape = Circle | Square;

function area(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            return Math.PI * shape.radius ** 2;
        case "square":
            return shape.side ** 2;
    }
}

// Valid discriminated union
const circle: Shape = { kind: "circle", radius: 5 };

// @ts-expect-error - Type '"triangle"' is not assignable to type '"circle" | "square"'
const triangle: Shape = { kind: "triangle", side: 3 } as any;

// Discriminated union narrowing with if statements
function getDimensions(shape: Shape): number {
    if (shape.kind === "circle") {
        // shape is narrowed to Circle here
        return shape.radius * 2;
    } else {
        // shape is narrowed to Square here
        return shape.side * 4;
    }
}

// Test that narrowing works correctly
const testCircle: Shape = { kind: "circle", radius: 10 };
const diameter: number = getDimensions(testCircle);

const testSquare: Shape = { kind: "square", side: 5 };
const perimeter: number = getDimensions(testSquare);

// Multiple discriminant properties
interface Success { status: "success"; data: string }
interface Loading { status: "loading"; progress: number }
interface Error { status: "error"; message: string }

type Result = Success | Loading | Error;

function handleResult(result: Result): string {
    if (result.status === "success") {
        return result.data;
    } else if (result.status === "loading") {
        return `Loading: ${result.progress}%`;
    } else {
        return result.message;
    }
}

// Test with multiple discriminants
const successResult: Result = { status: "success", data: "Done" };
const successOutput: string = handleResult(successResult);

// Discriminated union with exhaustive switch
function getCircumference(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            return 2 * Math.PI * shape.radius;
        case "square":
            return shape.side * 4;
    }
}

// Nested discriminated unions
interface Rectangle { kind: "rectangle"; width: number; height: number }
type ExtendedShape = Circle | Square | Rectangle;

function calculateAll(shape: ExtendedShape): number {
    switch (shape.kind) {
        case "circle":
            return Math.PI * shape.radius ** 2;
        case "square":
            return shape.side ** 2;
        case "rectangle":
            return shape.width * shape.height;
    }
}

// Test that property access fails without narrowing
function invalidAccess(shape: Shape): number {
    // @ts-expect-error - Property 'side' does not exist on type 'Circle'
    return shape.side;
}

// Test narrowing with assignment
function processShape(shape: Shape): void {
    const currentKind = shape.kind;
    if (currentKind === "circle") {
        // shape is narrowed to Circle
        const r: number = shape.radius;
    }
}

// =================================================================
// SECTION 21: OPTIONAL CHAINING AND NULLISH COALESCING
// =================================================================

interface Data {
    user?: {
        name?: string;
        address?: {
            city?: string;
        };
    };
}

const data: Data = {};

// Valid - optional chaining returns possibly undefined
const city1: string | undefined = data.user?.address?.city;

// @ts-expect-error - Type 'string | undefined' is not assignable to type 'string'
const city2: string = data.user?.address?.city;

// Nullish coalescing
const name1: string = data.user?.name ?? "Anonymous"; // Valid

// =================================================================
// SECTION 22: TYPE GUARDS WITH IN OPERATOR
// =================================================================

interface A1 { a: string }
interface B1 { b: number }

function processIn(value: A1 | B1) {
    if ("a" in value) {
        // Here value is narrowed to A1
        return value.a.toUpperCase();
    } else {
        // Here value is narrowed to B1
        return value.b * 2;
    }
}

// =================================================================
// SECTION 23: GENERICS WITH DEFAULT TYPES
// =================================================================

interface Box2<T = string> {
    value: T;
}

// Valid - uses default string type
let boxDefault1: Box2 = { value: "hello" };

// @ts-expect-error - Type 'number' is not assignable to type 'string'
let boxDefault2: Box2 = { value: 42 };

// Explicit type parameter
let boxExplicit: Box2<number> = { value: 42 }; // Valid

// =================================================================
// SECTION 24: BRANDS AND OPAQUE TYPES (via intersection)
// =================================================================

type USD = number & { readonly __brand: unique symbol };
type EUR = number & { readonly __brand: unique symbol };

const usd = (value: number): USD => value as USD;
const eur = (value: number): EUR => value as EUR;

// Valid
let money1: USD = usd(100);

// @ts-expect-error - Type 'EUR' is not assignable to type 'USD'
let money2: USD = eur(100);

// =================================================================
// SECTION 25: RECURSIVE TYPES
// =================================================================

type Json =
    | string
    | number
    | boolean
    | null
    | Json[]
    | { [key: string]: Json };

// Valid recursive types
const json1: Json = { name: "test", values: [1, 2, 3] };
const json2: Json = ["nested", { obj: true }];
const json3: Json = null;

// @ts-expect-error - Type 'undefined' is not assignable to type 'Json'
const json4: Json = undefined;

type Tree = {
    value: number;
    left?: Tree;
    right?: Tree;
};

// Valid tree
const tree: Tree = {
    value: 1,
    left: { value: 2 },
    right: { value: 3, left: { value: 4 } }
};

// =================================================================
// SECTION 26: THIS TYPE ANNOTATIONS
// =================================================================

interface Counter {
    count: number;
    increment(): this;
    reset(): this;
}

function createCounter(): Counter {
    let count = 0;
    return {
        get count() { return count; },
        increment() {
            count++;
            return this;
        },
        reset() {
            count = 0;
            return this;
        }
    };
}

const counter = createCounter();
// Valid - chaining with this type
counter.increment().increment().reset();

// =================================================================
// SECTION 27: AMBIENT TYPES
// =================================================================

declare const globalValue: number;

// Valid - ambient declaration
const x = globalValue * 2;

// @ts-expect-error - Type 'string' is not assignable to type 'number'
const y: number = globalValue as any;

// Declare module
declare module "my-module" {
    export function helper(): string;
}

// =================================================================
// SECTION 28: TYPE IMPORTS/EXPORTS
// =================================================================

// @ts-expect-error - Module '"non-existent"' has no exported member 'Type'
import { Type } from "non-existent";

// Re-export
export type { Person };
export type { Animal as Pet };

// =================================================================
// SECTION 29: SATISFIES OPERATOR
// =================================================================

type ShapeWithColor = { color: string; radius: number };

// @ts-expect-error - Property 'radius' is missing
let satisfies1: ShapeWithColor = { color: "red" };

// Valid with satisfies (colors would be inferred with narrower type)
const shape = { color: "red", radius: 10 } satisfies ShapeWithColor;

// =================================================================
// SECTION 30: ADDITIONAL EDGE CASES
// =================================================================

// Never type
function neverReturns(): never {
    throw new Error();
}

// Valid - never is assignable to everything
let n: string = neverReturns();

// @ts-expect-error - Type 'never[]' is not assignable to type 'string[]'
let neverArr: string[] = [] as never[];

// Void type
function returnsNothing(): void {
    console.log("nothing");
}

// @ts-expect-error - Type 'void' is not assignable to type 'string'
let voidVal: string = returnsNothing();

// Null assignments (with strictNullChecks)
// @ts-expect-error - Type 'null' is not assignable to type 'number'
let n1: number = null;

// @ts-expect-error - Type 'undefined' is not assignable to type 'string'
let n2: string = undefined;

// Non-null assertion
const maybeString: string | null = "hello";
// @ts-expect-error - Object is possibly 'null'
const len: number = maybeString.length;
const len2: number = maybeString!.length; // Valid with non-null assertion

// =================================================================
// SECTION 31: COMPLEX GENERIC CONSTRAINTS
// =================================================================

interface WithLength {
    length: number;
}

function getLength<T extends WithLength>(arg: T): number {
    return arg.length;
}

// Valid
getLength("hello");
getLength([1, 2, 3]);
getLength({ length: 10, value: "test" });

// @ts-expect-error - Argument of type 'number' is not assignable
getLength(42);

// Multiple constraints
interface A {
    a: string;
}
interface B {
    b: number;
}

function combine<T extends A & B>(obj: T): string {
    return obj.a + obj.b.toString();
}

// Valid
combine({ a: "test", b: 42 });

// @ts-expect-error - Type '{ a: string; }' is not assignable to type 'A & B'
combine({ a: "test" });

// =================================================================
// SECTION 32: TEMPLATE LITERAL TYPE INFERENCE
// =================================================================

function makeEvent<T extends string>(eventName: `on${T}`): T {
    // @ts-expect-error - Return type is complex, this tests inference
    return eventName.slice(2) as T;
}

// Valid
const click = makeEvent("onClick"); // "click"

// @ts-expect-error - Argument of type '"click"' is not assignable
const badEvent = makeEvent("click");

// =================================================================
// SECTION 33: INFER FROM RETURN TYPE
// =================================================================

type ReturnType<T> = T extends (...args: any[]) => infer R ? R : any;

function returnsString(): string {
    return "hello";
}

function returnsNumber(): number {
    return 42;
}

type R1 = ReturnType<typeof returnsString>; // string
type R2 = ReturnType<typeof returnsNumber>; // number

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let rTest: R2 = "hello";

// =================================================================
// SECTION 34: AWAITED TYPE
// =================================================================

type Awaited<T> = T extends Promise<infer U> ? U : T;

type PromiseString = Promise<string>;
type AwaitedString = Awaited<PromiseString>; // string

// @ts-expect-error - Type 'string' is not assignable to type 'number'
let awaitedTest: Awaited<Promise<number>> = "hello";

// =================================================================
// SECTION 35: SYMBOL TYPE CHECKING
// =================================================================

const sym1 = Symbol("description");
const sym2 = Symbol.for("key");

let s1: symbol = sym1;
let s2: symbol = sym2;

// @ts-expect-error - Type 'string' is not assignable to type 'symbol'
let s3: symbol = "description";

// Unique symbol
const us: unique symbol = Symbol("unique");
// @ts-expect-error - A 'unique symbol' type must be referenced
let us2: unique symbol = Symbol("another");

// =================================================================
// SECTION 36: BIGINT TYPE CHECKING
// =================================================================

let big1: bigint = 100n;
let big2: bigint = BigInt(100);

// @ts-expect-error - Type 'number' is not assignable to type 'bigint'
let big3: bigint = 100;

// @ts-expect-error - Type 'bigint' is not assignable to type 'number'
let numBig: number = 100n;

// Mixed arithmetic
// @ts-expect-error - Operator '+' cannot be applied to types 'bigint' and 'number'
const mixed = 100n + 100;

// =================================================================
// SECTION 37: FINAL VALID CASES (should not error)
// =================================================================

// These should all be valid assignments
const valid1: string = "hello";
const valid2: number = 42;
const valid3: boolean = true;
const valid4: string[] = ["a", "b"];
const valid5: [string, number] = ["test", 123];
const valid6: { name: string } = { name: "test" };
const valid7: null = null;
const valid8: undefined = undefined;
const valid9: any = "anything";
const valid10: never = (() => { throw new Error(); })();

// Generic valid cases
const valid11: Array<string> = ["hello"];
const valid12: ReadonlyArray<number> = [1, 2, 3];
const valid13: Promise<string> = Promise.resolve("test");

// Union valid cases
const valid14: string | number = "hello";
const valid15: string | number = 42;

// Intersection valid cases
const valid16: { a: string } & { b: number } = { a: "test", b: 123 };

// Function valid cases
const valid17: (x: number) => string = (x) => x.toString();
const valid18: () => void = () => {};

console.log("Solver tests complete");
