/**
 * FlowGraph Test Cases
 *
 * These test cases demonstrate the FlowGraph structure's ability to track
 * variable assignments through control flow for definite assignment analysis.
 *
 * These cases can be compiled and the FlowGraph queried to verify correct
 * tracking of variable states.
 */

// Test 1: Basic variable usage before assignment (should produce TS2454)
function testBasicUseBeforeDefiniteAssignment() {
    let x: number;
    console.log(x); // Expected: TS2454 - Variable 'x' is used before being assigned
    x = 1;
    console.log(x); // OK - x is definitely assigned here
}

// Test 2: Variable with initializer (should be OK)
function testVariableWithInitializer() {
    let x: number = 1;
    console.log(x); // OK - x has an initializer
}

// Test 3: Conditional assignment - both branches assign
function testConditionalAssignmentBothBranches() {
    let x: number;
    if (Math.random() > 0.5) {
        x = 1;
    } else {
        x = 2;
    }
    console.log(x); // OK - x is definitely assigned in both branches
}

// Test 4: Conditional assignment - only one branch assigns (should error)
function testConditionalAssignmentOneBranch() {
    let x: number;
    if (Math.random() > 0.5) {
        x = 1;
    }
    console.log(x); // Expected: TS2454 - x may not be assigned
}

// Test 5: Loop with assignment in body
function testLoopWithAssignment() {
    let x: number;
    for (let i = 0; i < 10; i++) {
        x = i;
    }
    console.log(x); // OK - x is definitely assigned after loop (loop always executes)
}

// Test 6: For-of loop assigns variable
function testForOfLoop() {
    for (let x of [1, 2, 3]) {
        console.log(x); // OK - x is assigned by for-of
    }
}

// Test 7: For-in loop assigns variable
function testForInLoop() {
    for (let x in { a: 1, b: 2 }) {
        console.log(x); // OK - x is assigned by for-in
    }
}

// Test 8: While loop with assignment
function testWhileLoop() {
    let x: number;
    let condition = true;
    while (condition) {
        x = 1;
        condition = false;
    }
    console.log(x); // OK - x is definitely assigned after loop (if loop executes)
}

// Test 9: Try-catch with assignment in both try and catch
function testTryCatchBothAssign() {
    let x: number;
    try {
        x = 1;
    } catch {
        x = 2;
    }
    console.log(x); // OK - x is assigned in both try and catch
}

// Test 10: Try-catch with assignment only in try (should error if catch can execute)
function testTryCatchOnlyTryAssign() {
    let x: number;
    try {
        x = 1;
        throw new Error("test");
    } catch {
        // x not assigned here
    }
    console.log(x); // Expected: TS2454 - x may not be assigned if catch executes
}

// Test 11: Nested conditions
function testNestedConditions() {
    let x: number;
    if (Math.random() > 0.5) {
        if (Math.random() > 0.5) {
            x = 1;
        }
    }
    console.log(x); // Expected: TS2454 - x may not be assigned
}

// Test 12: Nested conditions - all paths assign
function testNestedConditionsAllPaths() {
    let x: number;
    if (Math.random() > 0.5) {
        x = 1;
    } else {
        if (Math.random() > 0.5) {
            x = 2;
        } else {
            x = 3;
        }
    }
    console.log(x); // OK - x is definitely assigned on all paths
}

// Test 13: Switch statement - all cases assign
function testSwitchAllCasesAssign() {
    let x: number;
    switch (Math.random() > 0.5 ? 1 : 2) {
        case 1:
            x = 1;
            break;
        case 2:
            x = 2;
            break;
    }
    console.log(x); // OK - x is assigned in all cases
}

// Test 14: Switch statement - not all cases assign (should error)
function testSwitchNotAllCasesAssign() {
    let x: number;
    switch (Math.floor(Math.random() * 3)) {
        case 0:
            x = 0;
            break;
        case 1:
            x = 1;
            break;
        case 2:
            // x not assigned
            break;
    }
    console.log(x); // Expected: TS2454 - x may not be assigned in case 2
}

// Test 15: Assignment in expression statement
function testAssignmentInExpression() {
    let x: number;
    let y: number;
    y = (x = 1) + 1;
    console.log(x); // OK - x is assigned via expression
    console.log(y); // OK - y is assigned via expression
}

// Test 16: Compound assignment operators
function testCompoundAssignment() {
    let x: number = 0;
    x += 1;
    console.log(x); // OK - x is initialized and compound assignment modifies it
}

// Test 17: Multiple variables
function testMultipleVariables() {
    let x: number;
    let y: number;
    let z: number;

    x = 1;
    z = 3;
    console.log(x); // OK
    console.log(y); // Expected: TS2454 - y is not assigned
    console.log(z); // OK
}

// Test 18: Variable shadowing in block scope
function testBlockScopeShadowing() {
    let x: number;
    {
        let x: number = 1; // Different x
        console.log(x); // OK - refers to inner x
    }
    console.log(x); // Expected: TS2454 - outer x is not assigned
}

// Test 19: Do-while loop
function testDoWhileLoop() {
    let x: number;
    let i = 0;
    do {
        x = i;
        i++;
    } while (i < 10);
    console.log(x); // OK - do-while always executes at least once
}

// Test 20: Early return
function testEarlyReturn() {
    let x: number;
    if (Math.random() > 0.5) {
        x = 1;
        return;
    }
    console.log(x); // Expected: TS2454 - x may not be assigned if we reach here
}

// Test 21: Definite assignment assertion
function testDefiniteAssignmentAssertion() {
    let x!: number; // Definite assignment assertion
    console.log(x); // OK - assertion tells compiler x is assigned elsewhere
}

// Test 22: Function parameter
function testFunctionParameter(x: number) {
    console.log(x); // OK - parameters are definitely assigned
}

// Test 23: Array destructuring
function testArrayDestructuring() {
    let [x, y] = [1, 2];
    console.log(x); // OK - x is assigned via destructuring
    console.log(y); // OK - y is assigned via destructuring
}

// Test 24: Array destructuring with partial assignment
function testArrayDestructuringPartial() {
    let [x, y] = [1]; // Only one value
    console.log(x); // OK - x is assigned
    console.log(y); // Expected: TS2454 - y is undefined
}

// Test 25: Object destructuring
function testObjectDestructuring() {
    let { x, y } = { x: 1, y: 2 };
    console.log(x); // OK - x is assigned via destructuring
    console.log(y); // OK - y is assigned via destructuring
}
