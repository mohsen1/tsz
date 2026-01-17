// =================================================================
// OPTIONAL CHAINING TYPE CHECKING TESTS (SOLV-38)
// Tests for obj?.prop, obj?.method(), and obj?.[key] type checking
// =================================================================

// =================================================================
// SECTION 1: BASIC OPTIONAL PROPERTY ACCESS (obj?.prop)
// =================================================================

interface User {
    name: string;
    age: number;
    address?: {
        city: string;
        zipCode: number;
    };
}

declare const user: User | undefined;
declare const maybeNull: User | null;

// Test 1.1: Optional chaining returns T | undefined
const userName: string | undefined = user?.name; // Valid

// @ts-expect-error - Type 'string | undefined' is not assignable to type 'string'
const userNameStrict: string = user?.name;

// Test 1.2: Nested optional chaining
const city: string | undefined = user?.address?.city; // Valid

// @ts-expect-error - Type 'string | undefined' is not assignable to type 'string'
const cityStrict: string = user?.address?.city;

// Test 1.3: Optional chaining on null returns undefined type
const nullName: string | undefined = maybeNull?.name; // Valid

// Test 1.4: Chain of optional properties
const zipCode: number | undefined = user?.address?.zipCode; // Valid

// =================================================================
// SECTION 2: OPTIONAL METHOD CALLS (obj?.method())
// =================================================================

interface Calculator {
    add(a: number, b: number): number;
    multiply?(a: number, b: number): number;
}

declare const calc: Calculator | undefined;
declare const nullCalc: Calculator | null;

// Test 2.1: Optional method call returns T | undefined
const sum: number | undefined = calc?.add(1, 2); // Valid

// @ts-expect-error - Type 'number | undefined' is not assignable to type 'number'
const sumStrict: number = calc?.add(1, 2);

// Test 2.2: Optional property method call
const product: number | undefined = calc?.multiply?.(2, 3); // Valid

// Test 2.3: Method call on null object
const nullSum: number | undefined = nullCalc?.add(1, 2); // Valid

// Test 2.4: Chained method calls
interface Builder {
    value: number;
    increment(): Builder;
    getValue(): number;
}

declare const builder: Builder | undefined;
const finalValue: number | undefined = builder?.increment().getValue(); // Valid

// =================================================================
// SECTION 3: OPTIONAL ELEMENT ACCESS (obj?.[key])
// =================================================================

interface Dictionary {
    [key: string]: string;
}

declare const dict: Dictionary | undefined;
declare const arr: number[] | undefined;

// Test 3.1: Optional element access returns T | undefined
const value1: string | undefined = dict?.["hello"]; // Valid

// @ts-expect-error - Type 'string | undefined' is not assignable to type 'string'
const valueStrict: string = dict?.["hello"];

// Test 3.2: Optional element access with numeric index
const element: number | undefined = arr?.[0]; // Valid

// @ts-expect-error - Type 'number | undefined' is not assignable to type 'number'
const elementStrict: number = arr?.[0];

// Test 3.3: Optional element access with computed key
const key = "hello";
const computedValue: string | undefined = dict?.[key]; // Valid

// Test 3.4: Nested optional element access
interface NestedDict {
    [key: string]: { [innerKey: string]: number };
}

declare const nestedDict: NestedDict | undefined;
const nestedValue: number | undefined = nestedDict?.["outer"]?.["inner"]; // Valid

// =================================================================
// SECTION 4: COMBINED OPTIONAL CHAINING PATTERNS
// =================================================================

interface ComplexObject {
    data?: {
        items: Array<{ name: string; value?: number }>;
        getItem?(index: number): { name: string; value?: number };
    };
}

declare const complex: ComplexObject | undefined;

// Test 4.1: Property + element access
const itemName: string | undefined = complex?.data?.items[0]?.name; // Valid

// Test 4.2: Property + method + property access
const methodItem: string | undefined = complex?.data?.getItem?.(0)?.name; // Valid

// Test 4.3: Element access + property access
const firstItem: number | undefined = complex?.data?.items?.[0]?.value; // Valid

// =================================================================
// SECTION 5: OPTIONAL CHAINING WITH NULLISH COALESCING
// =================================================================

// Test 5.1: Optional chaining with ?? fallback
const nameWithDefault: string = user?.name ?? "Anonymous"; // Valid - string type

// Test 5.2: Nested optional chaining with ?? fallback
const cityWithDefault: string = user?.address?.city ?? "Unknown"; // Valid

// Test 5.3: Method call with ?? fallback
const sumWithDefault: number = calc?.add(1, 2) ?? 0; // Valid

// Test 5.4: Element access with ?? fallback
const elementWithDefault: number = arr?.[0] ?? -1; // Valid

// =================================================================
// SECTION 6: OPTIONAL CHAINING TYPE NARROWING
// =================================================================

interface Shape {
    kind: "circle" | "square";
    radius?: number;
    side?: number;
}

function getArea(shape: Shape | undefined): number {
    // Test 6.1: Narrowing with optional chaining comparison
    if (shape?.kind === "circle") {
        // shape is narrowed to Shape (non-null)
        return Math.PI * (shape.radius ?? 0) ** 2;
    }
    if (shape?.kind === "square") {
        return (shape.side ?? 0) ** 2;
    }
    return 0;
}

// Test 6.2: Truthy narrowing with optional chaining
function processUser(user: User | undefined): string {
    if (user?.address) {
        // user and user.address are both defined here
        return user.address.city;
    }
    return "No address";
}

// =================================================================
// SECTION 7: OPTIONAL CHAINING WITH GENERICS
// =================================================================

interface Container<T> {
    value?: T;
    getValue?(): T;
}

function getContainerValue<T>(container: Container<T> | undefined): T | undefined {
    // Test 7.1: Generic optional property access
    return container?.value;
}

function callContainerMethod<T>(container: Container<T> | undefined): T | undefined {
    // Test 7.2: Generic optional method call
    return container?.getValue?.();
}

declare const stringContainer: Container<string>;
const stringValue: string | undefined = getContainerValue(stringContainer); // Valid

declare const numberContainer: Container<number>;
const numberValue: number | undefined = callContainerMethod(numberContainer); // Valid

// =================================================================
// SECTION 8: OPTIONAL CHAINING ERROR CASES
// =================================================================

interface StrictUser {
    name: string;
    email: string;
}

declare const strictUser: StrictUser | undefined;

// Test 8.1: Property doesn't exist on type
// @ts-expect-error - Property 'phone' does not exist on type 'StrictUser'
const phone: string | undefined = strictUser?.phone;

// Test 8.2: Wrong property type expectation
// @ts-expect-error - Type 'string | undefined' is not assignable to type 'number'
const wrongType: number = strictUser?.name;

// =================================================================
// SECTION 9: OPTIONAL CHAINING WITH CLASSES
// =================================================================

class Person {
    constructor(public name: string, public age: number) {}

    greet(): string {
        return `Hello, I'm ${this.name}`;
    }

    static create(name: string, age: number): Person {
        return new Person(name, age);
    }
}

declare const person: Person | undefined;

// Test 9.1: Optional method call on class instance
const greeting: string | undefined = person?.greet(); // Valid

// Test 9.2: Optional property access on class instance
const personName: string | undefined = person?.name; // Valid

// Test 9.3: Optional chaining doesn't apply to static methods
const newPerson: Person = Person.create("Dave", 30); // Valid, not optional

// =================================================================
// SECTION 10: OPTIONAL CHAINING WITH FUNCTION TYPES
// =================================================================

type Callback = (value: string) => number;

declare const maybeCallback: Callback | undefined;

// Test 10.1: Optional function call
const callbackResult: number | undefined = maybeCallback?.("hello"); // Valid

// @ts-expect-error - Type 'number | undefined' is not assignable to type 'number'
const callbackStrict: number = maybeCallback?.("hello");

// Test 10.2: Optional function with arguments type check
// @ts-expect-error - Argument of type 'number' is not assignable to parameter of type 'string'
const wrongArg: number | undefined = maybeCallback?.(42);

// =================================================================
// SECTION 11: OPTIONAL CHAINING WITH READONLY PROPERTIES
// =================================================================

interface ReadonlyConfig {
    readonly settings: {
        readonly theme: string;
        readonly fontSize: number;
    };
}

declare const config: ReadonlyConfig | undefined;

// Test 11.1: Optional access to readonly properties
const theme: string | undefined = config?.settings?.theme; // Valid

// =================================================================
// SECTION 12: OPTIONAL CHAINING WITHOUT ERRORS (VALID CASES)
// =================================================================

// These should all compile without errors
declare const obj1: { a: { b: { c: string } } } | undefined;
const validChain1: string | undefined = obj1?.a.b.c;

declare const obj2: { fn?: () => number } | undefined;
const validChain2: number | undefined = obj2?.fn?.();

declare const arr2: string[][] | undefined;
const validChain3: string | undefined = arr2?.[0]?.[0];

declare const mixed: { items?: Array<{ name: string }> } | undefined;
const validChain4: string | undefined = mixed?.items?.[0]?.name;

console.log("Optional chaining tests complete");
