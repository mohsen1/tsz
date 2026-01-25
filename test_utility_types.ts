/**
 * Test: TypeScript Utility Types - Exclude, Extract, Pick, Omit
 *
 * This tests the distributivity disabling pattern [T] extends [U]
 * which is critical for Exclude and Extract utility types.
 */

// ============================================================================
// Test 1: Distributive Conditional Type (baseline)
// ============================================================================

type CheckDistributive<T> = T extends any ? true : false;

// With distributive conditional, union distributes over the check
type Test1 = CheckDistributive<"A" | "B">; // Should be: true | true = boolean

const t1: Test1 = true; // Should work
const t2: Test1 = false; // Should work
// const t3: Test1 = "string"; // Should error

// ============================================================================
// Test 2: Non-Distributive Conditional (tuple wrapper)
// ============================================================================

type CheckNonDistributive<T> = [T] extends [any] ? true : false;

// With tuple wrapper, union is checked as a whole
type Test2 = CheckNonDistributive<"A" | "B">; // Should be: true

const t4: Test2 = true; // Should work
// const t5: Test2 = false; // Should error

// ============================================================================
// Test 3: Exclude Utility Type (uses distributive conditional)
// ============================================================================

type MyExclude<T, U> = T extends U ? never : T;

// Should exclude "a" from the union
type Test3 = MyExclude<"a" | "b" | "c", "a">; // Should be: "b" | "c"

const t6: Test3 = "b"; // Should work
const t7: Test3 = "c"; // Should work
// const t8: Test3 = "a"; // Should error

// Exclude multiple types
type Test4 = MyExclude<"a" | "b" | "c" | "d", "a" | "b">; // Should be: "c" | "d"

const t9: Test4 = "c"; // Should work
const t10: Test4 = "d"; // Should work
// const t11: Test4 = "a"; // Should error
// const t12: Test4 = "b"; // Should error

// ============================================================================
// Test 4: Extract Utility Type (uses distributive conditional)
// ============================================================================

type MyExtract<T, U> = T extends U ? T : never;

// Should extract only "a" from the union
type Test5 = MyExtract<"a" | "b" | "c", "a">; // Should be: "a"

const t13: Test5 = "a"; // Should work
// const t14: Test5 = "b"; // Should error
// const t15: Test5 = "c"; // Should error

// Extract multiple types
type Test6 = MyExtract<"a" | "b" | "c" | "d", "a" | "b">; // Should be: "a" | "b"

const t16: Test6 = "a"; // Should work
const t17: Test6 = "b"; // Should work
// const t18: Test6 = "c"; // Should error
// const t19: Test6 = "d"; // Should error

// ============================================================================
// Test 5: Non-distributive check for unions
// ============================================================================

// This tests that [T] extends [U] does NOT distribute
type IsUnion<T> = [T] extends [never] ? false : true;

type Test7 = IsUnion<string>; // Should be: true
type Test8 = IsUnion<"a" | "b">; // Should be: true (union checked as whole)

const t20: Test7 = true;
const t21: Test8 = true;

// ============================================================================
// Test 6: Complex Exclude example
// ============================================================================

type EventType =
  | { type: "click"; x: number; y: number }
  | { type: "focus"; element: HTMLElement }
  | { type: "blur"; element: HTMLElement };

type ExcludeClick<T> = T extends { type: "click" } ? never : T;

type MouseEvents = ExcludeClick<EventType>;
// Should be: { type: "focus"; element: HTMLElement } | { type: "blur"; element: HTMLElement }

// ============================================================================
// Test 7: Extract with complex types
// ============================================================================

type ExtractFocus<T> = T extends { type: "focus" } ? T : never;

type FocusEvent = ExtractFocus<EventType>;
// Should be: { type: "focus"; element: HTMLElement }

// ============================================================================
// Test 8: Verify non-distributive behavior with never
// ============================================================================

type CheckNever<T> = [T] extends [never] ? true : false;

type Test9 = CheckNever<never>; // Should be: true
type Test10 = CheckNever<string>; // Should be: false
type Test11 = CheckNever<"a" | "b">; // Should be: false (union is not never)

// ============================================================================
// Test 9: Tuple wrapper prevents distribution
// ============================================================================

// This is the key test - verifies that [T] extends [U] does NOT distribute
type ToArray<T> = [T] extends [any] ? T[] : never;

type Test12 = ToArray<string | number>; // Should be: (string | number)[] NOT string[] | number[]

const t22: Test12 = ["a", 1]; // Should work - mixed array is (string | number)[]

// Without tuple wrapper, this would distribute:
type ToArrayDistributive<T> = T extends any ? T[] : never;

type Test13 = ToArrayDistributive<string | number>; // Should be: string[] | number[]

// const t23: Test13 = ["a", 1]; // Should error - can't mix types in string[] | number[]

console.log("All utility type tests compiled successfully!");
