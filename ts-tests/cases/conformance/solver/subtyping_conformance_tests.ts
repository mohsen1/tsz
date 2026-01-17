// =================================================================
// SUBTYPING CONFORMANCE TESTS
// Comprehensive tests for type assignability (TS2322) based on
// the 310 missing cases from differential testing.
//
// Each section focuses on a specific category of type mismatches
// that should emit TS2322 errors.
// =================================================================

// @strict: true
// @noEmit: true

// =================================================================
// SECTION 1: PRIMITIVE WIDENING ERRORS
// Literal types must widen correctly
// =================================================================

// String literal to number
// @ts-expect-error - Type '"hello"' is not assignable to type 'number'
const strLitToNum: number = "hello";

// Number literal to string
// @ts-expect-error - Type '42' is not assignable to type 'string'
const numLitToStr: string = 42;

// Boolean literal to string
// @ts-expect-error - Type 'true' is not assignable to type 'string'
const boolLitToStr: string = true;

// Symbol to string
const sym = Symbol("test");
// @ts-expect-error - Type 'symbol' is not assignable to type 'string'
const symToStr: string = sym;

// BigInt to number
const bigNum = 100n;
// @ts-expect-error - Type 'bigint' is not assignable to type 'number'
const bigToNum: number = bigNum;

// =================================================================
// SECTION 2: FUNCTION PARAMETER CONTRAVARIANCE
// Function parameters are contravariant
// =================================================================

type AnimalHandler = (animal: Animal) => void;
type DogHandler = (dog: Dog) => void;

interface Animal {
    name: string;
}

interface Dog extends Animal {
    breed: string;
}

// Valid: DogHandler can be assigned to AnimalHandler (widening parameter)
const dogHandler: DogHandler = (dog: Dog) => console.log(dog.breed);
const animalHandler1: AnimalHandler = dogHandler; // This should work

// @ts-expect-error - AnimalHandler is not assignable to DogHandler (narrowing)
const dogHandler2: DogHandler = ((animal: Animal) => console.log(animal.name)) as AnimalHandler;

// =================================================================
// SECTION 3: FUNCTION RETURN TYPE COVARIANCE
// Function return types are covariant
// =================================================================

type GetAnimal = () => Animal;
type GetDog = () => Dog;

const getDog: GetDog = () => ({ name: "Fido", breed: "Lab" });
const getAnimal1: GetAnimal = getDog; // Valid: Dog extends Animal

// @ts-expect-error - Return type 'Animal' is not assignable to 'Dog'
const getDog2: GetDog = (() => ({ name: "Spot" })) as GetAnimal;

// =================================================================
// SECTION 4: ARRAY TYPE COVARIANCE
// Array element types must be compatible
// =================================================================

// @ts-expect-error - Type 'string[]' is not assignable to type 'number[]'
const strArrayToNumArray: number[] = ["a", "b", "c"];

// @ts-expect-error - Type '(string | number)[]' is not assignable to type 'string[]'
const mixedToStr: string[] = [1, "two", 3] as (string | number)[];

// Nested array type mismatch
// @ts-expect-error - Type 'string[][]' is not assignable to type 'number[][]'
const nestedStrToNum: number[][] = [["a"], ["b"]];

// Array with object types
interface Item { id: number; }
interface NamedItem extends Item { name: string; }

const namedItems: NamedItem[] = [{ id: 1, name: "a" }];
const items: Item[] = namedItems; // Valid: NamedItem extends Item

// @ts-expect-error - Item[] is not assignable to NamedItem[]
const namedItems2: NamedItem[] = [{ id: 2 }] as Item[];

// =================================================================
// SECTION 5: TUPLE TYPE STRICTNESS
// Tuples have fixed length and positional types
// =================================================================

type Triple = [number, string, boolean];

// @ts-expect-error - Wrong order of types
const wrongOrder: Triple = ["hello", 42, true];

// @ts-expect-error - Too few elements
const tooFew: Triple = [1, "two"];

// @ts-expect-error - Wrong element type
const wrongElement: Triple = [1, "two", "three"];

// Nested tuple mismatch
type NestedTuple = [number, [string, boolean]];
// @ts-expect-error - Nested tuple type mismatch
const nestedWrong: NestedTuple = [1, [2, true]];

// Rest element in tuple
type TupleWithRest = [string, ...number[]];
// @ts-expect-error - First element must be string
const restWrong: TupleWithRest = [1, 2, 3];

// =================================================================
// SECTION 6: OBJECT TYPE STRUCTURAL SUBTYPING
// Object types must be structurally compatible
// =================================================================

interface Point {
    x: number;
    y: number;
}

interface Point3D extends Point {
    z: number;
}

interface ColorPoint {
    x: number;
    y: number;
    color: string;
}

// Valid structural subtyping
const point3d: Point3D = { x: 1, y: 2, z: 3 };
const point: Point = point3d; // Point3D has all Point properties

// @ts-expect-error - Missing 'z' property
const point3d2: Point3D = { x: 1, y: 2 } as Point;

// @ts-expect-error - Missing 'color' property
const colorPoint: ColorPoint = point3d;

// Property type mismatch
interface NumProps {
    a: number;
    b: number;
}

interface StrProps {
    a: string;
    b: string;
}

// @ts-expect-error - Property types don't match
const numToStr: StrProps = { a: 1, b: 2 } as NumProps;

// =================================================================
// SECTION 7: OPTIONAL VS REQUIRED PROPERTIES
// Required properties cannot be optional in source
// =================================================================

interface Required {
    name: string;
    age: number;
}

interface PartialRequired {
    name: string;
    age?: number;
}

const partial: PartialRequired = { name: "test" };
// @ts-expect-error - 'age' is optional but required in target
const required: Required = partial;

// The reverse is fine
const required2: Required = { name: "test", age: 25 };
const partial2: PartialRequired = required2; // OK

// =================================================================
// SECTION 8: READONLY VS MUTABLE PROPERTIES
// Readonly properties have assignability constraints
// =================================================================

interface Mutable {
    value: number;
}

interface Readonly {
    readonly value: number;
}

// Mutable to readonly is fine
const mutable: Mutable = { value: 1 };
const readonly1: Readonly = mutable; // OK

// @ts-expect-error - Cannot assign readonly to mutable (when fresh)
const mutableFromReadonly: Mutable = { value: 2 } as Readonly;

// =================================================================
// SECTION 9: INDEX SIGNATURE COMPATIBILITY
// Index signatures must be compatible
// =================================================================

interface StringIndex {
    [key: string]: string;
}

interface NumberIndex {
    [key: number]: string;
}

interface MixedIndex {
    [key: string]: string | number;
    [index: number]: number;
}

// @ts-expect-error - Index signature value types don't match
const strIdxToMixed: MixedIndex = { a: "test" } as StringIndex;

// Named property must match index signature
interface WithIndex {
    [key: string]: number;
    count: number;
}

// @ts-expect-error - Named property type must match index signature
interface BadWithIndex {
    [key: string]: number;
    name: string; // Error: 'string' not assignable to 'number'
}

// =================================================================
// SECTION 10: GENERIC TYPE PARAMETER CONSTRAINTS
// Generic constraints must be satisfied
// =================================================================

interface Lengthwise {
    length: number;
}

function acceptsLengthwise<T extends Lengthwise>(arg: T): T {
    return arg;
}

// @ts-expect-error - number has no length property
acceptsLengthwise(42);

// @ts-expect-error - boolean has no length property
acceptsLengthwise(true);

// Valid - string has length
acceptsLengthwise("hello");

// Valid - array has length
acceptsLengthwise([1, 2, 3]);

// =================================================================
// SECTION 11: GENERIC TYPE INSTANTIATION
// Generic types must instantiate correctly
// =================================================================

interface Container<T> {
    value: T;
    getValue(): T;
}

// @ts-expect-error - Type 'Container<string>' not assignable to 'Container<number>'
const numContainer: Container<number> = { value: "hello", getValue: () => "test" } as Container<string>;

// @ts-expect-error - getValue return type mismatch
const badContainer: Container<number> = {
    value: 42,
    getValue: () => "string"
};

// Multiple type parameters
interface Pair<T, U> {
    first: T;
    second: U;
}

// @ts-expect-error - Swapped type parameters
const swapped: Pair<string, number> = { first: 1, second: "two" };

// =================================================================
// SECTION 12: UNION TO NON-UNION ASSIGNMENT
// Union types cannot be assigned to narrower types without narrowing
// =================================================================

type StringOrNumber = string | number;
type StringOrBoolean = string | boolean;

declare const strOrNum: StringOrNumber;

// @ts-expect-error - Union 'string | number' not assignable to 'string'
const justStr: string = strOrNum;

// @ts-expect-error - Union 'string | number' not assignable to 'number'
const justNum: number = strOrNum;

// @ts-expect-error - Incompatible unions
const strOrBool: StringOrBoolean = strOrNum;

// =================================================================
// SECTION 13: INTERSECTION TYPE REQUIREMENTS
// Intersection types require all constituent properties
// =================================================================

interface A {
    propA: string;
}

interface B {
    propB: number;
}

type AandB = A & B;

// @ts-expect-error - Missing propB
const onlyA: AandB = { propA: "test" };

// @ts-expect-error - Missing propA
const onlyB: AandB = { propB: 42 };

// @ts-expect-error - Wrong type for propA
const wrongTypes: AandB = { propA: 123, propB: 456 };

// Valid
const bothProps: AandB = { propA: "test", propB: 42 };

// =================================================================
// SECTION 14: CLASS NOMINAL TYPING (PRIVATE/PROTECTED)
// Classes with private/protected members are nominally typed
// =================================================================

class ClassA {
    private secret: string = "a";
    value: number = 1;
}

class ClassB {
    private secret: string = "b";
    value: number = 2;
}

const instA = new ClassA();
const instB = new ClassB();

// @ts-expect-error - Private members make classes nominal
const aFromB: ClassA = instB;

// @ts-expect-error - Private members make classes nominal
const bFromA: ClassB = instA;

// Subclass is fine
class SubClassA extends ClassA {
    extra: boolean = true;
}

const subA = new SubClassA();
const baseA: ClassA = subA; // OK - inheritance

// =================================================================
// SECTION 15: MAPPED TYPE ASSIGNABILITY
// Mapped types transform property types
// =================================================================

type ReadonlyPoint = {
    readonly [K in keyof Point]: Point[K];
};

type PartialPoint = {
    [K in keyof Point]?: Point[K];
};

type RequiredPartial<T> = {
    [K in keyof T]-?: T[K];
};

// Partial to required
const partialPoint: PartialPoint = { x: 1 };
// @ts-expect-error - Partial type may be missing properties
const reqFromPartial: Point = partialPoint;

// Required from partial (all provided)
const fullPartial: PartialPoint = { x: 1, y: 2 };
const reqFromFull: RequiredPartial<PartialPoint> = { x: 1, y: 2 };

// =================================================================
// SECTION 16: CONDITIONAL TYPE ASSIGNABILITY
// Conditional types must resolve correctly
// =================================================================

type IsString<T> = T extends string ? true : false;
type IsNumber<T> = T extends number ? "yes" : "no";

// @ts-expect-error - Conditional type resolves to false
const isStrNum: IsString<number> = true;

// @ts-expect-error - Conditional type resolves to 'yes'
const isNumStr: IsNumber<number> = "no";

// Valid
const isStrStr: IsString<string> = true;
const isNumNum: IsNumber<number> = "yes";

// =================================================================
// SECTION 17: TEMPLATE LITERAL TYPE ASSIGNABILITY
// Template literal types have specific patterns
// =================================================================

type Greeting = `Hello, ${string}`;
type IdString = `id-${number}`;

// @ts-expect-error - Doesn't match template pattern
const badGreeting: Greeting = "Hi there";

// @ts-expect-error - Doesn't match template pattern
const badId: IdString = "user-123";

// Valid
const goodGreeting: Greeting = "Hello, World";
const goodId: IdString = "id-42";

// =================================================================
// SECTION 18: PROMISE TYPE UNWRAPPING
// Promise types have specific assignability rules
// =================================================================

async function asyncNum(): Promise<number> {
    return 42;
}

// @ts-expect-error - Promise<number> not assignable to number
const notAwaited: number = asyncNum();

// @ts-expect-error - Promise<string> not assignable to Promise<number>
const wrongPromise: Promise<number> = Promise.resolve("hello");

// Valid with await
async function validAwait() {
    const num: number = await asyncNum(); // OK
}

// =================================================================
// SECTION 19: ENUM TYPE ASSIGNABILITY
// Enums have specific type relationships
// =================================================================

enum Color {
    Red = 0,
    Green = 1,
    Blue = 2
}

enum Size {
    Small = 0,
    Medium = 1,
    Large = 2
}

const red: Color = Color.Red;

// @ts-expect-error - Different enum types
const colorFromSize: Color = Size.Small;

// String enums are even stricter
enum StringColor {
    Red = "RED",
    Green = "GREEN",
    Blue = "BLUE"
}

// @ts-expect-error - String literal not assignable to string enum
const strEnumFromLit: StringColor = "RED";

// Valid enum usage
const enumVal: StringColor = StringColor.Red;

// =================================================================
// SECTION 20: VOID VS UNDEFINED
// void and undefined have different semantics
// =================================================================

type VoidFn = () => void;
type UndefinedFn = () => undefined;

const voidFn: VoidFn = () => {}; // OK - implicit void
// @ts-expect-error - void is not assignable to undefined
const undefinedFn: UndefinedFn = voidFn;

// Return type constraints
function mustReturnUndefined(): undefined {
    return undefined;
}

// @ts-expect-error - No return statement doesn't satisfy undefined return
function badReturnUndefined(): undefined {
    // Missing return
}

// =================================================================
// SECTION 21: NEVER TYPE ASSIGNABILITY
// never is the bottom type
// =================================================================

function throwsError(): never {
    throw new Error("always throws");
}

// never is assignable to everything
const neverToStr: string = throwsError();
const neverToNum: number = throwsError();
const neverToObj: object = throwsError();

// @ts-expect-error - Non-never types not assignable to never
const strToNever: never = "hello" as string;

// @ts-expect-error - number not assignable to never
const numToNever: never = 42 as number;

// =================================================================
// SECTION 22: UNKNOWN TYPE RESTRICTIONS
// unknown requires type narrowing before use
// =================================================================

declare const unknownVal: unknown;

// @ts-expect-error - unknown not assignable to string
const unknownToStr: string = unknownVal;

// @ts-expect-error - unknown not assignable to number
const unknownToNum: number = unknownVal;

// @ts-expect-error - unknown not assignable to object
const unknownToObj: object = unknownVal;

// With narrowing it's fine
function useUnknown(val: unknown) {
    if (typeof val === "string") {
        const str: string = val; // OK after narrowing
    }
}

// =================================================================
// SECTION 23: NULL AND UNDEFINED IN STRICT MODE
// null and undefined are not assignable in strict mode
// =================================================================

// @ts-expect-error - null not assignable to string
const nullToStr: string = null;

// @ts-expect-error - undefined not assignable to string
const undefinedToStr: string = undefined;

// @ts-expect-error - null not assignable to number
const nullToNum: number = null;

// @ts-expect-error - undefined not assignable to boolean
const undefinedToBool: boolean = undefined;

// Valid with union types
const nullableStr: string | null = null;
const optionalNum: number | undefined = undefined;

// =================================================================
// SECTION 24: DISCRIMINATED UNION EXHAUSTIVENESS
// Discriminated unions require complete discrimination
// =================================================================

interface Square {
    kind: "square";
    size: number;
}

interface Circle {
    kind: "circle";
    radius: number;
}

interface Triangle {
    kind: "triangle";
    base: number;
    height: number;
}

type Shape = Square | Circle | Triangle;

// @ts-expect-error - Object doesn't match any discriminant
const invalidShape: Shape = { kind: "rectangle", width: 10 };

// @ts-expect-error - Wrong properties for discriminant
const wrongProps: Shape = { kind: "square", radius: 5 };

// @ts-expect-error - Missing required property for discriminant
const missingProp: Shape = { kind: "circle" };

// Valid
const validSquare: Shape = { kind: "square", size: 10 };
const validCircle: Shape = { kind: "circle", radius: 5 };

// =================================================================
// SECTION 25: SYMBOL TYPE UNIQUENESS
// unique symbol types are distinct
// =================================================================

declare const sym1: unique symbol;
declare const sym2: unique symbol;

// @ts-expect-error - Unique symbols are distinct types
const symSwap: typeof sym1 = sym2;

// Regular symbol is a broader type
const regularSym: symbol = sym1; // OK - widening
const regularSym2: symbol = sym2; // OK

// @ts-expect-error - Symbol not assignable to unique symbol
const uniqueFromReg: typeof sym1 = Symbol("new");

// =================================================================
// SECTION 26: CALL SIGNATURE COMPATIBILITY
// Call signatures must be compatible
// =================================================================

interface Callable {
    (x: number): string;
}

interface BiCallable {
    (x: number): string;
    (x: string): number;
}

const callable: Callable = (x: number) => x.toString();

// @ts-expect-error - BiCallable requires both overloads
const biCallable: BiCallable = callable;

// Interface with construct signature
interface Newable {
    new (x: number): { value: number };
}

interface PlainObj {
    (x: number): { value: number };
}

declare const newable: Newable;
// @ts-expect-error - Construct signature vs call signature
const plainFromNew: PlainObj = newable;

// =================================================================
// SECTION 27: THIS TYPE IN METHODS
// Methods with this types have constraints
// =================================================================

interface Chainable {
    setValue(value: number): this;
}

class ChainableImpl implements Chainable {
    private val: number = 0;
    setValue(value: number): this {
        this.val = value;
        return this;
    }
}

// @ts-expect-error - Return type 'Chainable' is not assignable to 'this'
const badChainable: Chainable = {
    setValue(value: number): Chainable {
        return {} as Chainable; // Should return 'this', not 'Chainable'
    }
};

// =================================================================
// SECTION 28: REST PARAMETER TYPE COMPATIBILITY
// Rest parameters have array-like constraints
// =================================================================

type VarArgs = (...args: number[]) => void;
type FixedArgs = (a: number, b: number) => void;

const varArgs: VarArgs = (...nums) => console.log(nums);
const fixedArgs: FixedArgs = (a, b) => console.log(a + b);

// Fixed args assignable to var args (subset of calls)
const varFromFixed: VarArgs = fixedArgs; // OK

// @ts-expect-error - VarArgs takes any count, FixedArgs needs exactly 2
// This may or may not error depending on variance rules

// Rest with different element types
type StringRest = (...args: string[]) => void;
type NumberRest = (...args: number[]) => void;

// @ts-expect-error - Rest parameter types don't match
const numFromStr: NumberRest = ((...args: string[]) => {}) as StringRest;

// =================================================================
// SECTION 29: TYPE ASSERTION LIMITATIONS
// Type assertions have some constraints
// =================================================================

// Completely unrelated types still error with assertions
// @ts-expect-error - Conversion of type 'string' to type 'number' may be a mistake
const forcedConversion: number = "hello" as number;

// Need double assertion for unrelated types
const doubleAssert: number = "hello" as unknown as number; // Works (but unsafe)

// =================================================================
// SECTION 30: RECURSIVE TYPE ASSIGNABILITY
// Recursive types must terminate correctly
// =================================================================

interface TreeNode<T> {
    value: T;
    children: TreeNode<T>[];
}

interface StringTree {
    value: string;
    children: StringTree[];
}

interface NumberTree {
    value: number;
    children: NumberTree[];
}

const strTree: StringTree = { value: "root", children: [] };
const numTree: NumberTree = { value: 1, children: [] };

// @ts-expect-error - Recursive structure types don't match
const strFromNum: StringTree = numTree;

// Generic recursive type
const genericTree: TreeNode<string> = strTree; // OK - same structure

// @ts-expect-error - TreeNode<string> not assignable to TreeNode<number>
const wrongGeneric: TreeNode<number> = genericTree;

// =================================================================
// SECTION 31: EXCESS PROPERTY CHECKS
// Fresh object literals are checked for excess properties
// =================================================================

interface Named {
    name: string;
}

// @ts-expect-error - Excess property 'age' in object literal
const excessProp: Named = { name: "test", age: 25 };

// Valid when assigned through variable (not fresh)
const obj = { name: "test", age: 25 };
const namedFromVar: Named = obj; // OK - obj is not fresh

// =================================================================
// SECTION 32: INFER TYPE IN CONDITIONALS
// Inferred types must satisfy constraints
// =================================================================

type Unpacked<T> = T extends (infer U)[] ? U : T;
type GetReturnType<T> = T extends (...args: any[]) => infer R ? R : never;

// These are type-level operations, verify instantiation
type UnpackedStrArr = Unpacked<string[]>; // string
type ReturnOfStrFn = GetReturnType<() => string>; // string

// @ts-expect-error - Type 'string' is not assignable to type 'number'
const wrongUnpacked: Unpacked<number[]> = "hello";

// Valid
const correctUnpacked: Unpacked<number[]> = 42;

// =================================================================
// SECTION 33: KEYOF TYPE CONSTRAINTS
// keyof creates union of property names
// =================================================================

interface Person {
    name: string;
    age: number;
    email: string;
}

type PersonKey = keyof Person; // "name" | "age" | "email"

// @ts-expect-error - "address" is not a key of Person
const invalidKey: PersonKey = "address";

// Valid keys
const validKey1: PersonKey = "name";
const validKey2: PersonKey = "age";
const validKey3: PersonKey = "email";

// =================================================================
// SECTION 34: TYPE PREDICATE CONSTRAINTS
// Type predicates must narrow correctly
// =================================================================

interface Cat {
    meow(): void;
}

interface Dog {
    bark(): void;
}

function isCat(animal: Cat | Dog): animal is Cat {
    return (animal as Cat).meow !== undefined;
}

// @ts-expect-error - Type predicate function signature mismatch
const badPredicate: (x: Cat | Dog) => x is Dog = isCat;

// =================================================================
// SECTION 35: ARRAY METHODS RETURN TYPE INFERENCE
// Array method types must be compatible
// =================================================================

const numbers = [1, 2, 3];
const strings = ["a", "b", "c"];

// @ts-expect-error - map returns string[], not number[]
const numFromStrMap: number[] = strings.map(s => s.toUpperCase());

// @ts-expect-error - filter type mismatch
const strFromNumFilter: string[] = numbers.filter(n => n > 1);

// Valid
const upperStrings: string[] = strings.map(s => s.toUpperCase());
const filteredNums: number[] = numbers.filter(n => n > 1);

console.log("Subtyping conformance tests complete");
