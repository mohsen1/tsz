// =================================================================
// TS2322 EDGE CASE TESTS
// Additional edge cases for type assignability errors
// covering patterns from differential testing
// =================================================================

// @strict: true
// @noEmit: true

// =================================================================
// SECTION 1: COMPLEX GENERIC VARIANCE
// =================================================================

interface Covariant<out T> {
    get value(): T;
}

interface Contravariant<in T> {
    set value(v: T);
}

interface Invariant<T> {
    value: T;
}

// Covariant generic - output position
declare const covariantStr: Covariant<string>;
declare const covariantUnknown: Covariant<unknown>;

const covariantValid: Covariant<unknown> = covariantStr; // OK - string -> unknown

// @ts-expect-error - Covariant doesn't allow narrowing
const covariantInvalid: Covariant<string> = covariantUnknown;

// Contravariant generic - input position
declare const contravariantStr: Contravariant<string>;
declare const contravariantUnknown: Contravariant<unknown>;

const contravariantValid: Contravariant<string> = contravariantUnknown; // OK - accept broader

// @ts-expect-error - Contravariant doesn't allow widening target
const contravariantInvalid: Contravariant<unknown> = contravariantStr;

// =================================================================
// SECTION 2: OBJECT LITERAL FRESHNESS
// =================================================================

interface StrictShape {
    x: number;
    y: number;
}

// Fresh object literal - excess property check
// @ts-expect-error - Object literal may only specify known properties
const freshExcess: StrictShape = { x: 1, y: 2, z: 3 };

// Non-fresh (widened) - no excess property check
const wide = { x: 1, y: 2, z: 3 };
const nonFreshOk: StrictShape = wide; // OK

// Spread creates fresh object
// @ts-expect-error - Spread creates fresh object, excess check applies
const spreadExcess: StrictShape = { ...wide };

// =================================================================
// SECTION 3: CALLABLE INTERFACE OVERLOADS
// =================================================================

interface Overloaded {
    (x: string): number;
    (x: number): string;
    (x: boolean): boolean;
}

// @ts-expect-error - Missing overloads
const singleOverload: Overloaded = (x: string) => x.length;

// @ts-expect-error - Wrong return type for overload
const wrongOverload: Overloaded = (x: string | number | boolean) => {
    return "always string";
};

// =================================================================
// SECTION 4: HYBRID INTERFACE (CALLABLE + PROPERTIES)
// =================================================================

interface HybridInterface {
    (input: string): string;
    count: number;
    reset(): void;
}

// @ts-expect-error - Missing 'count' property
const missingCount: HybridInterface = Object.assign(
    (input: string) => input,
    { reset: () => {} }
);

// @ts-expect-error - Missing 'reset' method
const missingReset: HybridInterface = Object.assign(
    (input: string) => input,
    { count: 0 }
);

// =================================================================
// SECTION 5: INDEXED ACCESS TYPES
// =================================================================

interface Lookup {
    name: string;
    age: number;
    active: boolean;
}

type NameType = Lookup["name"]; // string
type AgeType = Lookup["age"]; // number

// @ts-expect-error - Type 'number' not assignable to Lookup["name"]
const wrongIndexed: NameType = 42;

// @ts-expect-error - Type 'string' not assignable to Lookup["age"]
const wrongIndexed2: AgeType = "twenty";

// Dynamic indexed access
declare function getProperty<T, K extends keyof T>(obj: T, key: K): T[K];

// @ts-expect-error - "missing" is not a key of Lookup
getProperty({ name: "test", age: 25, active: true }, "missing");

// =================================================================
// SECTION 6: SPREAD TYPE INFERENCE
// =================================================================

interface Base {
    id: number;
    name: string;
}

interface Extended extends Base {
    extra: boolean;
}

const base: Base = { id: 1, name: "test" };
const extended: Extended = { id: 2, name: "ext", extra: true };

// Spread loses type narrowing
// @ts-expect-error - Spread of Base doesn't satisfy Extended
const spreadBase: Extended = { ...base, extra: true }; // Actually OK in tsc

// But wrong types still error
// @ts-expect-error - Wrong type for 'id'
const wrongSpread: Base = { ...base, id: "not-a-number" };

// =================================================================
// SECTION 7: DISTRIBUTIVE CONDITIONAL TYPES
// =================================================================

type Distributed<T> = T extends string ? "string" : "other";

// When T is union, distributes over each member
type Test1 = Distributed<string | number>; // "string" | "other"

// @ts-expect-error - "string" not assignable to "other"
const wrongDistributed: Distributed<number> = "string";

// Non-distributive with tuple
type NonDistributed<T> = [T] extends [string] ? "string" : "other";

// @ts-expect-error - Different result when non-distributed
const nonDistributedWrong: NonDistributed<string> = "other";

// =================================================================
// SECTION 8: INFERENCE IN CONDITIONAL TYPES
// =================================================================

type InferReturn<T> = T extends (...args: any[]) => infer R ? R : never;
type InferArg<T> = T extends (arg: infer A) => any ? A : never;

type GetReturnString = InferReturn<() => string>; // string
type GetArgNumber = InferArg<(x: number) => void>; // number

// @ts-expect-error - number not assignable to inferred string
const wrongInferred: GetReturnString = 42;

// @ts-expect-error - string not assignable to inferred number
const wrongInferredArg: GetArgNumber = "hello";

// =================================================================
// SECTION 9: RECURSIVE TYPE DEPTH
// =================================================================

interface DeepNested {
    level1: {
        level2: {
            level3: {
                value: string;
            };
        };
    };
}

// @ts-expect-error - Wrong type at deepest level
const wrongDeep: DeepNested = {
    level1: {
        level2: {
            level3: {
                value: 42
            }
        }
    }
};

// @ts-expect-error - Missing intermediate level
const missingLevel: DeepNested = {
    level1: {
        level3: {
            value: "test"
        }
    }
};

// =================================================================
// SECTION 10: ARRAY METHOD GENERIC INFERENCE
// =================================================================

const numbers = [1, 2, 3];
const strings = ["a", "b", "c"];

// reduce with wrong accumulator type
// @ts-expect-error - Accumulator type doesn't match
const wrongReduce: string = numbers.reduce((acc: number, n) => acc + n, 0);

// map with wrong return type assertion
// @ts-expect-error - Map callback returns string, not number
const wrongMap: number[] = strings.map(s => s.toUpperCase());

// filter type predicate mismatch
// @ts-expect-error - Filter returns same array type
const wrongFilter: string[] = numbers.filter(n => n > 1);

// =================================================================
// SECTION 11: PROMISE CHAIN TYPES
// =================================================================

declare function fetchString(): Promise<string>;
declare function fetchNumber(): Promise<number>;

// @ts-expect-error - Promise<string>.then returns Promise, not string
const wrongPromiseChain: string = fetchString().then(s => s.length);

// @ts-expect-error - then callback type mismatch
const wrongThenCallback: Promise<number> = fetchString().then((s: number) => s);

// Nested promise unwrapping
async function nestedPromise(): Promise<number> {
    // @ts-expect-error - Promise<string> returned, expected number
    return fetchString();
}

// =================================================================
// SECTION 12: CLASS STATIC MEMBERS
// =================================================================

class WithStatic {
    static count: number = 0;
    static getName(): string {
        return "name";
    }

    instance: string = "value";
}

// @ts-expect-error - typeof WithStatic vs WithStatic instance
const wrongStatic: WithStatic = WithStatic;

// @ts-expect-error - Instance type vs constructor type
const wrongConstructor: typeof WithStatic = new WithStatic();

// =================================================================
// SECTION 13: ABSTRACT CLASS CONSTRAINTS
// =================================================================

abstract class AbstractBase {
    abstract getValue(): string;
    concrete(): number {
        return 42;
    }
}

class ConcreteImpl extends AbstractBase {
    getValue(): string {
        return "value";
    }
}

// @ts-expect-error - Cannot instantiate abstract class
const abstractInstance: AbstractBase = new AbstractBase();

// Valid through subclass
const concreteInstance: AbstractBase = new ConcreteImpl(); // OK

// =================================================================
// SECTION 14: INTERFACE MERGING
// =================================================================

interface Mergeable {
    first: string;
}

interface Mergeable {
    second: number;
}

// Merged interface requires both
// @ts-expect-error - Missing 'second' property
const partialMerged: Mergeable = { first: "test" };

// @ts-expect-error - Missing 'first' property
const partialMerged2: Mergeable = { second: 42 };

// Valid with both
const fullMerged: Mergeable = { first: "test", second: 42 };

// =================================================================
// SECTION 15: NAMESPACE VALUE VS TYPE
// =================================================================

namespace MyNamespace {
    export interface MyType {
        value: string;
    }
    export const myValue = { value: "test" };
}

// @ts-expect-error - Namespace as type vs value
const namespaceAsValue: typeof MyNamespace = { MyType: {}, myValue: {} };

// Valid namespace type usage
const nsType: MyNamespace.MyType = { value: "test" };

// =================================================================
// SECTION 16: TYPE GUARD NARROWING LIMITS
// =================================================================

type Fish = { swim: () => void };
type Bird = { fly: () => void };

function isFish(pet: Fish | Bird): pet is Fish {
    return (pet as Fish).swim !== undefined;
}

declare const pet: Fish | Bird;

// Without narrowing
// @ts-expect-error - Union type doesn't have 'swim' directly
pet.swim();

// After narrowing - OK
if (isFish(pet)) {
    pet.swim(); // OK
}

// =================================================================
// SECTION 17: CONST ASSERTION EFFECTS
// =================================================================

const asConst = { x: 1, y: 2 } as const;
const asRegular = { x: 1, y: 2 };

// @ts-expect-error - readonly properties with literal types
const mutableFromConst: { x: number; y: number } = asConst;

// @ts-expect-error - Literal type narrowing
const literal1: 1 = asRegular.x;

// Valid with const
const literal1Const: 1 = asConst.x; // OK - literal type preserved

// =================================================================
// SECTION 18: BIVARIANT METHOD PARAMETERS
// =================================================================

interface MethodHost {
    process(input: string): void;
}

interface CovariantHost {
    process(input: string | number): void;
}

declare const methodHost: MethodHost;
declare const covariantHost: CovariantHost;

// Function parameters are contravariant, but method parameters are bivariant
// This is a known TypeScript design choice for compatibility

// =================================================================
// SECTION 19: TYPE PARAMETER DEFAULTS
// =================================================================

interface WithDefault<T = string> {
    value: T;
}

// Default T = string
// @ts-expect-error - number not assignable to default string
const wrongDefault: WithDefault = { value: 42 };

// Explicit T = number is fine
const explicitNumber: WithDefault<number> = { value: 42 };

// =================================================================
// SECTION 20: CONTEXTUAL TYPING LIMITS
// =================================================================

type Callback = (x: number, y: string) => boolean;

// Contextual parameter typing
// @ts-expect-error - Return type must be boolean
const wrongReturn: Callback = (x, y) => x + y.length;

// @ts-expect-error - Wrong parameter usage implies wrong types
const wrongUsage: Callback = (x, y) => {
    const z: string = x; // Error - x is number
    return true;
};

// =================================================================
// SECTION 21: ENUM MEMBER ASSIGNABILITY
// =================================================================

enum NumericEnum {
    A = 0,
    B = 1,
    C = 2
}

enum StringEnum {
    X = "X",
    Y = "Y",
    Z = "Z"
}

// Numeric enums are somewhat interchangeable with numbers
const numVal: number = NumericEnum.A; // OK

// @ts-expect-error - Number not directly assignable to enum
const enumVal: NumericEnum = 0;

// @ts-expect-error - String not assignable to string enum
const strEnumVal: StringEnum = "X";

// =================================================================
// SECTION 22: THIS POLYMORPHISM
// =================================================================

class Builder {
    private value: string = "";

    append(s: string): this {
        this.value += s;
        return this;
    }

    build(): string {
        return this.value;
    }
}

class ExtendedBuilder extends Builder {
    private count: number = 0;

    appendCounted(s: string): this {
        this.count++;
        return this.append(s);
    }
}

// @ts-expect-error - 'this' type mismatch in return
const wrongBuilder: Builder = {
    value: "",
    append(s: string): Builder {
        return this; // Should return 'this', not 'Builder'
    },
    build() { return ""; }
};

// =================================================================
// SECTION 23: TUPLE REST ELEMENTS
// =================================================================

type TupleWithRest = [string, ...number[]];
type TupleWithMiddleRest = [string, ...number[], boolean];

// @ts-expect-error - First element must be string
const wrongFirst: TupleWithRest = [1, 2, 3];

// @ts-expect-error - Rest elements must be numbers
const wrongRest: TupleWithRest = ["hello", "a", "b"];

// @ts-expect-error - Last element must be boolean
const wrongLast: TupleWithMiddleRest = ["hello", 1, 2, 3];

// Valid
const validTuple: TupleWithRest = ["hello", 1, 2, 3];
const validMiddle: TupleWithMiddleRest = ["hello", 1, 2, true];

// =================================================================
// SECTION 24: OBJECT INTERSECTION PROPERTY CONFLICTS
// =================================================================

type Conflict1 = { prop: string };
type Conflict2 = { prop: number };
type Conflicted = Conflict1 & Conflict2; // prop: string & number = never

// @ts-expect-error - Property 'prop' is never (string & number)
const conflicted: Conflicted = { prop: "test" };

// @ts-expect-error - Also never
const conflicted2: Conflicted = { prop: 42 };

// =================================================================
// SECTION 25: VARIADIC TUPLE TYPES
// =================================================================

type Concat<T extends unknown[], U extends unknown[]> = [...T, ...U];

type ConcatResult = Concat<[string, number], [boolean]>; // [string, number, boolean]

// @ts-expect-error - Wrong types in concatenated tuple
const wrongConcat: ConcatResult = [1, "two", "three"];

// Valid
const validConcat: ConcatResult = ["one", 2, true];

// =================================================================
// SECTION 26: SYMBOL INDEX SIGNATURES
// =================================================================

interface SymbolIndexed {
    [key: symbol]: string;
}

const symKey = Symbol("key");

const symbolIndexed: SymbolIndexed = {
    [symKey]: "value"
};

// @ts-expect-error - Value must be string
const wrongSymbolValue: SymbolIndexed = {
    [symKey]: 42
};

// =================================================================
// SECTION 27: OBJECT SPREAD OVERRIDE
// =================================================================

interface Original {
    a: string;
    b: number;
}

interface Override {
    a: number; // Different type
    c: boolean;
}

// Spread merges types, later wins
type Merged = Original & Override; // a is string & number = never

// @ts-expect-error - 'a' type conflicts
const merged: Merged = { a: "test", b: 1, c: true };

// =================================================================
// SECTION 28: NESTED GENERICS
// =================================================================

interface Container<T> {
    value: T;
}

interface NestedContainer<T> {
    inner: Container<T>;
}

// @ts-expect-error - Nested generic type mismatch
const wrongNested: NestedContainer<string> = {
    inner: { value: 42 }
};

// @ts-expect-error - Wrong outer type
const wrongOuter: NestedContainer<number> = {
    inner: { value: "string" }
};

// Valid
const validNested: NestedContainer<number> = {
    inner: { value: 42 }
};

// =================================================================
// SECTION 29: PARTIAL AND REQUIRED UTILITIES
// =================================================================

interface Config {
    host: string;
    port: number;
    secure: boolean;
}

// Partial makes all optional
type PartialConfig = Partial<Config>;

// Required makes all required
type RequiredPartial = Required<PartialConfig>;

// @ts-expect-error - Missing properties
const incompleteRequired: RequiredPartial = { host: "localhost" };

// Partial allows incomplete
const validPartial: PartialConfig = { host: "localhost" }; // OK

// =================================================================
// SECTION 30: PICK AND OMIT UTILITIES
// =================================================================

type HostOnly = Pick<Config, "host">;
type NoPort = Omit<Config, "port">;

// @ts-expect-error - Pick only has 'host'
const wrongPick: HostOnly = { host: "localhost", port: 80 };

// @ts-expect-error - Omit removes 'port'
const wrongOmit: NoPort = { host: "localhost", port: 80, secure: true };

// Valid
const validPick: HostOnly = { host: "localhost" };
const validOmit: NoPort = { host: "localhost", secure: true };

// =================================================================
// SECTION 31: RECORD TYPE
// =================================================================

type RecordStr = Record<string, number>;

// @ts-expect-error - Values must be numbers
const wrongRecord: RecordStr = { a: "string" };

// Valid
const validRecord: RecordStr = { a: 1, b: 2 };

// Key constraint
type RecordKeys = Record<"a" | "b", string>;

// @ts-expect-error - Key 'c' not in union
const wrongKeys: RecordKeys = { a: "x", b: "y", c: "z" };

// @ts-expect-error - Missing key 'b'
const missingKey: RecordKeys = { a: "x" };

// =================================================================
// SECTION 32: EXCLUDE AND EXTRACT
// =================================================================

type StrOrNum = string | number | boolean;
type OnlyNum = Extract<StrOrNum, number>;
type NoNum = Exclude<StrOrNum, number>;

// @ts-expect-error - Only number extracted
const wrongExtract: OnlyNum = "hello";

// @ts-expect-error - Number excluded
const wrongExclude: NoNum = 42;

// Valid
const validExtract: OnlyNum = 42;
const validExclude: NoNum = "hello";

// =================================================================
// SECTION 33: NONNULLABLE
// =================================================================

type MaybeString = string | null | undefined;
type DefiniteString = NonNullable<MaybeString>;

// @ts-expect-error - null not assignable to NonNullable
const wrongNonNull: DefiniteString = null;

// @ts-expect-error - undefined not assignable to NonNullable
const wrongNonUndefined: DefiniteString = undefined;

// Valid
const validNonNull: DefiniteString = "hello";

// =================================================================
// SECTION 34: PARAMETERS AND RETURNTYPE
// =================================================================

type Fn = (a: string, b: number) => boolean;
type FnParams = Parameters<Fn>; // [string, number]
type FnReturn = ReturnType<Fn>; // boolean

// @ts-expect-error - Wrong parameter types
const wrongParams: FnParams = [42, "hello"];

// @ts-expect-error - Wrong return type
const wrongRetType: FnReturn = "string";

// Valid
const validParams: FnParams = ["hello", 42];
const validRetType: FnReturn = true;

// =================================================================
// SECTION 35: INSTANCETYPE AND CONSTRUCTORPARAMETERS
// =================================================================

class MyClass {
    constructor(public name: string, public age: number) {}
}

type MyInstance = InstanceType<typeof MyClass>;
type MyConstructorParams = ConstructorParameters<typeof MyClass>;

// @ts-expect-error - Wrong constructor params
const wrongCtorParams: MyConstructorParams = [42, "hello"];

// @ts-expect-error - Instance type mismatch
const wrongInstance: MyInstance = { name: "test" };

// Valid
const validCtorParams: MyConstructorParams = ["test", 25];
const validInstance: MyInstance = new MyClass("test", 25);

console.log("TS2322 edge case tests complete");
