// BUG REPORT: Discovered issues in tsz implementation

// ==================== BUG 1: Key Remapping with Conditional Types ====================
// Location: src/solver/evaluate_rules/mapped.rs
// Issue: Key remapping using 'as' with conditional types not working correctly

type O = { a: string; b: number; c: boolean };
type Filtered = {
    [K in keyof O as O[K] extends string ? K : never]: O[K]
};

// tsc ACCEPTS this, tsz REJECTS
// tsz error: Type '{ a: string }' is not assignable to type 'Filtered'
const filtered: Filtered = { a: "hello" };

// ==================== BUG 2: Remove Readonly Modifier ====================
// Location: src/solver/evaluate_rules/mapped.rs
// Issue: -readonly modifier not working

type ReadonlyObj = { readonly a: string };
type Mutable = { -readonly [K in keyof ReadonlyObj]: ReadonlyObj[K] };

// tsc: mutable.a = "world" should work (property is mutable)
// tsz error: Type '{ a: string }' is not assignable to type 'Mutable'
const mutable: Mutable = { a: "hello" };
// mutable.a = "world"; // Uncomment to test

// ==================== BUG 3: Remove Optional Modifier ====================
// Location: src/solver/evaluate_rules/mapped.rs
// Issue: -? modifier not working

type OptionalObj = { a?: string };
type RequiredObj = { [K in keyof OptionalObj]-?: OptionalObj[K] };

// tsc: Should require 'a' to be present
// tsz error: Type '{ a: string }' is not assignable to type 'RequiredObj'
const required: RequiredObj = { a: "hello" };

// ==================== BUG 4: Recursive Mapped Types ====================
// Location: src/solver/evaluate_rules/mapped.rs
// Issue: Deep recursion or incorrect evaluation

type DeepPartial<T> = {
    [P in keyof T]?: DeepPartial<T[P]>;
};

type Nested = { a: { b: string } };
type RPartial = DeepPartial<Nested>;

// tsc ACCEPTS all of these:
const rp1: RPartial = {};
const rp2: RPartial = { a: {} };
const rp3: RPartial = { a: { b: "hello" } };

// tsz REJECTS rp2 and rp3

// ==================== BUG 5: Template Literal - any Interpolation ====================
// Location: src/solver/evaluate_rules/template_literal.rs
// Issue: ${any} should widen to string

type TAny = `val: ${any}`;

// tsc: Should be string
// tsz error: Type 'string' is not assignable to type 'TAny'
const tan: TAny = "val: anything";

// ==================== BUG 6: Template Literal - Number Formatting ====================
// Location: src/solver/evaluate_rules/template_literal.rs
// Issue: Number to string conversion

type TNum1 = `${0.000001}`; // Expected: "0.000001"
type TNum2 = `${0.0000001}`; // Expected: "1e-7"

// tsc ACCEPTS these
// tsz error: Type 'string' is not assignable to type 'TNum1'
const tnum1: TNum1 = "0.000001";
const tnum2: TNum2 = "1e-7";

console.log("Bug report test file");
