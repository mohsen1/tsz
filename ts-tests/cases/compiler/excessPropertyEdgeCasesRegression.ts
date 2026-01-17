// @strict: true
// Regression tests for excess property checking edge cases
// This file documents known edge cases that should continue to work correctly

// ============================================================================
// Edge Case 1: Nested excess properties in intersections
// ============================================================================

let obj1: { a: { x: string } } & { c: number } = { a: { x: 'hello', y: 2 }, c: 5 };  // Nested excess property - should error on 'y'
let obj2: { a: { x: string } } & { c: number } = { a: { x: 'hello' }, c: 5 };  // OK

// ============================================================================
// Edge Case 2: Intersections with generic type parameters
// ============================================================================

function testGeneric<T extends { x: string }>() {
    // No excess property checks on bare generic type parameters
    const obj1: T = { x: "test", y: 123 };  // OK - T is a type parameter

    // No excess property checks on intersections containing generics
    const obj2: T & { prop: boolean } = { x: "test", prop: true, extra: 123 };  // OK - intersection contains generic T

    // Excess property checks on non-generic parts of unions
    const obj3: T | { prop: boolean } = { x: "test", prop: true };  // OK
    const obj4: { prop: boolean } = { prop: true, extra: 123 };  // Should error on 'extra'
}

// ============================================================================
// Edge Case 3: Unions with 'object' type
// ============================================================================

const obj5: object | { x: string } = { z: 'abc' };  // OK - 'object' type makes union permissive
const obj6: object & { x: string } = { z: 'abc' };  // Should error on 'z'

// ============================================================================
// Edge Case 4: Index signatures
// ============================================================================

interface Indexed {
    [n: number]: { x?: number };
}

const obj7: Indexed = { 0: { }, '1': { } };  // OK - index signature allows any numeric key
const obj8: Indexed = { 0: { x: 1 }, '1': { y: 2 } };  // Should error on 'y' (nested excess property)

interface StringIndexed {
    [key: string]: number;
}

const obj9: StringIndexed = { a: 1, b: 2 };  // OK
const obj10: StringIndexed = { a: 1, b: 2, c: 'hello' };  // Should error on 'c' (wrong type)

// ============================================================================
// Edge Case 5: Empty and weak types in intersections
// ============================================================================

interface Empty {}

const obj11: Empty & { x: number } = { y: "hello" };  // Should error on 'y'
const obj12: Empty & { x: number } = { x: 1 };  // OK

interface A { x?: string }

const obj13: A & ThisType<any> = { y: 10 };  // Should error on 'y'
const obj14: A & ThisType<any> = { x: "hello" };  // OK

// ============================================================================
// Edge Case 6: Discriminated unions with nested excess properties
// ============================================================================

type AN = { a: string } | { c: string }
type BN = { b: string }
type AB = { kind: "A", n: AN } | { kind: "B", n: BN }

const obj15: AB = {
    kind: "A",
    n: {
        a: "a",
        b: "b",  // Should error - 'b' doesn't exist in { a: string } | { c: string }
    }
}

const obj16: AB = {
    kind: "A",
    n: {
        a: "a",
        c: "c",  // OK - 'c' exists in union { a: string } | { c: string }
    }
}

// ============================================================================
// Edge Case 7: Complex intersection with optional properties
// ============================================================================

function foo<T extends object>(x: { a?: string }, y: T & { a: boolean }) {
    x = y;  // Should error - type mismatch on property 'a'
}

// ============================================================================
// Edge Case 8: Freshness preservation and loss
// ============================================================================

const fresh1 = { x: 1, y: 2 };
const obj17: { x: number } = fresh1;  // OK - freshness lost after assignment

const obj18: { x: number } = { x: 1, y: 2 };  // Should error on 'y' (fresh object literal)

// Freshness lost after spreading
declare let t0: { a: any, b: any } | { d: any, e: any }
let t1 = { ...t0, f: 1 };
const obj19: { a: any, b: any } | { d: any, e: any } = t1;  // OK - freshness lost
