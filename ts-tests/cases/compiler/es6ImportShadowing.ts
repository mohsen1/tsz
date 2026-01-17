// Test ES6 import shadowing with local variables

// @module: esnext
// @strict: true

// File: moduleA.ts
export { x as exportedX, y, foo } from "./moduleB";

// File: moduleB.ts
export const x = 1;
export const y = 2;
export function foo() { return "foo"; }

// File: test1.ts - import should conflict with top-level let
import { x } from "./moduleB";
let x = 3; // Error: Duplicate identifier 'x'

// File: test2.ts - import should be shadowed by function-local let
import { y } from "./moduleB";

function f1() {
    let y = 10; // OK - shadowing in function scope
    return y;
}

// File: test3.ts - import should be shadowed by block-local let
import { foo } from "./moduleB";

{
    let foo = 42; // OK - shadowing in block scope
}

// File: test4.ts - verify correct scope chain lookup
import { x as x1 } from "./moduleB";

function testScopeChain() {
    // Local x should shadow the imported x1
    let x1 = 100;
    return x1; // Returns 100, not the imported value
}
