// @strict: true
// Test nested scope shadowing with different declaration types

// Test 1: Module-level declarations should conflict
const global1 = 1;
const global1 = 2; // Error: Duplicate identifier

// Test 2: Function-local can shadow module-level
const moduleLevel = 10;
function testFunctionShadow() {
    const moduleLevel = 20; // OK - shadows outer
    return moduleLevel;
}

// Test 3: Block-local can shadow function-local
function testBlockShadow() {
    const fnLevel = 30;
    {
        const fnLevel = 40; // OK - shadows outer
        return fnLevel;
    }
}

// Test 4: let shadows const
const outerConst = 50;
function testLetShadowsConst() {
    let outerConst = 60; // OK - shadows outer
    return outerConst;
}

// Test 5: var shadows let
let outerLet = 70;
function testVarShadowsLet() {
    var outerLet = 80; // OK - shadows outer
    return outerLet;
}

// Test 6: Loop variables can shadow outer declarations
const x = 100;
for (let x = 0; x < 10; x++) {
    // OK - loop variable shadows outer x
    const value = x;
}

// Test 7: Function parameters can shadow outer declarations
const y = 200;
function testParamShadow(y: number) {
    // OK - parameter shadows outer y
    return y;
}

// Test 8: Catch clause variables can shadow outer declarations
const z = 300;
try {
    throw new Error();
} catch (z) {
    // OK - catch variable shadows outer z
    const error = z;
}

// Test 9: Class member names don't conflict with module-level
const Value = 400;
class MyClass {
    Value = 500; // OK - separate namespace
}
