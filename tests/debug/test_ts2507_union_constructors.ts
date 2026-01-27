// Test cases for TS2507 with union constructors
// Based on TypeScript conformance tests

declare class MyDate {
    constructor(x: number);
}

// Test 1: Simple union of constructors with different return types (should work)
type Ctor1 = { new (a: number): number };
type Ctor2 = { new (a: number): MyDate };
type UnionCtor = Ctor1 | Ctor2;

var numOrDate: number | MyDate;
numOrDate = new UnionCtor(10); // Should be OK

// Test 2: Union with different parameter types (should error)
type Ctor3 = { new (a: number): number };
type Ctor4 = { new (a: string): MyDate };
type UnionCtor2 = Ctor3 | Ctor4;

new UnionCtor2(10); // Should error - no call signatures
new UnionCtor2("hello"); // Should error - no call signatures

// Test 3: Union with optional parameters
type Ctor5 = { new (a: string, b?: number): string };
type Ctor6 = { new (a: string, b?: number): number };
type UnionCtor3 = Ctor5 | Ctor6;

var strOrNum: string | number;
strOrNum = new UnionCtor3('hello'); // OK
strOrNum = new UnionCtor3('hello', 10); // OK

// Test 4: Union of class constructors
class A {
    constructor(x: number) { }
}

class B {
    constructor(x: number) { }
}

type ClassUnion = typeof A | typeof B;
const instance1 = new ClassUnion(10); // Should work - identical signatures

// Test 5: Union with different signatures
class C {
    constructor(x: string) { }
}

type ClassUnion2 = typeof A | typeof C;
const instance2 = new ClassUnion2(10); // Should error - different parameter types
const instance3 = new ClassUnion2("hello"); // Should error - different parameter types

// Test 6: Type parameter with constructor constraint
function factory<T extends { new (x: number): any }>(ctor: T): any {
    return new ctor(10); // Should work
}

// Test 7: Union of type aliases to constructors
type NumCtor = new (x: number) => number;
type DateCtor = new (x: number) => MyDate;
type AliasUnion = NumCtor | DateCtor;

var result: number | MyDate;
result = new AliasUnion(10); // Should work

// Test 8: Edge case - union with non-constructor
type NonCtor = string;
type UnionCtor4 = Ctor1 | NonCtor;
new UnionCtor4(10); // Should error - string is not a constructor
