// =================================================================
// TS2322 VALID ASSIGNMENT TESTS (NO FALSE POSITIVES)
// These tests verify that valid type assignments do NOT emit errors.
// This file contains only VALID code - no @ts-expect-error comments.
// If any of these emit TS2322 errors, it indicates a false positive.
// =================================================================

// @strict: true
// @noEmit: true

// =================================================================
// SECTION 1: PRIMITIVE TYPE WIDENING (VALID)
// =================================================================

// Literal widens to its base type
const strLiteral = "hello";
const str: string = strLiteral;

const numLiteral = 42;
const num: number = numLiteral;

const boolLiteral = true;
const bool: boolean = boolLiteral;

// Const assertions preserve literal type
const constStr = "hello" as const;
const literalStr: "hello" = constStr;

// =================================================================
// SECTION 2: ANY TYPE ASSIGNMENTS (VALID)
// =================================================================

// Any is assignable to everything
declare const anyVal: any;
const anyToStr: string = anyVal;
const anyToNum: number = anyVal;
const anyToBool: boolean = anyVal;
const anyToObj: object = anyVal;
const anyToArr: any[] = anyVal;

// Everything is assignable to any
const strToAny: any = "hello";
const numToAny: any = 42;
const objToAny: any = { x: 1 };

// =================================================================
// SECTION 3: UNKNOWN TOP TYPE (VALID ASSIGNMENTS TO)
// =================================================================

// Everything is assignable to unknown
const strToUnknown: unknown = "hello";
const numToUnknown: unknown = 42;
const boolToUnknown: unknown = true;
const objToUnknown: unknown = { x: 1 };
const nullToUnknown: unknown = null;
const undefinedToUnknown: unknown = undefined;

// =================================================================
// SECTION 4: NEVER BOTTOM TYPE (VALID)
// =================================================================

// Never is assignable to everything
function throwsError(): never {
    throw new Error("always throws");
}

const neverToStr: string = throwsError();
const neverToNum: number = throwsError();
const neverToBool: boolean = throwsError();
const neverToObj: object = throwsError();
const neverToUnion: string | number = throwsError();

// =================================================================
// SECTION 5: STRUCTURAL SUBTYPING (VALID)
// =================================================================

interface Point {
    x: number;
    y: number;
}

interface Point3D {
    x: number;
    y: number;
    z: number;
}

// Subtype (more properties) to supertype is valid
const point3d: Point3D = { x: 1, y: 2, z: 3 };
const point: Point = point3d;

// Object with extra properties assigned via variable (non-fresh)
const widened = { x: 1, y: 2, extra: "bonus" };
const pointFromWidened: Point = widened;

// =================================================================
// SECTION 6: INTERFACE EXTENSION (VALID)
// =================================================================

interface Animal {
    name: string;
}

interface Dog extends Animal {
    breed: string;
}

const dog: Dog = { name: "Rex", breed: "German Shepherd" };
const animal: Animal = dog;

// Multiple levels of extension
interface Mammal extends Animal {
    warmBlooded: boolean;
}

interface Cat extends Mammal {
    meows: boolean;
}

const cat: Cat = { name: "Whiskers", warmBlooded: true, meows: true };
const mammal: Mammal = cat;
const animal2: Animal = mammal;

// =================================================================
// SECTION 7: UNION TYPE ASSIGNMENTS (VALID)
// =================================================================

type StringOrNumber = string | number;

// Union members are assignable to union
const strToUnion: StringOrNumber = "hello";
const numToUnion: StringOrNumber = 42;

// Narrower union to wider union
type Narrow = "a" | "b";
type Wide = "a" | "b" | "c";

const narrow: Narrow = "a";
const wide: Wide = narrow;

// Union to optional (union with undefined)
const unionToOptional: string | undefined = "test";

// =================================================================
// SECTION 8: INTERSECTION TYPE ASSIGNMENTS (VALID)
// =================================================================

interface A { a: string; }
interface B { b: number; }

type AB = A & B;

// Object satisfying intersection
const ab: AB = { a: "test", b: 42 };

// Intersection assignable to constituent types
const aFromAB: A = ab;
const bFromAB: B = ab;

// =================================================================
// SECTION 9: ARRAY TYPE COVARIANCE (VALID)
// =================================================================

// Array of subtypes assignable to array of supertypes
const dogs: Dog[] = [{ name: "Fido", breed: "Lab" }];
const animals: Animal[] = dogs;

// Same element type
const nums: number[] = [1, 2, 3];
const numsRef: number[] = nums;

// Tuple to array
const tuple: [number, number] = [1, 2];
const arr: number[] = tuple;

// =================================================================
// SECTION 10: FUNCTION TYPE ASSIGNMENTS (VALID)
// =================================================================

// Return type covariance
const getDog = (): Dog => ({ name: "Rex", breed: "Lab" });
const getAnimal: () => Animal = getDog;

// Parameter contravariance - function taking broader type assignable to one taking narrower
const processAnimal = (a: Animal): void => console.log(a.name);
const processDog: (d: Dog) => void = processAnimal;

// Function with fewer parameters assignable to one with more
const oneParam = (x: number): number => x * 2;
const twoParams: (x: number, y: number) => number = oneParam;

// Optional parameters
const required = (x: number): void => {};
const optional: (x?: number) => void = required;

// =================================================================
// SECTION 11: GENERIC TYPE ASSIGNMENTS (VALID)
// =================================================================

interface Box<T> {
    value: T;
}

// Same type parameter
const strBox: Box<string> = { value: "hello" };
const strBoxRef: Box<string> = strBox;

// Generic structural compatibility
interface Container<T> {
    value: T;
}

const boxToContainer: Container<string> = strBox;

// =================================================================
// SECTION 12: OPTIONAL PROPERTY ASSIGNMENTS (VALID)
// =================================================================

interface Config {
    host?: string;
    port?: number;
}

// Partial object satisfies optional interface
const partial: Config = { host: "localhost" };
const empty: Config = {};
const full: Config = { host: "localhost", port: 8080 };

// Required to optional
interface Required {
    host: string;
    port: number;
}

const required2: Required = { host: "localhost", port: 8080 };
const configFromRequired: Config = required2;

// =================================================================
// SECTION 13: READONLY ASSIGNMENTS (VALID)
// =================================================================

interface Mutable {
    x: number;
}

interface Readonly {
    readonly x: number;
}

// Mutable to readonly is fine
const mutable: Mutable = { x: 1 };
const readonlyFromMutable: Readonly = mutable;

// Readonly array from mutable
const mutableArr = [1, 2, 3];
const readonlyArr: readonly number[] = mutableArr;

// =================================================================
// SECTION 14: NULL AND UNDEFINED IN UNIONS (VALID)
// =================================================================

// Null in union
const nullStr: string | null = null;
const nonNullStr: string | null = "hello";

// Undefined in union
const undefinedNum: number | undefined = undefined;
const nonUndefinedNum: number | undefined = 42;

// Both in union
const maybeStr: string | null | undefined = null;
const maybeStr2: string | null | undefined = undefined;
const maybeStr3: string | null | undefined = "hello";

// =================================================================
// SECTION 15: LITERAL TYPE UNIONS (VALID)
// =================================================================

type Direction = "north" | "south" | "east" | "west";

const north: Direction = "north";
const south: Direction = "south";
const east: Direction = "east";
const west: Direction = "west";

// Literal to union
const singleDirection: "north" = "north";
const directionFromLiteral: Direction = singleDirection;

// =================================================================
// SECTION 16: ENUM ASSIGNMENTS (VALID)
// =================================================================

enum Color {
    Red,
    Green,
    Blue
}

const red: Color = Color.Red;
const colorNum: Color = Color.Green;

// Numeric enum to number
const numFromEnum: number = Color.Red;

// =================================================================
// SECTION 17: CLASS ASSIGNMENTS (VALID)
// =================================================================

class BaseClass {
    base: string = "base";
}

class DerivedClass extends BaseClass {
    derived: string = "derived";
}

// Subclass to base class
const derived = new DerivedClass();
const base: BaseClass = derived;

// Structural compatibility with class
const structurallyCompatible: BaseClass = { base: "compatible" };

// =================================================================
// SECTION 18: PROMISE TYPE ASSIGNMENTS (VALID)
// =================================================================

// Promise of subtype to promise of supertype
const dogPromise: Promise<Dog> = Promise.resolve({ name: "Rex", breed: "Lab" });
const animalPromise: Promise<Animal> = dogPromise;

// Async function return types
async function getAsyncAnimal(): Promise<Animal> {
    return { name: "Animal" };
}

async function getAsyncDog(): Promise<Dog> {
    return { name: "Rex", breed: "Lab" };
}

// =================================================================
// SECTION 19: INDEX SIGNATURES (VALID)
// =================================================================

interface StringDict {
    [key: string]: string;
}

// All string values
const stringDict: StringDict = {
    name: "test",
    value: "hello"
};

// Mixed index signature
interface MixedDict {
    [key: string]: string | number;
}

const mixedDict: MixedDict = {
    name: "test",
    count: 42
};

// =================================================================
// SECTION 20: CALLABLE AND CONSTRUCTABLE (VALID)
// =================================================================

interface Callable {
    (x: number): string;
}

const callable: Callable = (x) => x.toString();

interface Constructable {
    new (x: number): { value: number };
}

class Impl {
    constructor(public value: number) {}
}

const constructable: Constructable = Impl;

// =================================================================
// SECTION 21: MAPPED TYPE ASSIGNMENTS (VALID)
// =================================================================

interface Original {
    name: string;
    age: number;
}

type Partial2<T> = { [P in keyof T]?: T[P] };
type Required2<T> = { [P in keyof T]-?: T[P] };
type Readonly2<T> = { readonly [P in keyof T]: T[P] };

const original: Original = { name: "test", age: 25 };
const partialFromOriginal: Partial2<Original> = original;
const readonlyFromOriginal: Readonly2<Original> = original;

// =================================================================
// SECTION 22: CONDITIONAL TYPE ASSIGNMENTS (VALID)
// =================================================================

type IsString<T> = T extends string ? true : false;

// Conditional resolves correctly
const isStringTrue: IsString<string> = true;
const isStringFalse: IsString<number> = false;

type Flatten<T> = T extends any[] ? T[number] : T;
const flattened: Flatten<number[]> = 42;
const notFlattened: Flatten<string> = "hello";

// =================================================================
// SECTION 23: TEMPLATE LITERAL TYPES (VALID)
// =================================================================

type Greeting = `Hello, ${string}`;

const greeting: Greeting = "Hello, World";
const greeting2: Greeting = "Hello, TypeScript";

type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";
type Endpoint = `/${string}`;
type Route = `${HttpMethod} ${Endpoint}`;

const route: Route = "GET /users";
const route2: Route = "POST /api/data";

// =================================================================
// SECTION 24: DISCRIMINATED UNIONS (VALID)
// =================================================================

interface Square {
    kind: "square";
    size: number;
}

interface Circle {
    kind: "circle";
    radius: number;
}

type Shape = Square | Circle;

const square: Shape = { kind: "square", size: 10 };
const circle: Shape = { kind: "circle", radius: 5 };

function getArea(shape: Shape): number {
    switch (shape.kind) {
        case "square":
            return shape.size * shape.size;
        case "circle":
            return Math.PI * shape.radius * shape.radius;
    }
}

// =================================================================
// SECTION 25: TYPE GUARDS AND NARROWING (VALID)
// =================================================================

function isString(x: unknown): x is string {
    return typeof x === "string";
}

function process(x: unknown): void {
    if (isString(x)) {
        const str: string = x; // Narrowed
    }
}

// Type narrowing with typeof
function narrowTypeof(x: string | number): void {
    if (typeof x === "string") {
        const s: string = x;
    } else {
        const n: number = x;
    }
}

// Type narrowing with instanceof
function narrowInstanceof(x: Animal | Dog): void {
    if (x instanceof Object) {
        const a: Animal = x;
    }
}

// =================================================================
// SECTION 26: GENERIC CONSTRAINTS (VALID)
// =================================================================

interface Lengthwise {
    length: number;
}

function logLength<T extends Lengthwise>(arg: T): T {
    console.log(arg.length);
    return arg;
}

// String satisfies Lengthwise
logLength("hello");

// Array satisfies Lengthwise
logLength([1, 2, 3]);

// Object with length satisfies Lengthwise
logLength({ length: 10, extra: "data" });

// =================================================================
// SECTION 27: THIS TYPES (VALID)
// =================================================================

class FluentBuilder {
    private value = 0;

    add(n: number): this {
        this.value += n;
        return this;
    }

    multiply(n: number): this {
        this.value *= n;
        return this;
    }
}

class ExtendedBuilder extends FluentBuilder {
    subtract(n: number): this {
        return this;
    }
}

const builder = new ExtendedBuilder();
const result = builder.add(1).multiply(2).subtract(1);

// =================================================================
// SECTION 28: TUPLE TYPES (VALID)
// =================================================================

type Pair = [string, number];

const pair: Pair = ["hello", 42];

// Rest elements in tuple
type StringAndNumbers = [string, ...number[]];
const stringAndNumbers: StringAndNumbers = ["start", 1, 2, 3];

// Optional elements in tuple
type OptionalTuple = [string, number?];
const optionalFull: OptionalTuple = ["test", 42];
const optionalPartial: OptionalTuple = ["test"];

// =================================================================
// SECTION 29: SYMBOL TYPES (VALID)
// =================================================================

const sym1 = Symbol("test");
const sym2 = Symbol("test");

// Symbol to symbol
const symRef: symbol = sym1;

// Unique symbol to symbol (widening)
declare const uniqueSym: unique symbol;
const symbolFromUnique: symbol = uniqueSym;

// =================================================================
// SECTION 30: RECURSIVE TYPES (VALID)
// =================================================================

interface TreeNode {
    value: string;
    children: TreeNode[];
}

const tree: TreeNode = {
    value: "root",
    children: [
        {
            value: "child1",
            children: []
        },
        {
            value: "child2",
            children: [
                {
                    value: "grandchild",
                    children: []
                }
            ]
        }
    ]
};

// JSON type (recursive)
type JSONValue = string | number | boolean | null | JSONValue[] | { [key: string]: JSONValue };

const jsonValue: JSONValue = {
    name: "test",
    count: 42,
    active: true,
    tags: ["a", "b"],
    nested: {
        value: 1
    }
};

console.log("All valid assignments - no errors expected");
