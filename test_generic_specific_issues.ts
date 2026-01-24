// Test specific issues found in generic TS2322 handling

// Issue 1: Array element type inference in generic functions
function doubleNumbers<T extends number>(arr: T[]): T[] {
    return arr.map(x => x * 2); // Should NOT emit TS2322
}

// Valid usage
const validDoubles: number[] = doubleNumbers([1, 2, 3]);

// Issue 2: Generic type parameter inference with constraints
function createWrapper<T extends number>(value: T): { value: T } {
    return { value }; // Should NOT emit TS2322
}

// Valid inference
const wrappedNumber = createWrapper(42); // Should infer { value: number }

// Issue 3: Conditional type narrowing
type IsStringType<T> = T extends string ? string : never;

function narrowToString<T>(value: T): IsStringType<T> {
    if (typeof value === "string") {
        return value; // Should NOT emit TS2322 in narrowed context
    }
    throw new Error("Not a string");
}

// Valid narrowing
const narrowedString: string = narrowToString("hello"); // Should be valid

// Issue 4: Generic mapped type property access
interface NumericData {
    a: number;
    b: number;
}

type StringifiedData = {
    [K in keyof NumericData]: string;
};

function stringifyObject(obj: NumericData): StringifiedData {
    return {
        a: obj.a.toString(), // Should NOT emit TS2322
        b: obj.b.toString()  // Should NOT emit TS2322
    };
}

// Valid stringification
const stringified: StringifiedData = stringifyObject({ a: 1, b: 2 });

// Issue 5: Generic promise chain with valid types
async function fetchNumber(): Promise<number> {
    return 42;
}

async function process<T extends number>(value: T): Promise<T> {
    return value * 2;
}

// Valid promise chain
const processedPromise: Promise<number> = fetchNumber().then(process); // Should NOT emit TS2322