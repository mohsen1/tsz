// Test to understand when TS2507 should actually be emitted

// Case 1: Using a non-constructor value with 'new'
const notAConstructor = 42;
new notAConstructor(); // Should be TS2507

// Case 2: Using a primitive type with 'new'
type PrimitiveType = string | number;
const x: PrimitiveType = "hello";
new x(); // Should be TS2507

// Case 3: Union of constructors with different signatures
class A { constructor(x: number) {} }
class B { constructor(x: string) {} }

function test1(ctorUnion1: typeof A | typeof B) {
    // TypeScript should give "no call signatures" not "not a constructor"
    // because both ARE constructors, just with incompatible signatures
    new ctorUnion1(); // What error should this give?
}

// Case 4: Union containing a non-constructor
function test2(ctorUnion2: typeof A | string) {
    new ctorUnion2(); // Should be TS2507 - one member is not a constructor
}
