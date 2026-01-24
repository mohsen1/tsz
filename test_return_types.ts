// Test file for missing TS2322 errors on return statements
// These should error but currently might not

// Test 1: Returning string when number expected
function returnsNumber(): number {
    return "wrong"; // Should error: TS2322
}

// Test 2: Returning number when string expected
function returnsString(): string {
    return 42; // Should error: TS2322
}

// Test 3: Returning null when non-nullable type expected
function returnsNonNull(): string {
    return null; // Should error: TS2322 (with strictNullChecks)
}

// Test 4: Returning undefined when non-nullable type expected
function returnsObject(): { x: number } {
    return undefined; // Should error: TS2322
}

// Test 5: Returning incompatible object type
interface Point2D {
    x: number;
    y: number;
}

interface Point3D {
    x: number;
    y: number;
    z: number;
}

function returnsPoint2D(): Point2D {
    const p: Point3D = { x: 1, y: 2, z: 3 };
    return p; // Should error: TS2322
}

// Test 6: Returning array when object expected
function returnsObject(): { x: number } {
    return [1, 2, 3]; // Should error: TS2322
}

// Test 7: Returning object when array expected
function returnsArray(): number[] {
    return { 0: 1, 1: 2 }; // Should error: TS2322
}

// Test 8: Generic function - returning wrong type
function identity<T>(value: T): T {
    return value as any; // Should error: TS2322 when T is constrained
}

// Test 9: Function with union return type - returning wrong type
function returnsStringOrNumber(): string | number {
    return true; // Should error: TS2322
}

// Test 10: Async function returning wrong type
async function returnsPromise(): Promise<number> {
    return "string"; // Should error: TS2322
}

// Test 11: Function returning tuple when single value expected
function returnsSingle(): number {
    return [1, 2]; // Should error: TS2322
}

// Test 12: Function with literal return type
function returnsOne(): 1 {
    return 2; // Should error: TS2322
}

// Test 13: Arrow function with wrong return type
const arrowReturnsNumber = (): number => {
    return "string"; // Should error: TS2322
}

// Test 14: Method returning wrong type
class MyClass {
    getValue(): number {
        return "wrong"; // Should error: TS2322
    }
}

// Test 15: Generic class method with constraint
class Container<T extends { x: number }> {
    get(): T {
        return { x: 1, y: 2 } as any; // Should error: TS2322
    }
}

console.log("All return statement type checking tests");
