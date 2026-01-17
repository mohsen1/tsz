/**
 * TS2454 Conformance Tests - Edge Cases
 *
 * Tests for "Variable is used before being assigned" scenarios
 * covering edge cases identified in CFA conformance analysis.
 *
 * Categories covered:
 * - Class heritage CFA
 * - Static block definite assignment
 * - Computed property CFA
 * - Abstract class patterns
 * - Closure capture scenarios
 */

// @strict: true

// =========================================================================
// SECTION 1: Class Heritage CFA
// =========================================================================

// Test 1.1: Class extending interface with variable assignment
interface Constructable {
    new (): object;
}

function testClassExtendsVariable() {
    let ctor: Constructable;
    class C extends (ctor = class {}) {}  // Assignment in extends clause
    console.log(ctor);  // OK - ctor is assigned in extends clause
}

// Test 1.2: Variable used in class heritage before assignment
function testClassExtendsBeforeAssignment() {
    let x: typeof Error;
    class C extends x {}  // Expected: TS2454 - x used before assignment
    x = Error;
}

// Test 1.3: Class with computed base expression
function testClassComputedBase() {
    let base: new () => object;
    const getBase = () => {
        base = class {};
        return base;
    };
    class C extends getBase() {}
    console.log(base);  // OK - base is assigned by getBase()
}

// Test 1.4: Conditional class heritage
function testConditionalClassHeritage(useBase: boolean) {
    let base: new () => object;
    if (useBase) {
        base = class {};
    }
    // This pattern requires base to be assigned
    class C extends (base ?? Object) {}  // Expected: TS2454 - base may not be assigned
}

// =========================================================================
// SECTION 2: Static Block Definite Assignment
// =========================================================================

// Test 2.1: Static block access before declaration
class StaticBlockBeforeDecl {
    static {
        console.log(this.value);  // Expected: TS2454 - value not yet assigned
    }
    static value: number = 1;
}

// Test 2.2: Static block with variable access
class StaticBlockWithVariable {
    static value: number;
    static {
        let x: number;
        this.value = x;  // Expected: TS2454 - x used before assignment
    }
}

// Test 2.3: Static block with conditional assignment
class StaticBlockConditional {
    static value: number;
    static {
        let x: number;
        if (Math.random() > 0.5) {
            x = 1;
        }
        this.value = x;  // Expected: TS2454 - x may not be assigned
    }
}

// Test 2.4: Static block with proper assignment
class StaticBlockProper {
    static value: number;
    static {
        let x: number;
        x = 1;
        this.value = x;  // OK - x is definitely assigned
    }
}

// Test 2.5: Multiple static blocks with cross-block access
class MultipleStaticBlocks {
    static first: number;
    static {
        this.first = 1;
    }
    static second: number;
    static {
        let x: number;
        x = this.first;  // OK - first is assigned in previous static block
        this.second = x;
    }
}

// Test 2.6: Static block TDZ - accessing before init
class StaticBlockTDZ {
    static {
        console.log(StaticBlockTDZ.later);  // Expected: TS2454 - later not yet initialized
    }
    static later = 42;
}

// =========================================================================
// SECTION 3: Computed Property CFA
// =========================================================================

// Test 3.1: Computed property name with side effect
function testComputedPropertySideEffect() {
    let x: number;
    const obj = {
        [(x = 1)]: "value"  // x assigned in computed property
    };
    console.log(x);  // OK - x is assigned in computed property
}

// Test 3.2: Computed property name accessing unassigned variable
function testComputedPropertyBeforeAssignment() {
    let x: number;
    const obj = {
        [x]: "value"  // Expected: TS2454 - x used before assignment
    };
    x = 1;
}

// Test 3.3: Multiple computed properties with dependencies
function testComputedPropertyDependencies() {
    let x: number;
    let y: number;
    const obj = {
        [(x = 1)]: "first",
        [x + (y = 2)]: "second"  // OK - x is assigned, y is assigned in expression
    };
    console.log(x);  // OK
    console.log(y);  // OK
}

// Test 3.4: Computed property in class declaration
function testClassComputedProperty() {
    let key: string;
    class C {
        [key] = 1;  // Expected: TS2454 - key used before assignment
    }
    key = "prop";
}

// Test 3.5: Symbol computed property
function testSymbolComputedProperty() {
    let sym: symbol;
    const obj = {
        [sym]: "value"  // Expected: TS2454 - sym used before assignment
    };
    sym = Symbol();
}

// Test 3.6: Private name computed property
function testPrivateNameComputedProperty() {
    let key: string;
    class C {
        // Private identifier computed properties
        #privateMethod() {
            let x: number;
            return x;  // Expected: TS2454
        }
    }
}

// =========================================================================
// SECTION 4: Abstract Class Patterns
// =========================================================================

// Test 4.1: Abstract property definite assignment
abstract class AbstractWithProperty {
    abstract value: number;

    method() {
        let x: number;
        x = this.value;  // OK - abstract property is assumed to be implemented
        return x;
    }
}

// Test 4.2: Abstract class with non-abstract property
abstract class AbstractWithNonAbstractProperty {
    value: number;  // Expected: TS2564 - not initialized

    constructor() {
        let x: number;
        x = this.value;  // Expected: TS2454 - value not assigned
    }
}

// Test 4.3: Abstract class with conditional initialization
abstract class AbstractConditionalInit {
    value?: number;

    constructor(init: boolean) {
        if (init) {
            this.value = 1;
        }
    }

    getValue(): number {
        let result: number;
        if (this.value !== undefined) {
            result = this.value;
        }
        return result;  // Expected: TS2454 - result may not be assigned
    }
}

// Test 4.4: Derived class from abstract
abstract class AbstractBase {
    abstract getValue(): number;
}

class DerivedFromAbstract extends AbstractBase {
    private _value: number;

    constructor() {
        super();
        let x: number;
        this._value = x;  // Expected: TS2454 - x used before assignment
    }

    getValue(): number {
        return this._value;
    }
}

// =========================================================================
// SECTION 5: Closure Capture Scenarios
// =========================================================================

// Test 5.1: Arrow function capturing unassigned variable
function testArrowCapture() {
    let x: number;
    const fn = () => x;  // Expected: TS2454 - x may not be assigned when fn is called
    x = 1;
    return fn();
}

// Test 5.2: Function expression capturing unassigned variable
function testFunctionExprCapture() {
    let x: number;
    const fn = function() { return x; };  // Expected: TS2454
    x = 1;
    return fn();
}

// Test 5.3: Nested closure capture
function testNestedClosureCapture() {
    let x: number;
    const outer = () => {
        const inner = () => x;  // Expected: TS2454
        return inner;
    };
    x = 1;
    return outer()();
}

// Test 5.4: Closure with conditional capture
function testConditionalClosureCapture(condition: boolean) {
    let x: number;
    if (condition) {
        x = 1;
    }
    const fn = () => x;  // Expected: TS2454 - x may not be assigned
    return fn();
}

// Test 5.5: Callback passed to function
function testCallbackCapture() {
    let x: number;
    setTimeout(() => {
        console.log(x);  // Expected: TS2454 - x not assigned when callback executes
    }, 0);
    x = 1;
}

// Test 5.6: Array method callback
function testArrayMethodCapture() {
    let x: number;
    [1, 2, 3].forEach(val => {
        if (val === 2) x = val;
    });
    console.log(x);  // Expected: TS2454 - x may not be assigned
}

// =========================================================================
// SECTION 6: Complex Control Flow Edge Cases
// =========================================================================

// Test 6.1: Try-finally with return and assignment
function testTryFinallyReturn() {
    let x: number;
    try {
        return;
    } finally {
        x = 1;
    }
    console.log(x);  // Unreachable but x would be assigned
}

// Test 6.2: Generator function yield
function* testGeneratorYield() {
    let x: number;
    yield;  // Control may return here without x assigned
    x = 1;
    yield x;  // OK
}

// Test 6.3: Async function await
async function testAsyncAwait() {
    let x: number;
    await Promise.resolve();
    x = 1;
    await Promise.resolve();
    console.log(x);  // OK - x is assigned before second await
}

// Test 6.4: Optional chaining with assignment
function testOptionalChainingAssignment(obj: { value?: number } | null) {
    let x: number;
    if (obj?.value) {
        x = obj.value;
    }
    console.log(x);  // Expected: TS2454 - x may not be assigned
}

// Test 6.5: Nullish coalescing with assignment
function testNullishCoalescingAssignment(value: number | null) {
    let x: number;
    x = value ?? (x = 1, 0);  // Complex expression
    console.log(x);  // OK - x is always assigned
}

// Test 6.6: Logical assignment operators
function testLogicalAssignment(obj: { value?: number }) {
    let x: number;
    obj.value ??= (x = 1);
    console.log(x);  // Expected: TS2454 - x may not be assigned if obj.value exists
}

// Test 6.7: Destructuring with default values
function testDestructuringDefault() {
    let x: number;
    const { a = (x = 1) } = { a: undefined };
    console.log(x);  // OK if default is evaluated
}

// Test 6.8: Spread with assignment
function testSpreadAssignment() {
    let x: number;
    const arr = [1, 2, (x = 3)];
    console.log(x);  // OK - x is assigned in array literal
}

// Test 6.9: Template literal with assignment
function testTemplateLiteralAssignment() {
    let x: number;
    const str = `value: ${x = 1}`;
    console.log(x);  // OK - x is assigned in template
}

// Test 6.10: Tagged template with assignment
function testTaggedTemplateAssignment() {
    let x: number;
    const tag = (strings: TemplateStringsArray, ...values: number[]) => values[0];
    const result = tag`value: ${x = 1}`;
    console.log(x);  // OK - x is assigned in template
}

// =========================================================================
// SECTION 7: Type Guard and Assertion Scenarios
// =========================================================================

// Test 7.1: User-defined type guard with assignment
function isNumber(x: unknown): x is number {
    return typeof x === "number";
}

function testTypeGuardAssignment() {
    let x: number;
    let value: unknown = 42;
    if (isNumber(value)) {
        x = value;
    }
    console.log(x);  // Expected: TS2454 - x may not be assigned
}

// Test 7.2: Assertion function with assignment
function assertDefined<T>(value: T | undefined): asserts value is T {
    if (value === undefined) throw new Error();
}

function testAssertionAssignment() {
    let x: number;
    let value: number | undefined;
    assertDefined(value);
    x = value;  // OK after assertion
    console.log(x);
}

// Test 7.3: In operator narrowing with assignment
function testInOperatorAssignment(obj: { a: number } | { b: string }) {
    let x: number;
    if ("a" in obj) {
        x = obj.a;
    }
    console.log(x);  // Expected: TS2454 - x may not be assigned
}

// Test 7.4: Instanceof narrowing with assignment
function testInstanceofAssignment(value: Error | string) {
    let x: string;
    if (value instanceof Error) {
        x = value.message;
    }
    console.log(x);  // Expected: TS2454 - x may not be assigned
}

// Test 7.5: Typeof narrowing with assignment
function testTypeofAssignment(value: number | string) {
    let x: number;
    if (typeof value === "number") {
        x = value;
    }
    console.log(x);  // Expected: TS2454 - x may not be assigned
}
