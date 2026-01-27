// Test case for TS2304 caching regression fix
// Tests that type nodes are properly re-evaluated when type parameter bindings change
// and that only ERROR results are cached to prevent false positives

// Test 1: Generic type parameter should resolve correctly
function testGeneric<T extends Array<number>>(x: T) {
    // Array should resolve correctly even though it might have been cached
    // in a different context
    let y: Array<string>;
    return y;
}

// Test 2: Type parameter used in nested context
function testNested<T>(x: T) {
    function inner() {
        // T should resolve correctly here, even though it's referenced
        // from within a nested function
        let y: T;
        return y;
    }
    return inner;
}

// Test 3: Same type name in different generic contexts
function testMultipleGenerics<T>(x: T) {
    let a: Array<T>;  // Array with T
}

function testMultipleGenerics2<U>(x: U) {
    let b: Array<U>;  // Array with U - should resolve independently
}

// Test 4: Forward reference within scope
function testForwardReference() {
    let x: ForwardType;  // Should resolve - ForwardType declared below
}

type ForwardType = number;

// Test 5: Type alias referencing later alias
type FirstAlias = SecondAlias;  // Should resolve
type SecondAlias = string;

// Test 6: Generic class with type parameter
class GenericClass<T> {
    value: T;
    method(): T {
        return this.value;
    }
}

// Test 7: Generic class instantiation
let instance1: GenericClass<number>;  // Should resolve
let instance2: GenericClass<string>;  // Should resolve independently

// Test 8: Interface extending generic interface
interface Base<T> {
    value: T;
}

interface Derived extends Base<number> {  // Should resolve
    extra: string;
}

// Test 9: Recursive type definition
type Tree<T> = {
    value: T;
    left: Tree<T> | null;
    right: Tree<T> | null;
};

// Test 10: Conditional type with type parameter
type Conditional<T> = T extends number ? string : boolean;

function testConditional<T>(x: T): Conditional<T> {
    return x as any;
}

// Test 11: Multiple type parameters
function testMultipleParams<T, U, V>(x: T, y: U, z: V) {
    let a: T;
    let b: U;
    let c: V;
    return { a, b, c };
}

// Test 12: Type parameter constraint
function testConstraint<T extends Array<number>>(x: T) {
    // Array should resolve even though T is constrained
    let y: Array<string>;
    return y;
}

// Test 13: Type parameter in nested generic
function testNestedGeneric<T>(x: T) {
    let y: Array<Array<T>>;  // Nested Array with T
    return y;
}

// Test 14: Generic function with multiple references to same type parameter
function testMultipleRefs<T>(x: T, y: T): T {
    let a: T;
    let b: T;
    return x;
}

// Test 15: Type parameter default
function testDefault<T = number>(x?: T): T {
    return x as T;
}

// Test 16: Undefined identifier should still emit error (only once)
function testUndefined() {
    let x: UndefinedType1;  // Should emit TS2304
    let y: UndefinedType1;  // Should NOT emit duplicate TS2304 (cached ERROR)
    return { x, y };
}

// Test 17: Undefined type parameter should emit error
function testUndefinedParam<T extends UndefinedType2>(x: T) {
    // Should emit TS2304 for UndefinedType2
    return x;
}

// Test 18: Mix of defined and undefined types
function testMix() {
    let a: number;  // Should resolve
    let b: UndefinedType3;  // Should emit TS2304
    let c: string;  // Should resolve
    let d: UndefinedType3;  // Should NOT emit duplicate TS2304 (cached ERROR)
    return { a, b, c, d };
}
