// Test Rule #26: Split Accessors (Getter/Setter Variance)
//
// This tests that properties with different getter and setter types
// properly check:
// - Covariant reads: source.read <: target.read
// - Contravariant writes: target.write <: source.write

// Test 1: Basic split accessor pattern
// Source has narrower setter, target has wider setter (should pass)
class Base1 {
    private _x: string | number;
    get x(): string {
        return this._x as string;
    }
    set x(v: string | number) {
        this._x = v;
    }
}

class Derived1 extends Base1 {
    // Derived can have narrower setter (contravariant)
    set x(v: string) {
        super.x = v;
    }
}

const b1: Base1 = new Derived1(); // OK: Derived1 setter is narrower (string)
b1.x = "hello"; // OK
b1.x = 42; // Type error at compile time: number is not assignable to string in Derived1

// Test 2: Split accessor in source type
interface ReadOnly {
    get x(): string; // readonly property
}

interface ReadWrite {
    get x(): string;
    set x(v: string);
}

const ro: ReadOnly = { x: "hello" }; // OK
const rw: ReadWrite = { x: "hello", set x(v: string) {} };

let test2: ReadWrite = ro; // Error: ReadOnly can't satisfy ReadWrite (missing setter)
let test2b: ReadOnly = rw; // OK: ReadWrite can satisfy ReadOnly (has getter)

// Test 3: Getter with wider type, setter with narrower type
class Base3 {
    protected _value: string | number | boolean;
    get value(): string {
        return String(this._value);
    }
    set value(v: string | number | boolean) {
        this._value = v;
    }
}

class Derived3 extends Base3 {
    // Getter returns same type (string), setter accepts narrower (string | number)
    set value(v: string | number) {
        super.value = v;
    }
}

const b3: Base3 = new Derived3(); // OK: Derived3 setter is narrower

// Test 4: Readonly target properties only check read type
interface ReadonlyTarget {
    readonly x: string;
}

interface MutableSource {
    x: string;
}

let test4: ReadonlyTarget = { x: "hello" }; // OK
let test4b: ReadonlyTarget = { x: "hello", set x(v: string) {} }; // OK - setter ignored

// Test 5: Contravariant write type check
// If target.write is narrower than source.write, it should fail
interface WideSetter {
    get x(): string;
    set x(v: string | number); // accepts string OR number
}

interface NarrowSetter {
    get x(): string;
    set x(v: string); // accepts ONLY string
}

// This should error: NarrowSetter can't satisfy WideSetter
// because WideSetter.write (string | number) is NOT a subtype of NarrowSetter.write (string)
let test5a: WideSetter = { x: "test", set x(v: string) {} }; // Error!

// This should work: WideSetter can satisfy NarrowSetter
let test5b: NarrowSetter = { x: "test", set x(v: string | number) {} }; // OK

// Test 6: Property with union getter type
class Base6 {
    private _state: "loading" | "success" | "error";
    get state(): "loading" | "success" | "error" {
        return this._state;
    }
    set state(s: "loading" | "success" | "error") {
        this._state = s;
    }
}

class Derived6 extends Base6 {
    // Derived can narrow the setter to just non-loading states
    set state(s: "success" | "error") {
        super.state = s;
    }
}

const b6: Base6 = new Derived6(); // OK
b6.state = "success"; // OK
// b6.state = "loading"; // Would be error in Derived6

// Test 7: Method bivariance should still work for methods
interface WithMethods {
    method(x: string): void;
}

interface WithMethod2 {
    method(x: string | number): void;
}

// Methods are bivariant in TypeScript
const test7a: WithMethods = { method: (x: string | number) => {} }; // OK (bivariant)
const test7b: WithMethod2 = { method: (x: string) => {} }; // OK (bivariant)

console.log("Rule #26 tests compiled");
