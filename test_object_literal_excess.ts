// Test file for object literal excess property checking
// Understanding when TS2322 should be emitted vs TS2353

// ============================================================================
// Case 1: Weak type violations - should show TS2353, not TS2322
// ============================================================================

interface Weak1 {
    a?: number;
}

// Should show TS2353 (excess property 'b'), not TS2322
const w1: Weak1 = { a: 1, b: 2 };

interface Weak2 {
    a?: number;
    b?: number;
}

// Should show TS2353 (excess property 'c'), not TS2322
const w2: Weak2 = { a: 1, b: 2, c: 3 };

// ============================================================================
// Case 2: Union types with weak members - should show TS2353, not TS2322
// ============================================================================

type UnionWeak1 = { a?: number } | { b?: string };

// Should show TS2353 (excess property 'c'), not TS2322
const u1: UnionWeak1 = { a: 1, b: 2, c: 3 };

// ============================================================================
// Case 3: Fresh object literals with index signature - should accept excess
// ============================================================================

interface WithIndex {
    [key: string]: any;
    a?: number;
}

// Should NOT show any error - index signature accepts all properties
const idx1: WithIndex = { a: 1, b: 2, c: 3 };

interface WithIndexNumber {
    [key: number]: any;
    a?: number;
}

// Should NOT show any error - number index signature accepts properties
const idx2: WithIndexNumber = { a: 1, b: 2, c: 3 };

// ============================================================================
// Case 4: Non-fresh object literals - should not check excess properties
// ============================================================================

interface Strict {
    a: number;
}

const temp = { a: 1, b: 2, c: 3 };
// Should NOT show TS2353 for excess properties (non-fresh)
const s1: Strict = temp;

// But if the types don't match, should show TS2322
const temp2 = { x: 1 };
const s2: Strict = temp2; // Should show TS2322

// ============================================================================
// Case 5: Exact property mismatch - should show TS2322, not TS2353
// ============================================================================

interface Exact {
    a: string;
}

// Should show TS2322 (type mismatch on 'a'), not TS2353
const e1: Exact = { a: 1 }; // 'a' is number, not string

// ============================================================================
// Case 6: Missing required property - should show TS2741/TS2322, not TS2353
// ============================================================================

interface Required {
    a: number;
    b: string;
}

// Should show TS2741 (missing 'b') or TS2322, not TS2353
const r1: Required = { a: 1 };

// ============================================================================
// Case 7: Excess property AND type mismatch - which error takes priority?
// ============================================================================

interface Mixed {
    a: string;
}

// Has excess 'b' AND 'a' is wrong type
// TypeScript shows: TS2322 (type mismatch on 'a') primarily
const m1: Mixed = { a: 1, b: 2 };

// ============================================================================
// Case 8: Empty object type - accepts everything
// ============================================================================

interface Empty {}

// Should NOT show any errors
const empty1: Empty = { a: 1, b: 2, c: 3 };

// ============================================================================
// Case 9: Union with non-weak members
// ============================================================================

type UnionMixed = { a: number } | { b: string };

// Should show TS2322 (not assignable to union)
const um1: UnionMixed = { a: 1, b: 2 }; // 'b' is excess

// ============================================================================
// Case 10: Intersection types
// ============================================================================

type Intersection = { a: number } & { b: string };

// Should show TS2322/TS2741 for missing required properties
const i1: Intersection = { a: 1, b: "x", c: 3 }; // 'c' might be OK or not?

// ============================================================================
// Case 11: Optional properties in target
// ============================================================================

interface OptionalTarget {
    a?: number;
    b?: number;
}

// Should show TS2353 for 'c', not TS2322
const o1: OptionalTarget = { a: 1, b: 2, c: 3 };

// ============================================================================
// Case 12: Generic types
// ============================================================================

interface Generic<T> {
    value: T;
}

// Should show TS2322 (type mismatch)
const g1: Generic<string> = { value: 1 };

// Should show TS2353 (excess 'extra')
const g2: Generic<number> = { value: 1, extra: "x" };

console.log("Object literal excess property tests complete");
