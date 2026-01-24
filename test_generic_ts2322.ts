// Generic-related TS2322 Error Tests
// Focus on generic type parameters, constraints, and function calls

// =============================================================================
// 1. Basic Generic Type Parameters - Should emit TS2322
// =============================================================================

// Identity function type mismatches
function identity<T>(x: T): T {
    return x;
}

const shouldFail1: number = identity("string"); // TS2322 - string to number
const shouldFail2: string = identity(42); // TS2322 - number to string
const shouldFail3: { a: number } = identity({ a: "string" }); // TS2322 - wrong property type

// Array generic type mismatches
const arr1: number[] = identity(["a", "b"]); // TS2322 - string[] to number[]
const arr2: string[] = identity([1, 2, 3]); // TS2322 - number[] to string[]

// =============================================================================
// 2. Generic Type Constraints - Should emit TS2322
// =============================================================================

// Constraint: T extends number
function addOne<T extends number>(x: T): T {
    return x + 1;
}

const shouldFail4: number = addOne("string"); // TS2322 - string doesn't extend number
const shouldFail5: number = addOne(true); // TS2322 - boolean doesn't extend number

// Constraint: T extends string
function append<T extends string>(x: T, suffix: string): T {
    return x + suffix;
}

const shouldFail6: string = append(123, "abc"); // TS2322 - number doesn't extend string
const shouldFail7: string = append(true, "abc"); // TS2322 - boolean doesn't extend string

// Constraint: T extends { id: number }
function extractId<T extends { id: number }>(obj: T): number {
    return obj.id;
}

const shouldFail8: number = extractId({ name: "test" }); // TS2322 - missing id property
const shouldFail9: number = extractId({ id: "string" }); // TS2322 - id is string, not number

// =============================================================================
// 3. Multiple Generic Type Parameters - Should emit TS2322
// =============================================================================

// Function with multiple generic parameters
function pair<T, U>(first: T, second: U): [T, U] {
    return [first, second];
}

const shouldFail10: [string, number] = pair(42, "hello"); // TS2322 - reversed types
const shouldFail11: [number, string] = pair("hello", 42); // TS2322 - should be [string, number]

// Function with constrained generics
function merge<T extends object, U extends object>(a: T, b: U): T & U {
    return { ...a, ...b };
}

interface Person { name: string; }
interface Address { city: string; }

const shouldFail12: Person = merge({ name: "John" }, { city: "NYC" }); // TS2322 - missing city property
const shouldFail13: Address = merge({ name: "John" }, { city: "NYC" }); // TS2322 - missing name property

// =============================================================================
// 4. Generic Class Types - Should emit TS2322
// =============================================================================

class Container<T> {
    private value: T;

    constructor(value: T) {
        this.value = value;
    }

    getValue(): T {
        return this.value;
    }
}

const container1: Container<number> = new Container("string"); // TS2322 - Container<string> to Container<number>
const container2: Container<string> = new Container(42); // TS2322 - Container<number> to Container<string>

// =============================================================================
// 5. Generic Interface Types - Should emit TS2322
// =============================================================================

interface Repository<T> {
    find(id: string): T | null;
    save(item: T): void;
}

interface User { id: string; name: string; }
interface Product { id: string; price: number; }

const userRepo: Repository<User> = {
    find: (id) => ({ id, name: "John" }),
    save: (user) => console.log(user)
};

const productRepo: Repository<Product> = {
    find: (id) => ({ id, price: 99.99 }),
    save: (product) => console.log(product)
};

const shouldFail14: User = userRepo.find("1") as unknown as Product; // TS2322 - Product to User
const shouldFail15: Product = productRepo.find("1") as unknown as User; // TS2322 - User to Product

// =============================================================================
// 6. Generic Utility Types - Should emit TS2322
// =============================================================================

interface ComplexUser {
    id: string;
    name: string;
    age: number;
    active: boolean;
}

// Partial type mismatches
const partialUpdate: Partial<ComplexUser> = { name: 123 }; // TS2322 - number instead of string
const shouldFail16: Partial<ComplexUser> = { age: "twenty" }; // TS2322 - string instead of number

// Pick type mismatches
const userName: Pick<ComplexUser, "name"> = { name: 123 }; // TS2322 - number instead of string
const shouldFail17: Pick<ComplexUser, "age"> = { age: "twenty" }; // TS2322 - string instead of number

// Readonly type mismatches
const readonlyUser: Readonly<ComplexUser> = { id: "1", name: "John", age: 30, active: true };
readonlyUser.age = "thirty"; // TS2322 - string instead of number

// =============================================================================
// 7. Conditional Types - Should emit TS2322
// =============================================================================

type ExtractType<T> = T extends string ? string : never;
type ExcludeType<T> = T extends string ? never : T;

const shouldFail18: ExtractType<number> = 42; // TS2322 - never type cannot be assigned
const shouldFail19: ExcludeType<string> = "hello"; // TS2322 - never type cannot be assigned

// =============================================================================
// 8. Mapped Types - Should emit TS2322
// =============================================================================

type StringifyProps<T> = {
    [K in keyof T]: string;
};

interface NumericProps {
    a: number;
    b: number;
}

const shouldFail20: StringifyProps<NumericProps> = { a: 1, b: "two" }; // TS2322 - number instead of string

// =============================================================================
// 9. Generic Function Return Types - Should emit TS2322
// =============================================================================

function createArray<T>(length: number, value: T): T[] {
    return Array(length).fill(value);
}

const shouldFail21: string[] = createArray(3, 1); // TS2322 - number instead of string
const shouldFail22: number[] = createArray(3, "one"); // TS2322 - string instead of number

// =============================================================================
// 10. Generic Constraints with Type Parameters - Should emit TS2322
// =============================================================================

function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

const user = { name: "John", age: 30 };
const shouldFail23: string = getProperty(user, "age"); // TS2322 - number instead of string
const shouldFail24: number = getProperty(user, "name"); // TS2322 - string instead of number

// =============================================================================
// 11. Generic Defaults - Should emit TS2322
// =============================================================================

interface ConfigurableOptions<T = string> {
    value: T;
}

const shouldFail25: ConfigurableOptions<number> = { value: "string" }; // TS2322 - string instead of number
const shouldFail26: ConfigurableOptions<string> = { value: 123 }; // TS2322 - number instead of string

// =============================================================================
// 12. Variance and Generic Parameters - Should emit TS2322
// =============================================================================

// Covariant array assignment (should fail)
interface Animal { name: string; }
interface Dog extends Animal { breed: string; }

const animalArray: Animal[] = [new Dog()]; // This is valid
const dogArray: Dog[] = [new Animal()]; // TS2322 - Animal to Dog

// Contravariant function parameters (should fail)
function takesAnimal(animals: Animal[]): void {}
function takesDog(dogs: Dog[]): void {}

const animalParam: Animal[] = [new Animal()];
const dogParam: Dog[] = [new Dog()];

takesDog(animalParam); // TS2322 - Animal[] to Dog[]
takesAnimal(dogParam); // This is valid

// =============================================================================
// 13. Recursive Generics - Should emit TS2322
// =============================================================================

type JsonPrimitive = string | number | boolean | null;
type JsonArray = JsonValue[];
type JsonObject = { [key: string]: JsonValue };
type JsonValue = JsonPrimitive | JsonArray | JsonObject;

const shouldFail27: JsonValue = { data: undefined }; // TS2322 - undefined not in JsonValue
const shouldFail28: JsonArray = [undefined]; // TS2322 - undefined not in JsonValue

// =============================================================================
// 14. Generic Function in Higher-Order Contexts - Should emit TS2322
// =============================================================================

function createMapper<T, U>(transform: (x: T) => U): (items: T[]) => U[] {
    return (items) => items.map(transform);
}

const stringToNumber = (s: string) => s.length;
const numberToString = (n: number) => n.toString();

const mapper1 = createMapper(stringToNumber);
const mapper2 = createMapper(numberToString);

const shouldFail29: number[] = mapper1(["one", "two"]); // Should be valid, but testing structure
const shouldFail30: string[] = mapper2([1, 2]); // Should be valid, but testing structure

// =============================================================================
// 15. Generic Type Guards - Should emit TS2322
// =============================================================================

function isString(value: unknown): value is string {
    return typeof value === "string";
}

function isNumber(value: unknown): value is number {
    return typeof value === "number";
}

const unknownValue: unknown = "hello";

if (isString(unknownValue)) {
    const shouldFail31: number = unknownValue; // TS2322 - string to number in narrowed context
}

if (isNumber(unknownValue)) {
    const shouldFail32: string = unknownValue; // TS2322 - number to string in narrowed context
}