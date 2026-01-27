// Simple test for TS2507 with union constructors

class A {
    constructor(x: number) { }
}

class B {
    constructor(x: number) { }
}

// Direct union of class constructors
var ctorUnion: typeof A | typeof B;
var instance1 = new ctorUnion(10); // Should work

// Union with different signatures
class C {
    constructor(x: string) { }
}

var ctorUnion2: typeof A | typeof C;
var instance2 = new ctorUnion2(10); // Should error TS2507
var instance3 = new ctorUnion2("hello"); // Should error TS2507

// Interface with construct signatures
interface D {
    new (x: number): number;
}

interface E {
    new (x: number): string;
}

var ctorUnion3: D | E;
var instance4 = new ctorUnion3(10); // Should work - same params
