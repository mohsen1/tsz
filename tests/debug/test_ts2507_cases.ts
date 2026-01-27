// Test case 1: Should NOT error - class constructor
class Base {
    x: number;
}
class C1 extends Base { }

// Test case 2: Should NOT error - typeof class
type Constructor<T> = new (...args: any[]) => T;
function extend<T>(ctor: Constructor<T>) {
    class C2 extends ctor { }
}

// Test case 3: Should NOT error - type parameter with constructor constraint
function factory<T extends new () => any>(ctor: T) {
    class C3 extends ctor { }
}

// Test case 4: Should error - primitive literals
class C4 extends undefined { }  // Error: TS2507
class C5 extends true { }       // Error: TS2507
class C6 extends false { }      // Error: TS2507
class C7 extends 42 { }         // Error: TS2507
class C8 extends "hello" { }    // Error: TS2507

// Test case 5: Should error - object literal
var x: {};
class C9 extends x { }          // Error: TS2507

// Test case 6: Should error - plain function without construct signatures
function foo() {
    this.x = 1;
}
class C10 extends foo { }       // Error: TS2507

// Test case 7: Should error - arrow function
const bar = () => {};
class C11 extends bar { }       // Error: TS2507

// Test case 8: Should NOT error - interface with construct signature
interface Constructable {
    new (): any;
}
function test<T extends Constructable>(ctor: T) {
    class C12 extends ctor { }
}

// Test case 9: Should error - union type
type MaybeConstructor = (new () => any) | undefined;
function testUnion(ctor: MaybeConstructor) {
    class C13 extends ctor { }  // Error: TS2507 (union includes undefined)
}

// Test case 10: Should NOT error - intersection with constructor
type MixinConstructor = new () => any;
type WithPrototype = { prototype: any };
type ConstructorWithPrototype = MixinConstructor & WithPrototype;
function testIntersection(ctor: ConstructorWithPrototype) {
    class C14 extends ctor { }
}
