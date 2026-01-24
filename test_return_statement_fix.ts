// Test file for return statement type checking fix
// Tests that function expressions and arrow functions now properly check return types

// ============================================================================
// Arrow Function Return Type Tests (Previously Missing)
// ============================================================================

// Test 1: Arrow function returning wrong type
const arrowReturnsNumber = (): number => {
    return "string"; // Should error: TS2322
}

// Test 2: Arrow function expression returning wrong type
const arrowReturnsString = (): string => "wrong"; // Should error: TS2322

// Test 3: Arrow function with object return type
interface Point {
    x: number;
    y: number;
}

const arrowReturnsPoint = (): Point => {
    return { x: 1 }; // Should error: TS2322 - missing 'y' property
}

// Test 4: Generic arrow function with wrong return
const arrowGeneric = <T>(value: T): T => {
    return value as any as T; // May error depending on context
}

// Test 5: Async arrow function returning wrong type
const asyncArrow = async (): Promise<number> => {
    return "string"; // Should error: TS2322
}

// Test 6: Arrow function with union return type
const arrowUnion = (): string | number => {
    return true; // Should error: TS2322
}

// ============================================================================
// Function Expression Return Type Tests (Previously Missing)
// ============================================================================

// Test 7: Function expression returning wrong type
const funcExprReturnsNumber = function(): number {
    return "wrong"; // Should error: TS2322
}

// Test 8: Named function expression returning wrong type
const namedFuncExpr = function returnsString(): string {
    return 42; // Should error: TS2322
}

// Test 9: Function expression with object return
const funcExprReturnsObject = function(): { x: number } {
    return [1, 2]; // Should error: TS2322
}

// Test 10: Async function expression returning wrong type
const asyncFuncExpr = async function(): Promise<number> {
    return "string"; // Should error: TS2322
}

// ============================================================================
// Function Declaration Tests (Should Already Work)
// ============================================================================

// Test 11: Function declaration returning wrong type
function declReturnsNumber(): number {
    return "wrong"; // Should error: TS2322
}

// Test 12: Async function declaration returning wrong type
async function asyncDeclReturns(): Promise<number> {
    return "string"; // Should error: TS2322
}

// ============================================================================
// Method Return Type Tests (Should Already Work)
// ============================================================================

class TestClass {
    methodReturnsNumber(): number {
        return "wrong"; // Should error: TS2322
    }

    arrowMethodReturnsString(): string {
        return 42; // Should error: TS2322
    }
}

// ============================================================================
// Nested Function Tests
// ============================================================================

// Test 13: Arrow function inside function declaration
function outer() {
    const inner = (): number => {
        return "wrong"; // Should error: TS2322
    }
    return inner;
}

// Test 14: Function expression inside object
const obj = {
    method: function(): number {
        return "wrong"; // Should error: TS2322
    },
    arrow: (): string => {
        return 42; // Should error: TS2322
    }
}

// ============================================================================
// Literal Type Return Tests
// ============================================================================

// Test 15: Arrow function returning literal type
const arrowReturnsOne = (): 1 => {
    return 2; // Should error: TS2322
}

// Test 16: Function expression returning literal type
const funcExprReturnsOne = function(): 1 {
    return 2; // Should error: TS2322
}

// ============================================================================
// Void Return Tests
// ============================================================================

// Test 17: Arrow function returning value when void expected
const arrowVoid = (): void => {
    return 42; // Should error: TS2322
}

// Test 18: Function expression returning value when void expected
const funcExprVoid = function(): void {
    return "string"; // Should error: TS2322
}

// ============================================================================
// Array/Tuple Return Tests
// ============================================================================

// Test 19: Arrow function returning wrong array type
const arrowArray = (): number[] => {
    return [1, "2", 3]; // Should error: TS2322
}

// Test 20: Function expression returning tuple when array expected
const funcExprTuple = function(): number[] {
    return [1, 2] as [number, number]; // May error depending on strictness
}

// ============================================================================
// Contextual Typing Tests
// ============================================================================

// Test 21: Arrow function with contextual typing
function takesCallback(cb: () => number) {
    return cb();
}

takesCallback(() => {
    return "wrong"; // Should error: TS2322
})

// Test 22: Function expression with contextual typing
takesCallback(function() {
    return "wrong"; // Should error: TS2322
})

// ============================================================================
// Union/Intersection Return Tests
// ============================================================================

interface A { a: number }
interface B { b: number }

// Test 23: Arrow function returning wrong union type
const arrowUnionReturn = (): A | B => {
    return { x: 1 }; // Should error: TS2322
}

// Test 24: Function expression returning incomplete type
const funcExprUnion = function(): A & B {
    return { a: 1 }; // Should error: TS2322 - missing 'b'
}

// ============================================================================
// Generic Return Type Tests
// ============================================================================

// Test 25: Generic arrow function with constraint
interface WithLength {
    length: number;
}

const genericArrow = <T extends WithLength>(value: T): T => {
    return { length: 0 } as T; // May error depending on inference
}

// Test 26: Generic function expression with wrong return
const genericFuncExpr = function<T>(value: T): T {
    return value as any as T; // Depends on context
}

console.log("Return statement type checking tests complete");
