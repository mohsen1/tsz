# LSP Type Checker Gap Test Cases

## Test Cases for Verifying Fixes

This document provides concrete test cases to verify each type checker gap fix.

---

## 1. Control Flow Narrowing Tests

### Test 1.1: Typeof Narrowing in Hover

**File**: `test-narrowing-hover.ts`

```typescript
function testTypeofNarrowing(value: string | number) {
    if (typeof value === "string") {
        // HOVER OVER 'value' HERE
        // Expected type: string
        // Current type: string | number ❌
        console.log(value.toUpperCase());
    }
}
```

**Verification Steps**:
1. Open file in VS Code with TSZ LSP
2. Hover over `value` inside the `if` block
3. Expected hover: `(variable) value: string`
4. Current hover: `(variable) value: string | number`

---

### Test 1.2: Discriminant Narrowing in Hover

**File**: `test-discriminant-hover.ts`

```typescript
type Shape =
    | { kind: "circle", radius: number }
    | { kind: "square", side: number };

function area(shape: Shape) {
    if (shape.kind === "circle") {
        // HOVER OVER 'shape' HERE
        // Expected type: { kind: "circle", radius: number }
        // Current type: Shape ❌
        console.log(shape.radius);
    }
}
```

**Verification Steps**:
1. Hover over `shape` inside the `if` block
2. Expected: Type narrowed to circle variant
3. Current: Still shows union Shape

---

### Test 1.3: Nullish Narrowing in Completions

**File**: `test-nullish-completions.ts`

```typescript
function testNullishCompletions(obj: { a: string } | null) {
    if (obj !== null) {
        // TYPE 'obj.' HERE
        // Expected completions: a
        // Current completions: a (but might show null properties) ❌
        console.log(obj.);
    }
}
```

**Verification Steps**:
1. Trigger completions after `obj.`
2. Expected: Only `a` appears
3. Current: `a` appears, but type might not be narrowed

---

### Test 1.4: Assignment Narrowing

**File**: `test-assignment-narrowing.ts`

```typescript
function testAssignmentNarrowing(x: string | number) {
    x = "hello";
    // HOVER OVER 'x' HERE
    // Expected type: string (narrowed by assignment)
    // Current type: string | number ❌
    console.log(x.toUpperCase());
}
```

**Verification Steps**:
1. Hover over `x` after assignment
2. Expected: Type narrowed to `string`
3. Current: Still shows union type

---

## 2. Definite Assignment Tests

### Test 2.1: Simple Definite Assignment Error

**File**: `test-definite-assignment.ts`

```typescript
function testSimpleUnassigned() {
    let x: string;
    // SHOULD ERROR HERE: TS2454 - Variable 'x' is used before being assigned
    // Current: No error ❌
    console.log(x);
}
```

**Verification Steps**:
1. Check diagnostics for line 4
2. Expected: Error TS2454 - Variable 'x' is used before being assigned
3. Current: No error reported

---

### Test 2.2: Conditional Definite Assignment

**File**: `test-conditional-assignment.ts`

```typescript
function testConditionalAssignment(flag: boolean) {
    let x: string;
    if (flag) {
        x = "yes";
    }
    // SHOULD ERROR HERE: Not definitely assigned on all paths
    // Current: No error ❌
    console.log(x);
}
```

**Verification Steps**:
1. Check diagnostics for line 8
2. Expected: Error TS2454 - 'x' is not definitely assigned
3. Current: No error reported

---

### Test 2.3: Valid Definite Assignment

**File**: `test-valid-assignment.ts`

```typescript
function testValidAssignment() {
    let x: string;
    x = "assigned";
    // SHOULD NOT ERROR: Definitely assigned before use
    console.log(x); // ✅ OK
}
```

**Verification Steps**:
1. Verify no diagnostic on line 5
2. Should pass without errors

---

### Test 2.4: Code Action Quick Fix

**File**: `test-code-action-quickfix.ts`

```typescript
function testCodeAction() {
    let x: string;
    console.log(x);
    // RUN CODE ACTION: "Initialize variable"
    // Expected: Inserts 'x = "";' or similar
}
```

**Verification Steps**:
1. Trigger code actions at error location
2. Check for "Initialize variable" quick fix
3. Apply quick fix and verify result compiles

---

## 3. TDZ Checking Tests

### Test 3.1: Static Block TDZ

**File**: `test-static-block-tdz.ts`

```typescript
class MyClass {
    static {
        // SHOULD ERROR HERE: Cannot access 'x' before initialization
        // Current: No error ❌
        console.log(x);

        let x = 42;
    }
}
```

**Verification Steps**:
1. Check diagnostics for line 5
2. Expected: TDZ violation error
3. Current: No error

---

### Test 3.2: TDZ in Completions

**File**: `test-tdz-completions.ts`

```typescript
function testTDZCompletions() {
    // TYPE HERE - BEFORE 'x' DECLARATION
    // Expected: 'x' should NOT appear in completions
    // Current: 'x' appears in completions ❌
    console.log();

    let x = 42;
}
```

**Verification Steps**:
1. Trigger completions before `let x` declaration
2. Expected: `x` NOT in completion list
3. Current: `x` incorrectly appears

---

### Test 3.3: Computed Property TDZ

**File**: `test-computed-property-tdz.ts`

```typescript
class Test {
    [x]: number;  // Error: 'x' used in TDZ

    // SHOULD ERROR HERE: Cannot access 'x' in computed property before declaration
    // Current: No error ❌
    static x = "value";
}
```

**Verification Steps**:
1. Check diagnostics for line 2
2. Expected: TDZ violation in computed property
3. Current: No error

---

### Test 3.4: Heritage Clause TDZ

**File**: `test-heritage-tdz.ts`

```typescript
class Base {
    static x = "base";
}

// SHOULD ERROR HERE: 'Derived' used before declaration in heritage clause
// Current: No error ❌
class Derived extends Base {
    static y = Derived.x;
}
```

**Verification Steps**:
1. Check diagnostics for line 8
2. Expected: TDZ violation for `Derived` in extends clause
3. Current: No error

---

## 4. Module Resolution Tests

### Test 4.1: Simple Import Resolution

**Files**: `file1.ts`, `file2.ts`

```typescript
// file1.ts
export function exportedFunction() {
    return "hello";
}

export const exportedConst = 42;

// file2.ts
import { exportedFunction } from './file1';

// TYPE HERE AFTER 'exportedFunction.'
// Expected: Completions work
// Current: Might fail if module resolution broken ❌
exportedFunction.;
```

**Verification Steps**:
1. Open `file2.ts` in editor
2. Trigger completions after `exportedFunction.`
3. Expected: Method completions appear
4. Current: May show nothing if resolution fails

---

### Test 4.2: Re-export Chain

**Files**: `a.ts`, `b.ts`, `c.ts`

```typescript
// a.ts
export function foo() { }

// b.ts
export * from './a';

// c.ts
export * from './b';

// d.ts
import { foo } from './c';
// SHOULD RESOLVE: foo is available through re-export chain
// Current: Might fail ❌
foo();
```

**Verification Steps**:
1. Open `d.ts` in editor
2. Hover over `foo`
3. Expected: Shows type information
4. Current: May show "not found" error

---

### Test 4.3: Go-to-Definition Cross-File

**Files**: `definitions.ts`, `usage.ts`

```typescript
// definitions.ts
export function myFunction(param: string): number {
    return param.length;
}

// usage.ts
import { myFunction } from './definitions';

myFunction("test");
// RIGHT-CLICK 'myFunction' -> "Go to Definition"
// Expected: Opens definitions.ts at myFunction declaration
// Current: Might fail ❌
```

**Verification Steps**:
1. Open `usage.ts`
2. Right-click on `myFunction`
3. Select "Go to Definition"
4. Expected: Opens `definitions.ts` at correct line
5. Current: Navigation may fail

---

## 5. Intersection Type Tests

### Test 5.1: Intersection Reduction (Should Work)

**File**: `test-intersection-reduction.ts`

```typescript
type TypeA = { kind: "a", valueA: string };
type TypeB = { kind: "b", valueB: number };

type Impossible = TypeA & TypeB;
// SHOULD REDUCE TO: never
// This should already work ✅

const x: Impossible = "any value";  // Should accept anything (never type)
```

**Verification Steps**:
1. Hover over `Impossible` type
2. Expected: Shows `never`
3. Current: Should already work (intersection reduction is complete)

---

### Test 5.2: Intersection Property Access

**File**: `test-intersection-properties.ts`

```typescript
type TypeA = { a: string };
type TypeB = { b: number };

type Both = TypeA & TypeB;

const obj: Both = { a: "hello", b: 42 };

// TYPE HERE: 'obj.'
// Expected: Completions show both 'a' and 'b'
// Current: Should already work ✅
obj.;
```

**Verification Steps**:
1. Trigger completions after `obj.`
2. Expected: Both `a` and `b` appear
3. Current: Should already work

---

## 6. Signature Help Tests

### Test 6.1: Basic Signature Help

**File**: `test-signature-help.ts`

```typescript
function add(a: number, b: number): number {
    return a + b;
}

// OPEN PAREN HERE: add(|)
// Expected: Shows signature "(a: number, b: number): number"
// Current: Should already work ✅
add(|);
```

**Verification Steps**:
1. Type `add(` and trigger signature help
2. Expected: Shows function signature
3. Current: Should already work

---

### Test 6.2: Overloaded Signatures

**File**: `test-overloads.ts`

```typescript
function overload(x: string): string;
function overload(x: number): number;
function overload(x: string | number): string | number {
    return x;
}

// TYPE HERE: overload(|)
// Expected: Shows both overload signatures
// Current: Should already work ✅
overload(|);
```

**Verification Steps**:
1. Type `overload(` and trigger signature help
2. Expected: Shows both `(x: string): string` and `(x: number): number`
3. Current: Should already work

---

## Test Automation Script

**File**: `run-lsp-gap-tests.sh`

```bash
#!/bin/bash

# Test script to verify LSP gap fixes

echo "=== LSP Type Checker Gap Tests ==="
echo ""

# Test 1: Control Flow Narrowing
echo "Test 1: Control Flow Narrowing"
echo "  - Open test-narrowing-hover.ts in VS Code"
echo "  - Hover over 'value' inside if block"
echo "  - Expected: type shows 'string', not 'string | number'"
echo ""

# Test 2: Definite Assignment
echo "Test 2: Definite Assignment"
echo "  - Open test-definite-assignment.ts"
echo "  - Check for TS2454 error on console.log(x)"
echo "  - Expected: Red squiggly underline"
echo ""

# Test 3: TDZ Checking
echo "Test 3: TDZ Checking"
echo "  - Open test-static-block-tdz.ts"
echo "  - Check for TDZ violation error"
echo "  - Expected: Error before 'let x' declaration"
echo ""

# Test 4: Module Resolution
echo "Test 4: Module Resolution"
echo "  - Open file2.ts (imports from file1.ts)"
echo "  - Trigger completions after exportedFunction."
echo "  - Expected: Shows method completions"
echo ""

echo "=== Manual Verification Required ==="
echo "These tests require manual verification in VS Code with TSZ LSP server"
```

---

## Conformance Test Commands

```bash
# Run conformance tests to see baseline
cd /Users/mohsenazimi/code/tsz
./conformance/run.sh --server > baseline-results.txt

# After implementing fixes, run again
./conformance/run.sh --server > after-fixes.txt

# Compare results
diff baseline-results.txt after-fixes.txt

# Look for improvements in:
# - TS2322 (Type not assignable) - should decrease with narrowing
# - TS2571 (Object is 'unknown') - should decrease with narrowing
# - TS2454 (Variable used before assignment) - should appear after fix
```

---

## Expected Results Timeline

### Before Any Fixes (Baseline)
```
Conformance Pass Rate: XX%

Hover Tests:
  - Typeof narrowing: ❌ FAIL
  - Discriminant narrowing: ❌ FAIL
  - Assignment narrowing: ❌ FAIL

Definite Assignment:
  - TS2454 detection: ❌ FAIL (no errors)
  - Code actions: ❌ NOT AVAILABLE

TDZ Tests:
  - Static block: ❌ FAIL
  - Completions filtering: ❌ FAIL
  - Computed properties: ❌ FAIL

Module Resolution:
  - Cross-file completions: ❌ FAIL
  - Go-to-definition: ❌ FAIL
```

### After Phase 1 (Week 2) - Flow Narrowing API
```
Conformance Pass Rate: XX% + 2-3%

Hover Tests:
  - Typeof narrowing: ✅ PASS
  - Discriminant narrowing: ✅ PASS
  - Assignment narrowing: ✅ PASS

Completions:
  - Narrowed context: ✅ PASS
```

### After Phase 2 (Week 3) - Definite Assignment
```
Conformance Pass Rate: XX% + 4-5%

Definite Assignment:
  - TS2454 detection: ✅ PASS
  - Code actions: ✅ AVAILABLE
  - Conditional paths: ✅ PASS
```

### After Phase 3 (Week 5) - TDZ Checking
```
Conformance Pass Rate: XX% + 5-6%

TDZ Tests:
  - Static block: ✅ PASS
  - Completions filtering: ✅ PASS
  - Computed properties: ✅ PASS
  - Heritage clauses: ✅ PASS
```

### After Phase 4 (Week 6) - Module Resolution
```
Conformance Pass Rate: XX% + 7-9%

Module Resolution:
  - Cross-file completions: ✅ PASS
  - Go-to-definition: ✅ PASS
  - Re-export chains: ✅ PASS
```

---

## Quick Test Commands

```bash
# Run LSP-specific tests
cargo test --package tsz --lib lsp::hover
cargo test --package tsz --lib lsp::completions
cargo test --package tsz --lib lsp::signature_help

# Run flow analysis tests
cargo test --package tsz --lib checker::flow_analysis

# Run conformance
./conformance/run.sh --server

# Start LSP server for manual testing
cargo run --bin tsz-server
```

---

**Test Case Status**: Ready for implementation and verification
**Last Updated**: 2026-01-30
**Next Update**: After Phase 1 completion
