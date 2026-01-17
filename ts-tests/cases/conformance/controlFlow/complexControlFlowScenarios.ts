/**
 * Complex Control Flow Scenarios Test File
 *
 * Tests complex control flow scenarios for CFA (Control Flow Analysis)
 * Covers nested try/catch/finally, loops with break/continue, switch fallthrough,
 * and conditional assignments across all branch paths.
 *
 * Expected errors:
 * - TS2454: Variable is used before being assigned
 * - TS2564: Property is not initialized
 */

// @strict: true
// @allowUnreachableCode: false

// =========================================================================
// SECTION 1: Nested Try/Catch/Finally Blocks
// =========================================================================

// Test 1: Deeply nested try-catch-finally - all paths assign
function nestedTryCatchFinallyAllPaths() {
    let x: number;
    try {
        try {
            try {
                x = 1;
            } finally {
                x = 2;
            }
        } catch (e1) {
            x = 3;
        }
    } catch (e2) {
        x = 4;
    }
    console.log(x); // OK - x is definitely assigned on all paths
}

// Test 2: Nested try-catch - inner catch assigns, outer doesn't
function nestedTryCatchInnerOnly() {
    let x: number;
    try {
        try {
            x = 1;
        } catch (e1) {
            x = 2;
        }
    } catch (e2) {
        // x not assigned here
    }
    console.log(x); // Expected: TS2454 - x may not be assigned in outer catch
}

// Test 3: Nested try-catch-finally - only finally assigns
function nestedTryCatchFinallyOnlyFinally() {
    let x: number;
    try {
        try {
            throw new Error();
        } catch (e) {
            // x not assigned
        } finally {
            x = 1;
        }
    } catch (e) {
        // x not assigned
    }
    console.log(x); // Expected: TS2454 - x may not be assigned if outer catch executes
}

// Test 4: Nested try-finally - nested return paths
function nestedTryFinallyWithReturns() {
    let x: number;
    try {
        try {
            x = 1;
            return;
        } finally {
            x = 2;
        }
    } finally {
        x = 3;
    }
}

// Test 5: Triple nested try blocks with complex control flow
function tripleNestedTryBlocks() {
    let x: number;
    let y: number;
    try {
        try {
            try {
                if (Math.random() > 0.5) {
                    x = 1;
                    y = 10;
                } else {
                    throw new Error();
                }
            } catch (e1) {
                x = 2;
            }
        } catch (e2) {
            x = 3;
            y = 20;
        }
    } catch (e3) {
        x = 4;
    }
    console.log(x); // OK - x is assigned on all paths
    console.log(y); // Expected: TS2454 - y not assigned on path to e2->x=3
}

// Test 6: Nested try-catch with re-throw
function nestedTryCatchWithRethrow() {
    let x: number;
    try {
        try {
            x = 1;
            throw new Error();
        } catch (e1) {
            x = 2;
            throw e1; // re-throw
        }
    } catch (e2) {
        x = 3;
    }
    console.log(x); // OK - x is definitely assigned
}

// Test 7: Multiple catch blocks with nested structure
function multipleNestedCatchBlocks() {
    let x: number;
    try {
        x = 1;
    } catch (e1) {
        try {
            x = 2;
        } catch (e2) {
            try {
                x = 3;
            } catch (e3) {
                // x not assigned
            }
        }
    }
    console.log(x); // Expected: TS2454 - x may not be assigned in innermost catch
}

// Test 8: Finally block with nested try-catch
function finallyWithNestedTryCatch() {
    let x: number;
    try {
        x = 1;
    } finally {
        try {
            x = 2;
        } catch (e) {
            x = 3;
        }
    }
    console.log(x); // OK - x is definitely assigned
}

// =========================================================================
// SECTION 2: Loops with Break/Continue and Definite Assignment
// =========================================================================

// Test 9: For loop with break - assignment before break
function forLoopWithBreakBeforeAssignment() {
    let x: number;
    for (let i = 0; i < 10; i++) {
        if (i === 5) break;
        x = i;
    }
    console.log(x); // Expected: TS2454 - loop may break before assignment
}

// Test 10: For loop with break - assignment guaranteed
function forLoopWithBreakAfterAssignment() {
    let x: number;
    for (let i = 0; i < 10; i++) {
        x = i;
        if (i === 5) break;
    }
    console.log(x); // OK - x is assigned before any break
}

// Test 11: While loop with continue
function whileLoopWithContinue() {
    let x: number;
    let i = 0;
    while (i < 10) {
        i++;
        if (i % 2 === 0) continue;
        x = i;
    }
    console.log(x); // Expected: TS2454 - continue may skip assignment
}

// Test 12: Nested loops with break to outer
function nestedLoopBreakOuter() {
    let x: number;
    outer: for (let i = 0; i < 10; i++) {
        for (let j = 0; j < 10; j++) {
            x = i + j;
            if (j === 5) break outer;
        }
    }
    console.log(x); // OK - x is assigned before break
}

// Test 13: Nested loops with break to inner - uncertain assignment
function nestedLoopBreakInner() {
    let x: number;
    for (let i = 0; i < 10; i++) {
        for (let j = 0; j < 10; j++) {
            if (j === 0) break;
            x = i + j;
        }
    }
    console.log(x); // Expected: TS2454 - inner loop may break before assignment
}

// Test 14: Do-while with break
function doWhileWithBreak() {
    let x: number;
    let i = 0;
    do {
        i++;
        if (i > 5) break;
        x = i;
    } while (i < 10);
    console.log(x); // Expected: TS2454 - break may occur before assignment
}

// Test 15: For-of loop with break
function forOfLoopWithBreak() {
    let x: number;
    for (let i of [1, 2, 3, 4, 5]) {
        if (i === 1) break;
        x = i;
    }
    console.log(x); // Expected: TS2454 - loop may break before assignment
}

// Test 16: For-in loop with continue
function forInLoopWithContinue() {
    let x: number;
    let obj = { a: 1, b: 2, c: 3 };
    for (let key in obj) {
        if (key === 'a') continue;
        x = obj[key as keyof typeof obj];
    }
    console.log(x); // Expected: TS2454 - continue may skip assignment
}

// Test 17: Multiple continue statements in loop
function multipleContinueInLoop() {
    let x: number;
    for (let i = 0; i < 10; i++) {
        if (i < 3) continue;
        if (i > 7) continue;
        x = i;
    }
    console.log(x); // Expected: TS2454 - continue may skip assignment
}

// Test 18: Loop with labeled break
function labeledBreakInLoop() {
    let x: number;
    myLabel: for (let i = 0; i < 10; i++) {
        if (i === 0) break myLabel;
        x = i;
    }
    console.log(x); // Expected: TS2454 - break occurs before assignment
}

// Test 19: Loop with conditional break and assignment
function conditionalBreakAndAssignment() {
    let x: number;
    for (let i = 0; i < 10; i++) {
        if (Math.random() > 0.5) {
            break;
        }
        x = i;
    }
    console.log(x); // Expected: TS2454 - break may occur before assignment
}

// Test 20: Nested loops with outer break and inner assignment
function nestedLoopsOuterBreakInnerAssignment() {
    let x: number;
    outer: for (let i = 0; i < 10; i++) {
        for (let j = 0; j < 10; j++) {
            if (i === 5) break outer;
            x = j;
        }
    }
    console.log(x); // Expected: TS2454 - outer break may occur before inner loop assigns
}

// =========================================================================
// SECTION 3: Switch Statement Fallthrough Tracking
// =========================================================================

// Test 21: Switch with fallthrough - all cases assign
function switchFallthroughAllAssign() {
    let x: number;
    switch (Math.floor(Math.random() * 3)) {
        case 0:
            x = 0;
            // fallthrough
        case 1:
            x = 1;
            break;
        case 2:
            x = 2;
            break;
    }
    console.log(x); // OK - x is assigned on all paths
}

// Test 22: Switch with fallthrough - some cases don't assign
function switchFallthroughMissingAssignment() {
    let x: number;
    switch (Math.floor(Math.random() * 3)) {
        case 0:
            // no assignment
            // fallthrough
        case 1:
            x = 1;
            break;
        case 2:
            x = 2;
            break;
    }
    console.log(x); // Expected: TS2454 - case 0 doesn't assign before fallthrough
}

// Test 23: Switch with default - all paths assign
function switchWithDefaultAllAssign() {
    let x: number;
    switch (Math.floor(Math.random() * 3)) {
        case 0:
            x = 0;
            break;
        case 1:
            x = 1;
            break;
        default:
            x = 2;
            break;
    }
    console.log(x); // OK - x is assigned on all paths including default
}

// Test 24: Switch without default - some cases don't assign
function switchWithoutDefaultMissingAssign() {
    let x: number;
    switch (Math.floor(Math.random() * 3)) {
        case 0:
            x = 0;
            break;
        case 1:
            x = 1;
            break;
        // case 2 not handled, no default
    }
    console.log(x); // Expected: TS2454 - case 2 doesn't assign
}

// Test 25: Nested switch statements
function nestedSwitchStatements() {
    let x: number;
    switch (Math.floor(Math.random() * 2)) {
        case 0:
            x = 0;
            break;
        case 1:
            switch (Math.floor(Math.random() * 2)) {
                case 0:
                    x = 1;
                    break;
                case 1:
                    // x not assigned
                    break;
            }
            break;
    }
    console.log(x); // Expected: TS2454 - inner switch case 1 doesn't assign
}

// Test 26: Switch with multiple fallthroughs
function switchMultipleFallthroughs() {
    let x: number;
    switch (Math.floor(Math.random() * 4)) {
        case 0:
            // no assignment
            // fallthrough
        case 1:
            x = 1;
            // fallthrough
        case 2:
            x = 2;
            break;
        case 3:
            x = 3;
            break;
    }
    console.log(x); // Expected: TS2454 - case 0 doesn't assign before fallthrough
}

// Test 27: Switch with break in nested block
function switchWithBreakInNestedBlock() {
    let x: number;
    switch (Math.floor(Math.random() * 2)) {
        case 0:
            {
                x = 0;
                break;
            }
        case 1:
            x = 1;
            break;
    }
    console.log(x); // OK - x is assigned on all paths
}

// Test 28: Switch with return in some cases
function switchWithReturnInCases() {
    let x: number;
    switch (Math.floor(Math.random() * 2)) {
        case 0:
            x = 0;
            return;
        case 1:
            x = 1;
            break;
    }
    console.log(x); // OK - case 0 returns, case 1 assigns
}

// Test 29: Switch with throw in some cases
function switchWithThrowInCases() {
    let x: number;
    switch (Math.floor(Math.random() * 2)) {
        case 0:
            throw new Error();
        case 1:
            x = 1;
            break;
    }
    console.log(x); // OK - case 0 throws, case 1 assigns
}

// Test 30: Switch with fallthrough to default
function switchFallthroughToDefault() {
    let x: number;
    switch (Math.floor(Math.random() * 2)) {
        case 0:
            // no assignment
            // fallthrough
        default:
            x = 1;
            break;
    }
    console.log(x); // OK - default assigns
}

// =========================================================================
// SECTION 4: Conditional Assignments with All Branch Paths
// =========================================================================

// Test 31: Deeply nested if-else - all paths assign
function deeplyNestedIfElseAllPaths() {
    let x: number;
    if (Math.random() > 0.5) {
        if (Math.random() > 0.5) {
            if (Math.random() > 0.5) {
                x = 1;
            } else {
                x = 2;
            }
        } else {
            x = 3;
        }
    } else {
        x = 4;
    }
    console.log(x); // OK - x is assigned on all paths
}

// Test 32: Deeply nested if-else - missing assignment in one path
function deeplyNestedIfElseMissingAssignment() {
    let x: number;
    if (Math.random() > 0.5) {
        if (Math.random() > 0.5) {
            x = 1;
        } else {
            // x not assigned
        }
    } else {
        x = 2;
    }
    console.log(x); // Expected: TS2454 - inner else doesn't assign
}

// Test 33: Ternary operator - both branches assign
function ternaryBothBranchesAssign() {
    let x: number;
    x = Math.random() > 0.5 ? 1 : 2;
    console.log(x); // OK - x is assigned
}

// Test 34: Nested ternary operators - all paths assign
function nestedTernaryAllPaths() {
    let x: number;
    x = Math.random() > 0.5
        ? (Math.random() > 0.5 ? 1 : 2)
        : (Math.random() > 0.5 ? 3 : 4);
    console.log(x); // OK - x is assigned on all paths
}

// Test 35: Logical AND short-circuit
function logicalAndShortCircuit() {
    let x: number;
    if (Math.random() > 0.5 && (x = 1) > 0) {
        console.log(x); // OK - x is assigned if condition is true
    }
    console.log(x); // Expected: TS2454 - x may not be assigned if first condition is false
}

// Test 36: Logical OR short-circuit
function logicalOrShortCircuit() {
    let x: number;
    if (Math.random() > 0.5 || (x = 1) > 0) {
        console.log(x); // Expected: TS2454 - x may not be assigned if first condition is true
    }
}

// Test 37: Nullish coalescing with assignment
function nullishCoalescingWithAssignment() {
    let x: number;
    let y: number | undefined = undefined;
    x = y ?? (x = 1);
    console.log(x); // Expected: TS2454 - x is read before being assigned in nullish coalescing
}

// Test 38: Multiple variables with conditional assignments
function multipleVariablesConditional() {
    let x: number;
    let y: number;
    let z: number;

    if (Math.random() > 0.5) {
        x = 1;
        y = 10;
    } else {
        x = 2;
        z = 30;
    }
    console.log(x); // OK - x is assigned on both branches
    console.log(y); // Expected: TS2454 - y not assigned on else branch
    console.log(z); // Expected: TS2454 - z not assigned on if branch
}

// Test 39: Conditional with early return
function conditionalWithEarlyReturn() {
    let x: number;
    if (Math.random() > 0.5) {
        x = 1;
        return;
    }
    console.log(x); // Expected: TS2454 - x not assigned if we reach here
}

// Test 40: Conditional with throw
function conditionalWithThrow() {
    let x: number;
    if (Math.random() > 0.5) {
        throw new Error();
    }
    x = 1;
    console.log(x); // OK - x is assigned after throw path
}

// Test 41: Complex boolean expression with assignment in condition
function complexBooleanWithAssignment() {
    let x: number;
    let y: number;
    if ((x = 1) > 0 && (y = 2) > 0) {
        console.log(x); // OK
        console.log(y); // OK
    }
    console.log(x); // Expected: TS2454 - x may not be assigned if second part of && fails
    console.log(y); // Expected: TS2454 - y may not be assigned if first part fails
}

// Test 42: While loop with assignment in condition
function whileLoopAssignmentInCondition() {
    let x: number;
    let i = 0;
    while ((x = i++) < 10) {
        console.log(x); // OK - x is assigned in condition
    }
    console.log(x); // OK - x was assigned in last iteration
}

// Test 43: For loop with assignment in condition
function forLoopAssignmentInCondition() {
    let x: number;
    let i = 0;
    for (; (x = i) < 10; i++) {
        console.log(x); // OK - x is assigned in condition
    }
    console.log(x); // OK - x was assigned in last iteration
}

// Test 44: Switch with complex expressions
function switchWithComplexExpressions() {
    let x: number;
    let y: number;
    switch (x = Math.floor(Math.random() * 2)) {
        case 0:
            y = 10;
            break;
        case 1:
            y = 20;
            break;
    }
    console.log(x); // OK - x is assigned in switch expression
    console.log(y); // Expected: TS2454 - y may not be assigned if new case added
}

// Test 45: Try-catch with conditional throw
function tryCatchWithConditionalThrow() {
    let x: number;
    try {
        if (Math.random() > 0.5) {
            throw new Error();
        }
        x = 1;
    } catch (e) {
        x = 2;
    }
    console.log(x); // OK - x is assigned in both try and catch
}

// Test 46: Nested conditionals with mixed control flow
function nestedConditionalsMixedControlFlow() {
    let x: number;
    let y: number;

    if (Math.random() > 0.5) {
        x = 1;
        if (Math.random() > 0.5) {
            y = 10;
            return;
        }
    } else {
        y = 20;
    }
    console.log(x); // Expected: TS2454 - x may not be assigned if we take else branch
    console.log(y); // Expected: TS2454 - y may not be assigned if we take if branch and don't return
}

// Test 47: Multiple assignments with reassignment
function multipleReassignments() {
    let x: number;
    if (Math.random() > 0.5) {
        x = 1;
    } else {
        x = 2;
    }
    x = 3; // Reassignment
    console.log(x); // OK - x is definitely assigned
}

// Test 48: Array method with callback - assignment in callback
function arrayMethodCallbackAssignment() {
    let x: number;
    [1, 2, 3].forEach((val) => {
        if (val === 2) x = val;
    });
    console.log(x); // Expected: TS2454 - x may not be assigned if callback never sets it
}

// Test 49: Promise chain with assignment
function promiseChainWithAssignment() {
    let x: number;
    Promise.resolve()
        .then(() => {
            x = 1;
        })
        .catch(() => {
            x = 2;
        });
    console.log(x); // Expected: TS2454 - x is not assigned synchronously
}

// Test 50: Conditional with nested loops
function conditionalWithNestedLoops() {
    let x: number;
    if (Math.random() > 0.5) {
        for (let i = 0; i < 10; i++) {
            x = i;
            break;
        }
    } else {
        for (let i = 0; i < 10; i++) {
            x = i;
            break;
        }
    }
    console.log(x); // OK - x is assigned in both branches
}
