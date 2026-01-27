// Comprehensive TS2749 test cases

// ===============================================
// SHOULD NOT ERROR (Valid type usage)
// ===============================================

// 1. Class used as type
class MyClass {
    x: number;
}
let a: MyClass; // OK

// 2. Interface used as type
interface MyInterface {
    y: string;
}
let b: MyInterface; // OK

// 3. Enum used as type
enum MyEnum {
    A, B, C
}
let c: MyEnum; // OK

// 4. Type alias used as type
type MyType = { z: boolean };
let d: MyType; // OK

// 5. Namespace with qualified name
namespace NS {
    export interface Inner {
        prop: number;
    }
}
let e: NS.Inner; // OK

// 6. Qualified name with type arguments
namespace Container {
    export class Box<T> {
        value: T;
    }
}
let f: Container.Box<string>; // OK

// 7. Type literal with qualified name
let g: {
    field: NS.Inner;
}; // OK

// 8. Nested qualified names
namespace Outer {
    export namespace Middle {
        export interface Deep {
            val: number;
        }
    }
}
let h: Outer.Middle.Deep; // OK

// ===============================================
// SHOULD ERROR (Invalid type usage)
// ===============================================

// 9. Variable used as type
const myVar = 42;
// let x: myVar; // SHOULD ERROR: TS2749

// 10. Function used as type
function myFunc() { return 1; }
// let y: myFunc; // SHOULD ERROR: TS2749

// 11. Namespace + function merge (value-only context)
namespace A {
    export function B<T>(x: T) { return x; }
    export namespace B {
        export var x = 1;
    }
}
// var z: A.B; // SHOULD ERROR: TS2749 (B is primarily a function)
