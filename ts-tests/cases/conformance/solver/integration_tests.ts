// =================================================================
// CFA + BINDER + SOLVER INTEGRATION TESTS (SOLV-39)
// Tests for cross-cutting issues between Control Flow Analysis,
// Binder symbol resolution, and Solver type checking
// =================================================================

// =================================================================
// SECTION 1: CFA + SOLVER STRICTNESS INTEGRATION
// Tests that control flow analysis works correctly with strict type checking
// =================================================================

// Test 1.1: Type narrowing in conditionals with strict types
function narrowingWithStrictTypes(value: string | number | undefined) {
    if (typeof value === "string") {
        // value should be narrowed to string
        const len: number = value.length; // Valid
        return len;
    }
    if (typeof value === "number") {
        // value should be narrowed to number
        const doubled: number = value * 2; // Valid
        return doubled;
    }
    // value should be undefined here
    return 0;
}

// Test 1.2: Type guard functions with strict mode
function isString(value: unknown): value is string {
    return typeof value === "string";
}

function useTypeGuard(value: unknown): string {
    if (isString(value)) {
        // value should be narrowed to string
        return value.toUpperCase(); // Valid
    }
    return "not a string";
}

// Test 1.3: Definite assignment analysis with type checking
function definiteAssignment() {
    let x: number;

    // Control flow ensures x is assigned before use
    if (Math.random() > 0.5) {
        x = 1;
    } else {
        x = 2;
    }

    // x is definitely assigned here
    const result: number = x * 2; // Valid
    return result;
}

// Test 1.4: Narrowing in switch statements with discriminated unions
interface Circle {
    kind: "circle";
    radius: number;
}

interface Square {
    kind: "square";
    side: number;
}

interface Rectangle {
    kind: "rectangle";
    width: number;
    height: number;
}

type Shape = Circle | Square | Rectangle;

function computeArea(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            // shape narrowed to Circle
            return Math.PI * shape.radius ** 2;
        case "square":
            // shape narrowed to Square
            return shape.side ** 2;
        case "rectangle":
            // shape narrowed to Rectangle
            return shape.width * shape.height;
    }
}

// =================================================================
// SECTION 2: BINDER + SOLVER SCOPE RESOLUTION
// Tests that symbol resolution works correctly with type checking
// =================================================================

// Test 2.1: Block-scoped variable shadowing with type checking
const outerVar: string = "outer";

function testShadowing() {
    const outerVar: number = 42; // Shadows outer, different type
    const result: number = outerVar * 2; // Uses inner outerVar (number)
    return result;
}

// Test 2.2: Module scope with type checking
interface ModuleInterface {
    value: number;
    method(): string;
}

const moduleConst: ModuleInterface = {
    value: 42,
    method() { return "hello"; }
};

function useModuleConst(): number {
    return moduleConst.value; // Resolves module-level constant
}

// Test 2.3: Class member scope resolution
class ScopeTest {
    private value: number = 10;

    getValue(): number {
        return this.value; // Resolves class member
    }

    doubleValue(): number {
        const value = this.value; // Local shadows class member
        return value * 2;
    }

    static staticValue: string = "static";

    static getStaticValue(): string {
        return ScopeTest.staticValue;
    }
}

// Test 2.4: Closure scope resolution with type checking
function closureScope(): () => number {
    const captured: number = 42;

    return function inner(): number {
        // captured is resolved from outer scope
        return captured * 2;
    };
}

// =================================================================
// SECTION 3: UNKNOWN FALLBACK AND GLOBAL SCOPE
// Tests that Unknown type handling doesn't conflict with global scope
// =================================================================

// Test 3.1: Global type checking
declare const globalNumber: number;
const useGlobal: number = globalNumber; // Should resolve global

// Test 3.2: typeof narrowing with globals
declare const maybeGlobal: unknown;

function checkGlobal(value: unknown): number {
    if (typeof value === "number") {
        return value; // value narrowed to number
    }
    return 0;
}

// Test 3.3: Unknown doesn't propagate incorrectly
function handleUnknown(value: unknown): string {
    if (typeof value === "string") {
        // After narrowing, value is string, not unknown
        const len: number = value.length;
        return value;
    }
    // @ts-expect-error - unknown is not assignable to string
    const badAssign: string = value;
    return "default";
}

// Test 3.4: Global this access
declare function setTimeout(callback: () => void, ms: number): number;
declare function console_log(message: string): void;

function useGlobals() {
    const timeoutId: number = setTimeout(() => {}, 1000);
    return timeoutId;
}

// =================================================================
// SECTION 4: END-TO-END TYPE CHECKING INTEGRATION
// Comprehensive tests combining CFA, Binder, and Solver
// =================================================================

// Test 4.1: Complex control flow with type narrowing
type ApiResponse<T> = {
    success: true;
    data: T;
} | {
    success: false;
    error: string;
}

function processResponse<T>(response: ApiResponse<T>): T | null {
    if (response.success) {
        // response narrowed to success case
        return response.data;
    } else {
        // response narrowed to error case
        console.log(response.error);
        return null;
    }
}

// Test 4.2: Class with control flow and type guards
class DataProcessor<T> {
    data: T | null = null;

    setData(value: T): void {
        this.data = value;
    }

    hasData(): boolean {
        return this.data !== null;
    }

    getData(): T {
        if (this.data !== null) {
            return this.data; // data is T after null check
        }
        throw new Error("No data");
    }
}

// Test 4.3: Nested type narrowing
interface NestedData {
    outer?: {
        inner?: {
            value: number;
        };
    };
}

function getNestedValue(data: NestedData): number {
    if (data.outer && data.outer.inner) {
        // All properties narrowed to defined
        return data.outer.inner.value;
    }
    return 0;
}

// Test 4.4: Generic type resolution with control flow
function firstDefined<T>(...args: (T | undefined)[]): T | undefined {
    for (const arg of args) {
        if (arg !== undefined) {
            return arg; // arg is T here
        }
    }
    return undefined;
}

// Test 4.5: Function with type narrowing and callbacks
function callbackNarrowing(value: string | number, callback: (result: string) => void): void {
    if (typeof value === "string") {
        callback(value.toUpperCase());
    } else {
        callback(value.toString());
    }
}

// =================================================================
// SECTION 5: ERROR CASES - VERIFYING ERRORS ARE REPORTED
// =================================================================

// Test 5.1: Type mismatch should be caught
function typeMismatchError(): number {
    // @ts-expect-error - Type 'string' is not assignable to type 'number'
    const x: number = "hello";
    return 0;
}

// Test 5.2: Property access on possibly undefined should error
interface MaybeData {
    value?: number;
}

function unsafeAccess(data: MaybeData) {
    // This is actually allowed - accessing optional property returns number | undefined
    const v: number | undefined = data.value; // Valid

    // @ts-expect-error - Cannot assign number | undefined to number
    const strict: number = data.value;
}

// Test 5.3: Unknown type operations should error
function unknownError(value: unknown) {
    // @ts-expect-error - Object is of type 'unknown'
    const len = value.length;

    // @ts-expect-error - unknown is not assignable to string
    const str: string = value;
}

// Test 5.4: Discriminated union access without narrowing should error
function unionWithoutNarrowing(shape: Shape) {
    // @ts-expect-error - Property 'radius' does not exist on type 'Square'
    return shape.radius;
}

// =================================================================
// SECTION 6: EDGE CASES FOR INTEGRATION
// =================================================================

// Test 6.1: Reassignment with type change in branches
function reassignmentFlowc(condition: boolean): number | string {
    let result: number | string;

    if (condition) {
        result = 42;
    } else {
        result = "hello";
    }

    // result is number | string here
    return result;
}

// Test 6.2: Type assertion in control flow
function typeAssertionFlow(value: unknown): number {
    if (typeof value === "number") {
        return value;
    }

    // Type assertion bypasses flow analysis
    return value as number; // Allowed but unsafe
}

// Test 6.3: Never type in exhaustive checks
function exhaustiveCheck(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            return shape.radius;
        case "square":
            return shape.side;
        case "rectangle":
            return shape.width;
        default:
            // Should never reach here - shape is never
            const _exhaustive: never = shape;
            return _exhaustive;
    }
}

// Test 6.4: Optional chaining with type narrowing
interface DeepObject {
    a?: {
        b?: {
            c: number;
        };
    };
}

function optionalChainingNarrowing(obj: DeepObject): number {
    if (obj.a?.b) {
        // obj.a and obj.a.b are narrowed to defined
        return obj.a.b.c;
    }
    return 0;
}

// Test 6.5: Array type narrowing with filter
function filterNarrowing(values: (string | null)[]): string[] {
    // Filter should narrow the array type
    return values.filter((v): v is string => v !== null);
}

console.log("Integration tests complete");
