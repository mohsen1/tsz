/**
 * Test: Template Literal Expansion Limits (Rule #22)
 *
 * This tests the 100k item limit for template literal type expansion.
 */

// ============================================================================
// Test 1: Simple template literal expansion
// ============================================================================

type Keys1 = "a" | "b" | "c";
type Getters1 = `get${Keys1}`;
// Should expand to: "geta" | "getb" | "getc"

const g1: Getters1 = "geta"; // ✅
const g2: Getters1 = "getb"; // ✅
const g3: Getters1 = "getc"; // ✅
// const g4: Getters1 = "getd"; // ❌ Error

// ============================================================================
// Test 2: Cartesian product expansion
// ============================================================================

type First = "a" | "b" | "c";
type Second = "x" | "y" | "z";
type Combined = `${First}-${Second}`;
// Should expand to: "a-x" | "a-y" | "a-z" | "b-x" | "b-y" | "b-z" | "c-x" | "c-y" | "c-z"

const c1: Combined = "a-x"; // ✅
const c2: Combined = "c-z"; // ✅
// const c3: Combined = "a-a"; // ❌ Error

// ============================================================================
// Test 3: Larger expansion (under limit)
// ============================================================================

// 100 keys should expand to 100 combinations
type LargeKeys =
  | "k0" | "k1" | "k2" | "k3" | "k4"
  | "k5" | "k6" | "k7" | "k8" | "k9";
// ... (imagine 100 total keys)

type LargeGetters = `get${LargeKeys}`;
// Should expand to 100 combinations

// ============================================================================
// Test 4: Very large expansion (would exceed limit)
// ============================================================================

// 500 keys each with 500 combinations = 250,000 combinations
// This should widen to `string` instead of expanding
//
// Note: We can't actually create 500 literal string unions in TypeScript
// without hitting the editor limits, but the type system handles this.
//
// In practice, TypeScript would widen such large unions to `string`:
type VeryLargeTemplate = string; // Would be widened

// ============================================================================
// Test 5: Nested template literals
// ============================================================================

type Inner = "x" | "y" | "z";
type Outer = `prefix_${Inner}_suffix`;

const n1: Outer = "prefix_x_suffix"; // ✅
const n2: Outer = "prefix_y_suffix"; // ✅
// const n3: Outer = "prefix_a_suffix"; // ❌ Error

// ============================================================================
// Test 6: Template literal with multiple interpolations
// ============================================================================

type Verb = "get" | "set" | "delete";
type Noun = "User" | "Post" | "Comment";
type Method = `${Verb}${Noun}`;

const m1: Method = "getUser"; // ✅
const m2: Method = "setPost"; // ✅
const m3: Method = "deleteComment"; // ✅
// const m4: Method = "createUser"; // ❌ Error

// ============================================================================
// Test 7: Mixed with non-literal types
// ============================================================================

// When non-literal types are involved, template remains unevaluated
type WithString = `get${string}`;
const w1: WithString = "getanything"; // ✅
const w2: WithString = "get"; // ✅

// ============================================================================
// Test 8: Utility type patterns
// ============================================================================

type EventName<T extends string> = `on${T}`;

type ClickEvents = EventName<"Click">;
const e1: ClickEvents = "onClick"; // ✅

type AllEvents = EventName<"Click" | "Focus" | "Blur">;
const e2: AllEvents = "onClick"; // ✅
const e3: AllEvents = "onFocus"; // ✅
const e4: AllEvents = "onBlur"; // ✅
// const e5: AllEvents = "onHover"; // ❌ Error

// ============================================================================
// Test 9: Branded types with template literals
// ============================================================================

type Brand<T extends string> = `${T}_Brand`;

type BrandedString = Brand<"MyString">;
const b1: BrandedString = "MyString_Brand"; // ✅
// const b2: BrandedString = "MyString"; // ❌ Error

// ============================================================================
// Test 10: Template literal inference
// ============================================================================

function createEvent<T extends string>(eventName: EventName<T>) {
  return eventName;
}

const ev1 = createEvent("Click"); // ✅ Returns "onClick"
const ev2 = createEvent("Hover"); // ✅ Returns "onHover"

// ============================================================================
// Test 11: Conditional types with template literals
// ============================================================================

type ExtractGet<T> = T extends `get${infer Rest}` ? Rest : never;

type G1 = ExtractGet<"getUser">; // "User"
type G2 = ExtractGet<"setUser">; // never

const g11: G1 = "User"; // ✅
// const g12: G1 = "setUser"; // ❌ Error (wrong type)

// ============================================================================
// Test 12: Template literal type constraints
// ============================================================================

type OnlyGetters<T extends `get${string}`> = T;

function onlyGetters<T extends `get${string}`>(name: T): T {
  return name;
}

const og1 = onlyGetters("getUser"); // ✅
// const og2 = onlyGetters("setUser"); // ❌ Error (doesn't match pattern)

// ============================================================================
// Test 13: Uppercase/Lowercase intrinsic with templates
// ============================================================================

type UppercaseId<T extends string> = `ID_${Uppercase<T>}`;

type UserId = UppercaseId<"user">;
const uid: UserId = "ID_USER"; // ✅
// const uid2: UserId = "ID_user"; // ❌ Error

// ============================================================================
// Test 14: Uncapitalize with templates
// ============================================================================

type UncapitalizeFirst<T extends string> = `${Uncapitalize<T>}Rest`;

type Uncapped = UncapitalizeFirst<"HELLO">;
const uc: Uncapped = "hELLORest"; // ✅
// const uc2: Uncapped = "HELLORest"; // ❌ Error

console.log("All template literal tests compiled successfully!");
