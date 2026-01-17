// =================================================================
// STRUCTURAL TYPE CHECKING TESTS
// Tests for structural type compatibility between object types
// =================================================================

// =================================================================
// SECTION 1: OBJECT TYPES WITH SAME PROPERTIES
// =================================================================

// Basic structural compatibility - objects with same shape are compatible
interface Point2D {
    x: number;
    y: number;
}

interface NamedPoint {
    x: number;
    y: number;
}

// Valid - same structure, compatible
const p1: Point2D = { x: 1, y: 2 };
const p2: NamedPoint = p1; // OK - structural compatibility

// Valid - object literal with same properties
const p3: Point2D = { x: 0, y: 0 };

// Extra properties - when assigned to variable first, no excess property check
const temp = { x: 1, y: 2, z: 3 };
const p4: Point2D = temp; // OK - temp is not fresh

// Fresh object literal - excess properties should error in some cases
// but for now we're testing structural compatibility

// Property type compatibility
interface Point {
    x: number;
    y: number;
}

interface StringPoint {
    x: string;
    y: string;
}

// @ts-expect-error - Type 'StringPoint' is not assignable to type 'Point'
const sp1: Point = { x: "hello", y: "world" } as StringPoint;

// @ts-expect-error - Type 'Point' is not assignable to type 'StringPoint'
const sp2: StringPoint = p1;

// =================================================================
// SECTION 2: INTERFACE COMPATIBILITY
// =================================================================

// Subset/superset relationships
interface Animal {
    name: string;
}

interface Dog {
    name: string;
    breed: string;
}

// Valid - Dog has all Animal properties (superset)
const dog: Dog = { name: "Fido", breed: "Labrador" };
const animal: Animal = dog; // OK - Dog is superset of Animal

// @ts-expect-error - Animal does not have 'breed' property
const dog2: Dog = { name: "Spot" } as Animal;

// Multiple interfaces with structural compatibility
interface Person {
    firstName: string;
    lastName: string;
}

interface Employee {
    firstName: string;
    lastName: string;
    employeeId: number;
}

const emp: Employee = { firstName: "John", lastName: "Doe", employeeId: 123 };
const person: Person = emp; // OK - Employee is superset of Person

// @ts-expect-error - Missing 'employeeId'
const emp2: Employee = { firstName: "Jane", lastName: "Smith" } as Person;

// Interface with optional properties
interface Config {
    host?: string;
    port?: number;
}

interface FullConfig {
    host: string;
    port: number;
}

// Valid - FullConfig satisfies Config
const fc: FullConfig = { host: "localhost", port: 8080 };
const cfg: Config = fc; // OK

// Valid - Config with all properties satisfies FullConfig
const cfg2: Config = { host: "example.com", port: 443 };
// @ts-expect-error - Properties might be undefined
const fc2: FullConfig = cfg2; // Error - optional vs required

// Interface with methods
interface Greeting {
    greet(): string;
}

interface FormalGreeting {
    greet(): string;
    farewell(): string;
}

const formal: FormalGreeting = {
    greet() { return "Hello"; },
    farewell() { return "Goodbye"; }
};

// Valid - FormalGreeting is superset of Greeting
const greeting: Greeting = formal; // OK

// =================================================================
// SECTION 3: CLASS STRUCTURAL TYPING
// =================================================================

// Classes are compared structurally (unless they have private/protected members)
class PointClass {
    x: number;
    y: number;
}

interface IPoint {
    x: number;
    y: number;
}

// Valid - class and interface with same structure
const pointClass: PointClass = new PointClass();
const ipoint: IPoint = pointClass; // OK - structural

// Valid - interface can be satisfied by class instance
const pointClass2: PointClass = { x: 1, y: 2 }; // OK when not using constructor

// Private members break structural typing (make classes nominal)
class WithPrivate {
    private secret: string;
    public value: number;
    constructor(secret: string, value: number) {
        this.secret = secret;
        this.value = value;
    }
}

class WithPrivate2 {
    private secret: string;
    public value: number;
    constructor(secret: string, value: number) {
        this.secret = secret;
        this.value = value;
    }
}

// @ts-expect-error - Private members make classes nominal, not structural
const wp1: WithPrivate = new WithPrivate2("s", 1);

// Protected members also make classes nominal
class WithProtected {
    protected data: string;
    constructor(data: string) {
        this.data = data;
    }
}

class WithProtected2 {
    protected data: string;
    constructor(data: string) {
        this.data = data;
    }
}

// @ts-expect-error - Protected members break structural typing
const wpp1: WithProtected = new WithProtected2("data");

// =================================================================
// SECTION 4: INTERSECTION TYPES
// =================================================================

interface HasX {
    x: number;
}

interface HasY {
    y: number;
}

interface HasZ {
    z: number;
}

type Point3D = HasX & HasY & HasZ;

// Valid - intersection requires all properties
const p3d: Point3D = { x: 1, y: 2, z: 3 };

// @ts-expect-error - Missing 'z' property
const p3d2: Point3D = { x: 1, y: 2 };

// @ts-expect-error - Wrong type for 'z'
const p3d3: Point3D = { x: 1, y: 2, z: "3" };

// Intersection with overlapping properties
interface A {
    name: string;
    value: number;
}

interface B {
    name: string;
    flag: boolean;
}

type AB = A & B;

// Valid - overlapping property 'name' has same type
const ab1: AB = { name: "test", value: 42, flag: true };

// Conflicting intersection properties reduce to never
interface X {
    value: string;
}

interface Y {
    value: number;
}

type XY = X & Y;

// @ts-expect-error - 'value' is never (string & number)
const xy1: XY = { value: "test" as any };

// =================================================================
// SECTION 5: UNION TYPES
// =================================================================

type StringOrNumber = string | number;

// Valid - union members
const u1: StringOrNumber = "hello";
const u2: StringOrNumber = 42;

// @ts-expect-error - boolean not in union
const u3: StringOrNumber = true;

// Union of object types (discriminated unions)
interface Square {
    kind: "square";
    size: number;
}

interface Circle {
    kind: "circle";
    radius: number;
}

type Shape = Square | Circle;

// Valid - union members
const shape1: Shape = { kind: "square", size: 10 };
const shape2: Shape = { kind: "circle", radius: 5 };

// @ts-expect-error - Invalid discriminant
const shape3: Shape = { kind: "triangle", base: 10 };

// @ts-expect-error - Missing discriminant
const shape4: Shape = { size: 10 };

// Union with shared property
interface A1 {
    type: "a";
    shared: string;
}

interface B1 {
    type: "b";
    shared: string;
}

type A1B1 = A1 | B1;

function processType(t: A1B1) {
    // Shared property accessible on both types
    return t.shared;
}

// =================================================================
// SECTION 6: GENERIC STRUCTURAL TYPING
// =================================================================

interface Box<T> {
    value: T;
}

interface Container<T> {
    value: T;
}

// Valid - same structure, different names
const boxNum: Box<number> = { value: 42 };
const containerNum: Container<number> = boxNum; // OK - structural

// Generic type constraints
interface WithLength {
    length: number;
}

function getLength<T extends WithLength>(item: T): number {
    return item.length;
}

// Valid - string has length property
const len1 = getLength("hello");

// Valid - array has length property
const len2 = getLength([1, 2, 3]);

// @ts-expect-error - number does not have length
const len3 = getLength(42);

// =================================================================
// SECTION 7: INDEX SIGNATURES
// =================================================================

interface StringDictionary {
    [key: string]: string;
}

interface NumericDictionary {
    [index: number]: string;
}

// Valid - numeric index signature
const nd1: NumericDictionary = { 0: "a", 1: "b" };
const nd2: NumericDictionary = ["x", "y", "z"];

// @ts-expect-error - number not assignable to string
const sd1: StringDictionary = { name: "test", count: 42 };

// Valid - all strings
const sd2: StringDictionary = { name: "test", value: "hello" };

// Index signature with named properties
interface Mixed {
    [key: string]: number;
    length: number; // Must match index signature type
}

// Valid
const m1: Mixed = { length: 10, other: 20 };

// @ts-expect-error - 'name' is string, not number
const m2: Mixed = { length: 10, name: "test" };
