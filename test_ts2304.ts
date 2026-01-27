// Test cases for TS2304 patterns

// 1. Simple undefined identifier (should error)
const x = asdf;

// 2. Type annotation (should error)
const y: UndefinedType;

// 3. Function parameter (should error)
function foo(x: UndefinedParam): UndefinedReturn {}

// 4. Class property (should error)
class C {
    prop: UndefinedProp;
}

// 5. Generic type argument (should error)
type T = Array<UndefinedGeneric>;

// 6. Implements clause (should error)
class C2 implements UndefinedInterface {}

// 7. Extends clause (should error)
interface I extends UndefinedBase {}

// 8. typeof operator (should error if name doesn't exist)
type TypeofUndefined = typeof undefinedValue;

// 9. Import alias (should error differently - TS2503)
import alias = UndefinedNamespace;

// 10. Switch expression (should error)
switch (undefinedSwitch) {
    case 1: break;
}
