/**
 * TS2564 Conformance Tests - Edge Cases
 *
 * Tests for "Property has no initializer and is not definitely assigned
 * in the constructor" scenarios covering edge cases identified in CFA
 * conformance analysis.
 *
 * Categories covered:
 * - Computed property names in classes
 * - Symbol property initialization
 * - Class expression type inference
 * - ES5 target-specific patterns
 * - Derived class initialization
 */

// @strict: true
// @strictPropertyInitialization: true

// =========================================================================
// SECTION 1: Computed Property Names in Classes
// =========================================================================

// Test 1.1: Basic computed property without initialization
const key1 = "computedKey";
class ComputedPropertyBasic {
    [key1]: number;  // Expected: TS2564 - not initialized
}

// Test 1.2: Computed property with symbol key
const symKey = Symbol("key");
class ComputedSymbolProperty {
    [symKey]: string;  // Expected: TS2564 - not initialized
}

// Test 1.3: Computed property initialized in constructor
const key2 = "initInConstructor";
class ComputedPropertyInitialized {
    [key2]: number;

    constructor() {
        this[key2] = 42;  // OK - initialized in constructor
    }
}

// Test 1.4: Multiple computed properties
const key3 = "keyA";
const key4 = "keyB";
class MultipleComputedProperties {
    [key3]: number;      // Expected: TS2564
    [key4]: string = ""; // OK - has initializer
}

// Test 1.5: Computed property with getter key
function getKey(): string { return "dynamicKey"; }
class ComputedPropertyFromFunction {
    [getKey()]: number;  // Expected: TS2564
}

// Test 1.6: Template literal computed property
const prefix = "prop_";
class TemplateLiteralProperty {
    [`${prefix}value`]: number;  // Expected: TS2564
}

// =========================================================================
// SECTION 2: Symbol Property Initialization
// =========================================================================

// Test 2.1: Well-known symbol property
class WellKnownSymbolProperty {
    [Symbol.iterator]: () => Iterator<number>;  // Expected: TS2564
}

// Test 2.2: Multiple symbol properties
const sym1 = Symbol("first");
const sym2 = Symbol("second");
class MultipleSymbolProperties {
    [sym1]: number;  // Expected: TS2564
    [sym2]: string;  // Expected: TS2564
}

// Test 2.3: Symbol property with default value
const symWithDefault = Symbol("withDefault");
class SymbolPropertyWithDefault {
    [symWithDefault]: number = 42;  // OK - has initializer
}

// Test 2.4: Symbol property initialized conditionally
const symConditional = Symbol("conditional");
class SymbolPropertyConditional {
    [symConditional]: number;  // Expected: TS2564

    constructor(init: boolean) {
        if (init) {
            this[symConditional] = 1;
        }
        // Not all paths initialize
    }
}

// Test 2.5: Symbol property with definite assignment assertion
const symAsserted = Symbol("asserted");
class SymbolPropertyAsserted {
    [symAsserted]!: number;  // OK - definite assignment assertion
}

// =========================================================================
// SECTION 3: Class Expression Type Inference
// =========================================================================

// Test 3.1: Basic class expression
const ClassExpr1 = class {
    value: number;  // Expected: TS2564
};

// Test 3.2: Named class expression
const ClassExpr2 = class NamedClass {
    value: string;  // Expected: TS2564
};

// Test 3.3: Class expression extending another class
class Base {
    baseValue: number = 0;
}

const DerivedExpr = class extends Base {
    derivedValue: string;  // Expected: TS2564
};

// Test 3.4: Generic class expression
const GenericClassExpr = class<T> {
    value: T;  // Expected: TS2564
};

// Test 3.5: Class expression with constructor
const ClassExprWithCtor = class {
    value: number;

    constructor() {
        this.value = 42;  // OK - initialized in constructor
    }
};

// Test 3.6: Class expression as parameter type
function takeClass(ctor: new () => { value: number }) {}

takeClass(class {
    value: number;  // Expected: TS2564
});

// Test 3.7: Class expression in object literal
const obj = {
    MyClass: class {
        value: number;  // Expected: TS2564
    }
};

// Test 3.8: Immediately instantiated class expression
const instance = new (class {
    value: number;  // Expected: TS2564
})();

// =========================================================================
// SECTION 4: Derived Class Initialization
// =========================================================================

// Test 4.1: Derived class with own property
class BaseClass {
    baseValue: number = 0;
}

class DerivedWithOwnProperty extends BaseClass {
    derivedValue: string;  // Expected: TS2564
}

// Test 4.2: Derived class calling super with property init
class DerivedWithSuperCall extends BaseClass {
    value: number;

    constructor() {
        super();
        this.value = 1;  // OK - initialized after super()
    }
}

// Test 4.3: Derived class with property before super
class DerivedBeforeSuper extends BaseClass {
    value: number;  // Expected: TS2564 (can't init before super)

    constructor() {
        // this.value = 1;  // Would be error - before super
        super();
    }
}

// Test 4.4: Derived class with conditional super path
class ConditionalSuperPath extends BaseClass {
    value: number;  // Expected: TS2564

    constructor(init: boolean) {
        super();
        if (init) {
            this.value = 1;
        }
        // Not all paths initialize
    }
}

// Test 4.5: Deep inheritance chain
class Level0 {
    level0: number = 0;
}

class Level1 extends Level0 {
    level1: number;  // Expected: TS2564
}

class Level2 extends Level1 {
    level2: number;  // Expected: TS2564

    constructor() {
        super();
        this.level2 = 2;  // Only initializes level2, not level1
    }
}

// Test 4.6: Abstract class implementation
abstract class AbstractBase {
    abstract abstractValue: number;
    concreteValue: number;  // Expected: TS2564
}

class ConcreteImpl extends AbstractBase {
    abstractValue: number;  // Expected: TS2564

    constructor() {
        super();
        this.abstractValue = 1;
    }
}

// =========================================================================
// SECTION 5: Constructor Control Flow Patterns
// =========================================================================

// Test 5.1: Constructor with try-catch
class CtorWithTryCatch {
    value: number;  // Expected: TS2564

    constructor() {
        try {
            this.value = 1;
        } catch {
            // value not initialized in catch path
        }
    }
}

// Test 5.2: Constructor with try-finally
class CtorWithTryFinally {
    value: number;

    constructor() {
        try {
            throw new Error();
        } finally {
            this.value = 1;  // OK - finally always runs
        }
    }
}

// Test 5.3: Constructor with early return
class CtorWithEarlyReturn {
    value: number;  // Expected: TS2564

    constructor(init: boolean) {
        if (!init) {
            return;  // Early return without initializing
        }
        this.value = 1;
    }
}

// Test 5.4: Constructor with throw
class CtorWithThrow {
    value: number;

    constructor(valid: boolean) {
        if (!valid) {
            throw new Error("Invalid");
        }
        this.value = 1;  // OK - only reached if valid
    }
}

// Test 5.5: Constructor with loop assignment
class CtorWithLoop {
    value: number;  // Expected: TS2564

    constructor() {
        for (let i = 0; i < 0; i++) {
            this.value = i;  // Loop may not execute
        }
    }
}

// Test 5.6: Constructor with switch
class CtorWithSwitch {
    value: number;  // Expected: TS2564

    constructor(type: number) {
        switch (type) {
            case 0:
                this.value = 0;
                break;
            case 1:
                this.value = 1;
                break;
            // default case missing
        }
    }
}

// Test 5.7: Constructor with switch and default
class CtorWithSwitchDefault {
    value: number;

    constructor(type: number) {
        switch (type) {
            case 0:
                this.value = 0;
                break;
            default:
                this.value = -1;
                break;
        }
    }
}

// =========================================================================
// SECTION 6: Special Property Patterns
// =========================================================================

// Test 6.1: Optional property (no error expected)
class OptionalProperty {
    value?: number;  // OK - optional
}

// Test 6.2: Property with union type including undefined
class UndefinedUnionProperty {
    value: number | undefined;  // OK - undefined is allowed
}

// Test 6.3: Readonly property without initializer
class ReadonlyProperty {
    readonly value: number;  // Expected: TS2564

    constructor() {
        this.value = 42;  // OK - can initialize readonly in constructor
    }
}

// Test 6.4: Readonly property not initialized
class ReadonlyNotInitialized {
    readonly value: number;  // Expected: TS2564
}

// Test 6.5: Private property
class PrivateProperty {
    private value: number;  // Expected: TS2564
}

// Test 6.6: Protected property
class ProtectedProperty {
    protected value: number;  // Expected: TS2564
}

// Test 6.7: Property with method initialization
class PropertyFromMethod {
    value: number;

    constructor() {
        this.initialize();
    }

    initialize() {
        this.value = 42;  // Assignment in method doesn't count for TS2564
    }
}

// Test 6.8: Property initialized via destructuring in constructor
class PropertyFromDestructuring {
    a: number;
    b: string;

    constructor(data: { a: number; b: string }) {
        ({ a: this.a, b: this.b } = data);  // OK - initialized via destructuring
    }
}

// Test 6.9: Property with numeric literal key
class NumericKeyProperty {
    0: number;  // Expected: TS2564
    1: string = "";  // OK - has initializer
}

// Test 6.10: Property with string literal key
class StringLiteralKeyProperty {
    "my-property": number;  // Expected: TS2564
    "other-property": string = "";  // OK
}

// =========================================================================
// SECTION 7: Parameter Properties
// =========================================================================

// Test 7.1: Basic parameter property (no error expected)
class ParameterPropertyBasic {
    constructor(public value: number) {}  // OK - parameter property
}

// Test 7.2: Mixed parameter and regular properties
class MixedProperties {
    regular: string;  // Expected: TS2564

    constructor(public param: number) {
        // regular not initialized
    }
}

// Test 7.3: Readonly parameter property
class ReadonlyParameterProperty {
    constructor(public readonly value: number) {}  // OK
}

// Test 7.4: Private parameter property
class PrivateParameterProperty {
    constructor(private value: number) {}  // OK
}

// Test 7.5: Protected parameter property
class ProtectedParameterProperty {
    constructor(protected value: number) {}  // OK
}

// =========================================================================
// SECTION 8: Decorators and Property Initialization
// =========================================================================

// Test 8.1: Property with type that has complex initialization
interface ComplexType {
    nested: {
        value: number;
    };
}

class ComplexPropertyType {
    complex: ComplexType;  // Expected: TS2564
}

// Test 8.2: Property with function type
class FunctionTypeProperty {
    callback: () => void;  // Expected: TS2564
}

// Test 8.3: Property with generic constraint
class GenericConstraintProperty<T extends { id: number }> {
    item: T;  // Expected: TS2564
}

// Test 8.4: Property with conditional type
type ConditionalValue<T> = T extends string ? string : number;

class ConditionalTypeProperty<T> {
    value: ConditionalValue<T>;  // Expected: TS2564
}

// Test 8.5: Property with mapped type
type Mapped<T> = { [K in keyof T]: T[K] };

class MappedTypeProperty {
    mapped: Mapped<{ a: number }>;  // Expected: TS2564
}
