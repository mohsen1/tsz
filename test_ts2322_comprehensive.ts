// Comprehensive TS2322 assignability error tests
// These tests cover various scenarios where TS2322 should be emitted

// 1. Return statement type mismatches
function returnNumber(): number {
    return "string"; // TS2322
}

function returnString(): string {
    return 42; // TS2322
}

function returnObject(): { a: number } {
    return { a: "string" }; // TS2322
}

function returnArray(): number[] {
    return ["string"]; // TS2322
}

// 2. Variable declaration with type annotation
let x: number = "string"; // TS2322
let y: { a: number } = { a: "string" }; // TS2322
let z: string[] = [1, 2, 3]; // TS2322

// 3. Assignment expressions
let a: number;
a = "string"; // TS2322

let obj: { a: number };
obj = { a: "string" }; // TS2322

// 4. Property assignments
interface PropTarget {
    prop: number;
}

const t: PropTarget = { prop: 42 };
t.prop = "string"; // TS2322

// 5. Array destructuring
const [num]: number = ["string"]; // TS2322

// 6. Object destructuring
const { prop }: { prop: number } = { prop: "string" }; // TS2322

// 7. Function parameter type mismatches in calls
function takesNumber(n: number): void {}
takesNumber("string"); // TS2322

function takesObject(o: { a: number }): void {}
takesObject({ a: "string" }); // TS2322

// 8. Class property assignments
class MyClass {
    prop: number;
    constructor() {
        this.prop = "string"; // TS2322
    }
}

// 9. Union type assignability
type StringOrNumber = string | number;

let union: StringOrNumber;
let unionTarget: StringOrNumber = true; // TS2322

// 10. Generic type parameter mismatches
function identity<T>(x: T): T {
    return x;
}

const result: number = identity("string"); // TS2322

// 11. Promise type mismatches
async function returnsPromiseString(): Promise<string> {
    return 42; // TS2322
}

async function returnsPromiseNumber(): Promise<number> {
    return "string"; // TS2322
}

// 12. Tuple assignability
type Tuple = [number, string];
const tuple: Tuple = ["string", 42]; // TS2322 - reversed

// 13. Literal type mismatches
type Literal = "a" | "b";
const literal: Literal = "c"; // TS2322

// 14. Enum assignability
enum Enum {
    A = 0,
    B = 1
}

const enumVar: Enum = 2; // TS2322

// 15. Strict null checks
let withNull: number = null; // TS2322 (strictNullChecks)
let withUndefined: string = undefined; // TS2322 (strictNullChecks)

// 16. Function type assignability
type NumberToString = (n: number) => string;
type StringToNumber = (s: string) => number;

let fn1: NumberToString = (s: string) => s.length; // TS2322 - wrong param type
let fn2: StringToNumber = (n: number) => n.toString(); // TS2322 - wrong param type

// 17. Interface assignability
interface Animal {
    name: string;
}

interface Dog {
    name: string;
    bark: () => void;
}

let animal: Animal;
const dog: Dog = {
    name: "Fido",
    bark: () => console.log("woof")
};

// This should work (Dog is assignable to Animal)
animal = dog; // OK

// This should not work
const dog2: Dog = {
    name: "Fido",
    bark: 123 // TS2322
};

// 18. Readonly vs mutable
interface ReadonlyPoint {
    readonly x: number;
    readonly y: number;
}

interface Point {
    x: number;
    y: number;
}

let readonlyPoint: ReadonlyPoint = { x: 1, y: 2 };
let point: Point = readonlyPoint; // This works (mutable can be assigned from readonly)

// But excess properties
const rp: ReadonlyPoint = { x: 1, y: 2, z: 3 }; // TS2322 - excess property

// 19. Optional property mismatches
interface WithOptional {
    a?: number;
}

interface WithoutOptional {
    a: number;
}

let withOpt: WithOptional = { a: 1 };
let withoutOpt: WithoutOptional = { a: 1 };

withoutOpt = withOpt; // This should work
withOpt = withoutOpt; // This should also work

// 20. Index signature mismatches
interface WithIndex {
    [key: string]: number;
}

const withIndex: WithIndex = { a: "string" }; // TS2322

// 21. Class vs interface assignability
class PointClass {
    x: number;
    y: number;
}

interface IPoint {
    x: number;
    y: number;
}

let pointClass: PointClass = new PointClass();
let iPoint: IPoint = pointClass; // OK

// But wrong types
const iPoint2: IPoint = { x: "string", y: 2 }; // TS2322

// 22. Abstract class assignability
abstract class AbstractClass {
    abstract method(): void;
}

class ConcreteClass extends AbstractClass {
    method(): void {}
}

let abstract: AbstractClass = new ConcreteClass(); // OK

// 23. Intersection types
type A = { a: number };
type B = { b: string };

type AB = A & B;

const ab: AB = { a: 1, b: 2 };
const ab2: AB = { a: "string", b: 2 }; // TS2322

// 24. Type predicates
function isString(x: unknown): x is string {
    return typeof x === "string";
}

let predicate: (x: unknown) => x is string = isString;
let predicate2: (x: unknown) => x is number = isString; // TS2322

// 25. This type mismatches
interface ThisInterface {
    method(this: { prop: number }): void;
}

const impl: ThisInterface = {
    method: function() { console.log(this.prop); } // TS2322 - missing prop
};

// 26. Constructor type mismatches
interface Constructable {
    new(x: number): string;
}

const constructable: Constructable = class {
    constructor(x: string) {} // TS2322
};

// 27. Array covariance
let arrayOfNumbers: number[] = [1, 2, 3];
let arrayOfStrings: string[] = arrayOfNumbers; // TS2322

// 28. Function return covariance
type NumberFunc = () => number;
type StringFunc = () => string;

let nf: NumberFunc = () => 42;
let sf: StringFunc = nf; // TS2322

// 29. Generic constraints
interface WithLength {
    length: number;
}

function getLength<T extends WithLength>(x: T): number {
    return x.length;
}

const noLength: number = getLength(42); // TS2322

// 30. Conditional type mismatches
type NonNullable<T> = T extends null | undefined ? never : T;

let nn: NonNullable<string | null> = "string";
let nn2: NonNullable<string | null> = null; // TS2322

// 31. Mapped type mismatches
type Readonly<T> = {
    readonly [P in keyof T]: T[P];
};

type Mutable = {
    a: number;
    b: string;
};

const readonly: Readonly<Mutable> = { a: 1, b: "test" };
const readonly2: Readonly<Mutable> = { a: "string", b: "test" }; // TS2322

// 32. Template literal type mismatches
type Greeting = `hello ${string}`;

const greeting: Greeting = "goodbye world"; // TS2322

// 33. Branded type mismatches
type Brand = number & { __brand: "Brand" };

const brand: Brand = 42; // TS2322

// 34. Omit type mismatches
type Person = {
    name: string;
    age: number;
};

type WithoutAge = Omit<Person, "age">;

const withoutAge: WithoutAge = { name: "John", age: 30 }; // TS2322 - excess property

// 35. Partial type mismatches
type PartialPerson = Partial<Person>;

const partialPerson: PartialPerson = { name: "John", age: "string" }; // TS2322

// 36. Required type mismatches
type RequiredPerson = Required<PartialPerson>;

const requiredPerson: RequiredPerson = { name: "John" }; // TS2322 - missing age

// 37. Pick type mismatches
type NameOnly = Pick<Person, "name">;

const nameOnly: NameOnly = { name: "John", age: 30 }; // TS2322 - excess property

// 38. Record type mismatches
type RecordType = Record<string, number>;

const record: RecordType = { a: "string" }; // TS2322

// 39. Recursive type mismatches
type Tree = {
    value: number;
    children?: Tree[];
};

const tree: Tree = {
    value: 1,
    children: [
        { value: "string" } // TS2322
    ]
};

// 40. instanceof type guard mismatches
class Base {}
class Derived extends Base {}

let base: Base = new Base();
let derived: Derived = base; // TS2322 - base is not necessarily a Derived

console.log("TS2322 test file complete");
