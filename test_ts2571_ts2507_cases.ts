// Test cases for TS2571 and TS2507 false positives

// === TS2571 Tests ===

// Test 1: Catch clause variable with typeof narrowing
try {
    throw new Error("test");
} catch (error: unknown) {
    if (typeof error === "object" && error !== null) {
        if ("message" in error) {
            console.log(error.message); // Should NOT error TS2571
        }
    }
}

// Test 2: Catch clause variable with instanceof
try {
    throw new Error("test");
} catch (error: unknown) {
    if (error instanceof Error) {
        console.log(error.message); // Should NOT error TS2571
    }
}

// Test 3: Function parameter with unknown and type guard
function processUnknown(val: unknown) {
    if (typeof val === "string") {
        console.log(val.toUpperCase()); // Should NOT error TS2571
    }
}

// Test 4: in operator narrowing
function hasProperty(obj: unknown) {
    if ("prop" in obj) {
        console.log(obj.prop); // Should NOT error TS2571
    }
}

// Test 5: Falsy narrowing
function ifTruthy(val: unknown) {
    if (val) {
        // val is narrowed to non-falsy types
        console.log(val); // Should NOT error TS2571
    }
}

// === TS2507 Tests ===

// Test 6: Class expression in extends clause
const BaseClass = class {
    constructor() {
        this.x = 1;
    }
};

class Derived extends BaseClass {
    // Should NOT error TS2507
    y = 2;
}

// Test 7: Function with prototype as constructor
function Foo() {
    this.x = 1;
}
Foo.prototype = {
    getY() { return 2; }
};

const foo = new Foo(); // Should NOT error TS2507

// Test 8: Generic constraint with constructor type
interface Constructor<T> {
    new(...args: any[]): T;
}

function createInstance<T>(ctor: Constructor<T>): T {
    return new ctor(); // Should NOT error TS2507
}

// Test 9: Class expression with type parameters
const GenericBase = class<T> {
    value: T;
    constructor(val: T) {
        this.value = val;
    }
};

class DerivedGeneric extends GenericBase<string> {
    // Should NOT error TS2507
}

// Test 10: Intersection type for mixins
type Constructor<T> = new (...args: any[]) => T;

function Mixin<TBase extends Constructor<{}>>(Base: TBase) {
    return class extends Base {
        mixinMethod() {
            return 1;
        }
    };
}

class Base {
    baseMethod() {}
}

const Mixed = Mixin(Base);
const instance = new Mixed(); // Should NOT error TS2507
