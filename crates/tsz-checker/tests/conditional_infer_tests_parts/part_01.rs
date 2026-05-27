#[test]
fn test_generic_object_index_with_instantiated_conditional_key() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type Selected<N extends number, I extends any[]> = {
  1: "terminal";
  0: { children: any[] };
}[Length<I> extends N ? 1 : 0];

type Depth2 = Selected<2, [any, any]>;
const value: Depth2 = "terminal";
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Generic object index should use instantiated conditional key, got: {diagnostics:?}"
    );
}

#[test]
fn test_utility_types_function_keys_generic_pick_has_no_false_diagnostics() {
    let source = r#"
type NonUndefined<A> = A extends undefined ? never : A;
type FunctionKeys<T extends object> = {
  [K in keyof T]-?: NonUndefined<T[K]> extends (...args: any[]) => unknown ? K : never;
}[keyof T];
type FunctionProps<T extends object> = Pick<T, FunctionKeys<T>>;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected utility-types FunctionKeys/Pick pattern to check cleanly, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_infer_inside_type_predicate_target_resolves_in_true_branch() {
    // `target is infer X` exposes `X` to the conditional's true branch the
    // same way return-position `infer X` does.
    let source = r#"
type X<F> = F extends (x: any) => x is infer N ? N : never;
type Test = X<(x: any) => x is string>;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected `infer` inside type predicate to bind in conditional true branch, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_infer_inside_asserts_predicate_resolves_in_true_branch() {
    let source = r#"
type AssertedType<F> = F extends (target: any) => asserts target is infer N ? N : never;
type Test = AssertedType<(target: any) => asserts target is number>;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected `infer` inside asserts predicate to bind, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// Tests for fix: try_expand_application must evaluate its instantiated result
// so that distributive conditionals (K extends K ? [K, ...] : never) are resolved
// to concrete union-of-tuples before the structural subtype check proceeds.
//
// Structural rule: "When an Application type's expanded body contains conditional
// types from distributive instantiation over a union type parameter, the expanded
// result must be fully evaluated before subtype comparison."

#[test]
fn test_permutation_type_with_default_param_and_distribution() {
    // Case 1 (reported repro): T/K naming, numeric literals
    let source = r#"
type MyExclude<T, U> = T extends U ? never : T;

type Permutation<T, K = T> =
  [T] extends [never]
    ? []
    : K extends K
      ? [K, ...Permutation<MyExclude<T, K>>]
      : never;

type P = Permutation<1 | 2>;

const p1: P = [1, 2];
const p2: P = [2, 1];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Expected no errors for Permutation<1|2> assignments, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_permutation_type_renamed_params() {
    // Case 2: same rule, different type parameter names (A/B instead of T/K)
    // proves the fix is structural, not dependent on parameter name spelling
    let source = r#"
type Rem<A, B> = A extends B ? never : A;

type Perm<A, B = A> =
  [A] extends [never]
    ? []
    : B extends B
      ? [B, ...Perm<Rem<A, B>>]
      : never;

type Q = Perm<"x" | "y">;

const q1: Q = ["x", "y"];
const q2: Q = ["y", "x"];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Expected no errors for Perm<'x'|'y'> with renamed params, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_permutation_type_with_builtin_exclude() {
    let source = r#"
type Permutation<T, K = T> =
  [T] extends [never]
    ? []
    : K extends K
      ? [K, ...Permutation<Exclude<T, K>>]
      : never;

type P = Permutation<"A" | "B">;

const p1: P = ["A", "B"];
const p2: P = ["B", "A"];
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert!(
        diags.is_empty(),
        "Expected no errors for Permutation<'A'|'B'> through built-in Exclude, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_permutation_type_three_element_union() {
    // Case 3: larger union (3 members) — ensures distribution over >2 members works
    let source = r#"
type MyExclude<T, U> = T extends U ? never : T;

type Permutation<T, K = T> =
  [T] extends [never]
    ? []
    : K extends K
      ? [K, ...Permutation<MyExclude<T, K>>]
      : never;

type P3 = Permutation<1 | 2 | 3>;

const pa: P3 = [1, 2, 3];
const pb: P3 = [2, 1, 3];
const pc: P3 = [3, 1, 2];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Expected no errors for Permutation<1|2|3> assignments, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_permutation_type_invalid_assignment_still_rejected() {
    // Case 4 (negative): invalid value must still be rejected — the fix must not
    // loosen type safety for non-permutation tuples
    let source = r#"
type MyExclude<T, U> = T extends U ? never : T;

type Permutation<T, K = T> =
  [T] extends [never]
    ? []
    : K extends K
      ? [K, ...Permutation<MyExclude<T, K>>]
      : never;

type P = Permutation<1 | 2>;

const bad: P = [3, 1];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for invalid permutation [3, 1]: P, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// Rule: when `infer R` matches a generic function's return type, free type parameters
// must be erased to their upper bounds (constraint or `unknown`) before `R` is bound.

const GET_RET_DEF: &str = "type GetRet<F> = F extends (...args: any[]) => infer R ? R : never;\n";

#[test]
fn return_type_of_unconstrained_generic_fn_erases_to_unknown() {
    let source = format!(
        r#"{GET_RET_DEF}
function generic<T>(x: T): T[] {{
    return [x];
}}
type GR = GetRet<typeof generic>;
const gr: GR = ["test"];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of generic<T>: T[] should erase T to unknown, making string[] assignable. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_type_of_unconstrained_generic_fn_different_param_name() {
    // Same rule, different type-parameter name (U instead of T) — proves the fix
    // is not hardcoded to any specific name.
    let source = format!(
        r#"{GET_RET_DEF}
function wrap<U>(val: U): U[] {{
    return [val];
}}
type WR = GetRet<typeof wrap>;
const wr: WR = [42];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of wrap<U>: U[] should erase U to unknown. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_type_of_constrained_generic_fn_erases_to_constraint() {
    let source = format!(
        r#"{GET_RET_DEF}
function constrained<T extends string>(x: T): T[] {{
    return [x];
}}
type CR = GetRet<typeof constrained>;
const cr: CR = ["hello"];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of constrained<T extends string> erases T to string. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_infer_only_pattern_erases_single_type_param() {
    let source = format!(
        r#"{GET_RET_DEF}
function identity<K>(x: K): K {{
    return x;
}}
type IR = GetRet<typeof identity>;
const accepted: IR = "any value";
const accepted2: IR = 42;
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of identity<K>: K should produce unknown. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_type_of_multi_param_generic_fn_erases_all_type_params() {
    let source = format!(
        r#"{GET_RET_DEF}
function pair<A, B>(a: A, b: B): [A, B] {{
    return [a, b];
}}
type PR = GetRet<typeof pair>;
const pr: PR = ["x", 1];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of pair<A,B> should erase both to unknown. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_type_of_non_generic_fn_still_precise() {
    let source = format!(
        r#"{GET_RET_DEF}
function nums(): number[] {{
    return [1, 2, 3];
}}
type NR = GetRet<typeof nums>;
const bad: NR = ["oops"];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "GetRet of non-generic nums(): number[] should stay number[]. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_with_unknown_rest_resolves_to_string() {
    // issue #6253: fixed source params must match the element type of a non-infer rest param
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: unknown[]) => unknown ? A : never;
type A1 = FirstArg<(a: string, b: number) => void>;
const a1: A1 = "test";
const bad: A1 = 42;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Only `bad: A1 = 42` should error (A1 = string). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_different_names_resolves_correctly() {
    // different type-param and rest-param names — fix must be structural, not name-keyed
    let source = r#"
type FirstArg<T> = T extends (first: infer S, ...rest: unknown[]) => unknown ? S : never;
type F1 = FirstArg<(x: number, y: string) => void>;
const ok: F1 = 1;
const bad: F1 = "nope";
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Only `bad: F1 = \"nope\"` should error (F1 = number). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_multiple_extra_fixed_params() {
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: unknown[]) => unknown ? A : never;
type F3 = FirstArg<(a: string, b: number, c: boolean) => void>;
const ok: F3 = "hi";
const bad: F3 = 1;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Only numeric assignment should fail (F3 = string). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_rest_elem_constraint_fails_for_incompatible_extra_param() {
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: string[]) => unknown ? A : never;
type F = FirstArg<(a: string, b: object) => void>;
const accepted: F = "x" as never;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "F should be `never`; assigning `never` is valid (no errors). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_with_return_infer_and_unknown_rest() {
    let source = r#"
type FirstArgAndRet<T> =
  T extends (x: infer A, ...args: unknown[]) => infer R ? [A, R] : never;
type FR = FirstArgAndRet<(a: string, b: number) => boolean>;
const ok: FR = ["hi", true];
const bad: FR = [1, true];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "FR = [string, boolean]; only `[1, true]` should fail. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_source_rest_param_against_unknown_rest_pattern() {
    // source rest param compared array-to-array (not array vs element) against pattern rest
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: unknown[]) => unknown ? A : never;
type FR = FirstArg<(a: string, ...rest: number[]) => void>;
const ok: FR = "hi";
const bad: FR = 99;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "FR = string; only numeric assignment should fail. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_source_with_no_extra_params() {
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: unknown[]) => unknown ? A : never;
type F0 = FirstArg<(a: number) => void>;
const ok: F0 = 1;
const bad: F0 = "nope";
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "F0 = number; only string assignment should fail. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// ── Tuple rest/last infer spread flattening (issue #6671) ──────────────────

/// `[...infer R, infer L]` captures R as a tuple. The true branch `[L, ...R]`
/// must spread R's elements into the result, not wrap them in a nested tuple.
#[test]
fn test_rotate_right_tuple_infer_rest_last_spreads_correctly() {
    let source = r#"
type RotateRight<T extends unknown[]> =
  T extends [...infer R, infer L] ? [L, ...R] : T;

type RR1 = RotateRight<[1, 2, 3]>;
type RR2 = RotateRight<[string, number]>;

const ok1: RR1 = [3, 1, 2];
const ok2: RR2 = [42, "hello"];
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert!(
        diags.is_empty(),
        "RotateRight should produce no errors. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// `RotateRight` should reject assignments that don't match the rotated type.
#[test]
fn test_rotate_right_rejects_wrong_assignment() {
    let source = r#"
type RotateRight<T extends unknown[]> =
  T extends [...infer R, infer L] ? [L, ...R] : T;

type RR1 = RotateRight<[1, 2, 3]>;
const bad: RR1 = [1, 2, 3];
"#;
    let diags = check_source_strict_with_default_libs(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Assigning [1,2,3] to RotateRight<[1,2,3]>=[3,1,2] should error."
    );
}

/// `RotateLeft` `[infer F, ...infer R]` followed by `[...R, F]` must also spread R.
#[test]
fn test_rotate_left_tuple_infer_first_rest_spreads_correctly() {
    let source = r#"
type RotateLeft<T extends unknown[]> =
  T extends [infer F, ...infer R] ? [...R, F] : T;

type RL1 = RotateLeft<[1, 2, 3]>;
type RL2 = RotateLeft<[string, number, boolean]>;

const ok1: RL1 = [2, 3, 1];
const ok2: RL2 = [42, true, "hello"];
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert!(
        diags.is_empty(),
        "RotateLeft should produce no errors. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// `Reverse<T>` uses `[...infer Init, infer Last]` recursively; every recursive
/// step must spread the init portion correctly.
#[test]
fn test_recursive_reverse_tuple_spreads_correctly() {
    let source = r#"
type Reverse<T extends unknown[]> =
  T extends [...infer Init, infer Last] ? [Last, ...Reverse<Init>] : T;

type Rev1 = Reverse<[1, 2, 3]>;
type Rev2 = Reverse<[string, number]>;

const ok1: Rev1 = [3, 2, 1];
const ok2: Rev2 = [42, "hello"];
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert!(
        diags.is_empty(),
        "Reverse should produce no errors. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Tests for issue #6657: infer bindings in complex extends clauses
//
// Structural rule: when the extends clause of a conditional type contains
// `infer X` (regardless of surrounding expression complexity), references to
// `X` in the true/false branches must resolve without TS2304.
// ============================================================================

fn no_ts2304(diags: &[tsz_checker::diagnostics::Diagnostic], ctx: &str) {
    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2304).collect();
    assert!(
        errors.is_empty(),
        "{ctx}: expected no TS2304, got: {:?}",
        errors
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

fn ts2304_count(diags: &[tsz_checker::diagnostics::Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 2304).count()
}

/// Utility type in check position: `Pick<T, K> extends infer R`
/// — `R` must be visible inside the true branch mapped type.
#[test]
fn infer_binding_visible_when_check_type_is_utility_application() {
    let source = r#"
type Flatten<T, K extends keyof T> =
    Pick<T, K> extends infer R ? { [P in keyof R]: R[P] } : never;
interface Obj { a: string; b: number }
type T1 = Flatten<Obj, "a">;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "Pick<T,K> extends infer R");
}

/// Intersection in check position: `T & U extends infer V`
/// — `V` must be visible in the true branch.
#[test]
fn infer_binding_visible_when_check_type_is_intersection() {
    let source = r#"
type Merge<A, B> =
    A & B extends infer V ? { [K in keyof V]: V[K] } : never;
type T2 = Merge<{ x: number }, { y: string }>;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "A & B extends infer V");
}

/// Real-world `RequiredByKeys` pattern with Omit & Required<Pick>.
#[test]
fn infer_binding_visible_in_required_by_keys_pattern() {
    let source = r#"
type RequiredByKeys<T, K extends keyof T = keyof T> =
    Omit<T, K> & Required<Pick<T, K>> extends infer X
        ? { [P in keyof X]: X[P] }
        : never;
interface User { name?: string; age?: number }
type T3 = RequiredByKeys<User, "name">;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "Omit<T,K> & Required<Pick<T,K>> extends infer X");
}

/// Multiple infer bindings in the extends clause.
/// All bound names must be visible in the branches.
#[test]
fn multiple_infer_bindings_all_visible_in_branches() {
    let source = r#"
type Unpack<T> =
    T extends { first: infer A; second: infer B }
        ? { [K in keyof A | keyof B]: K extends keyof A ? A[K] : B[K extends keyof B ? K : never] }
        : never;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "multiple infer A, B");
}

/// Renamed infer variable (Q instead of R) — proves the fix is not
/// tied to any specific identifier spelling.
#[test]
fn infer_binding_works_with_any_variable_name() {
    let source = r#"
type Spread<T, K extends keyof T> =
    Pick<T, K> extends infer Q ? { [P in keyof Q]: Q[P] } : never;
interface Data { x: number; y: string }
type T4 = Spread<Data, "x">;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "Pick<T,K> extends infer Q (renamed variable)");
}

/// The simple case (check type is a direct type parameter) must still work:
/// no regression for `T extends infer R`.
#[test]
fn infer_binding_simple_type_param_check_no_regression() {
    let source = r#"
type Identity<T> = T extends infer R ? { [P in keyof R]: R[P] } : never;
type T5 = Identity<{ a: string }>;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "T extends infer R (simple case regression)");
}

/// Infer bindings are scoped to the true branch only.
/// The false branch must still report unknown-name errors for `R`.
#[test]
fn infer_binding_not_visible_in_false_branch() {
    let source = r#"
type Bad<T> =
    T extends { a: infer R }
        ? string
        : { [K in keyof R]: R[K] };
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert_eq!(
        ts2304_count(&diags),
        2,
        "false branch infer references should remain unbound. Actual diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_extends_constraint_on_tuple_length() {
    let source = r#"
type GetLength<T> = T extends { length: infer L extends number } ? L : never;

type Len1 = GetLength<[1, 2, 3]>;
const l1: Len1 = 3;
const l1bad: Len1 = 4;

type LenArr = GetLength<number[]>;
const larr: LenArr = 42;

type LenObj = GetLength<{ length: 5 }>;
const lobj: LenObj = 5;
const lobjbad: LenObj = 6;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect();
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2322).count(),
        2,
        "Expected exactly 2 TS2322 errors (l1bad and lobjbad); got: {codes:?}"
    );
}

#[test]
fn infer_extends_constraint_on_string_literal_length() {
    // String types have `length: number` from the String interface.
    // tsc infers `number` (not a literal count) for string literal length.
    // The bug: tsz returned `never` because string types weren't handled in
    // the conditional infer property resolver.
    let source = r#"
type GetStringLength<T> = T extends { length: infer L extends number } ? L : never;

type L1 = GetStringLength<"hi">;
type L2 = GetStringLength<"hello">;
type L3 = GetStringLength<"">;
type L4 = GetStringLength<string>;

const l1: L1 = 2;
const l2: L2 = 5;
const l3: L3 = 0;
const l4: L4 = 42;

const l1bad: L1 = "oops";
const l4bad: L4 = "also_oops";
"#;
    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect();
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2322).count(),
        2,
        "Expected exactly 2 TS2322 errors (l1bad and l4bad — string to number); got: {codes:?}"
    );
}

#[test]
fn infer_extends_constraint_on_string_length_with_infer_name_variants() {
    // The fix must work regardless of the infer variable name.
    // Structural rule: string sources match { length: infer X extends number }
    // for any bound name X or Y.
    let source_x = r#"
type GetLen<T> = T extends { length: infer X extends number } ? X : never;
type R1 = GetLen<"abc">;
const ok: R1 = 3;
const bad: R1 = "nope";
"#;
    let source_y = r#"
type GetLen<T> = T extends { length: infer Y extends number } ? Y : never;
type R1 = GetLen<"abc">;
const ok: R1 = 3;
const bad: R1 = "nope";
"#;
    for (name, src) in [("X-bound", source_x), ("Y-bound", source_y)] {
        let diagnostics = tsz_checker::test_utils::check_source_strict(src);
        let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
        assert_eq!(
            ts2322.len(),
            1,
            "{name}: expected exactly 1 TS2322 (string-to-number); got: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, d.message_text.as_str()))
                .collect::<Vec<_>>()
        );
    }
}

// =============================================================================
// IsUnion<T> supplement — cases not covered by distributive_conditional_default_tests.rs
// =============================================================================

const IS_UNION_PRELUDE: &str = r#"
type IsUnion<T, U = T> = T extends U
  ? [U] extends [T]
    ? false
    : true
  : never;
"#;

#[test]
fn is_union_of_primitives_evaluates_to_true() {
    assert_no_ts2322(
        &format!(
            "{IS_UNION_PRELUDE}\n\
            type R = IsUnion<string | number>;\n\
            const r: R = true;\n"
        ),
        "IsUnion<string | number> = true",
    );
}

#[test]
fn is_union_diagnostic_shows_evaluated_literal_not_alias() {
    let source = format!(
        "{IS_UNION_PRELUDE}\n\
        type R = IsUnion<\"a\" | \"b\">;\n\
        const r: R = false;\n"
    );
    let diags = tsz_checker::test_utils::check_source_strict(&source);
    let msgs = tsz_checker::test_utils::diagnostic_messages_with_code(&diags, 2322);
    assert_eq!(
        msgs.len(),
        1,
        "Expected exactly one TS2322; got: {diags:#?}"
    );
    let msg = msgs[0];
    assert!(
        msg.contains("'false'") && msg.contains("'true'"),
        "Expected evaluated literal types in message; got: {msg:?}"
    );
    assert!(
        !msg.contains("IsUnion"),
        "Diagnostic must not show unevaluated alias 'IsUnion'; got: {msg:?}"
    );
}

// =============================================================================
// Constructor return type infer — typeof Class patterns (issue #6157)
// Rule: when a constructor pattern `new (...) => infer I` checks a `typeof C`
// expression, the check type must be fully evaluated from the TypeQuery before
// pattern matching, and construct signatures must be selected (not call signatures)
// when the source Callable carries both.
// =============================================================================

fn assert_no_ts2322_with_libs(source: &str, label: &str) {
    let diags = check_source_strict_with_default_libs(source);
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "[{label}] expected no TS2322, got:\n{:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn constructor_infer_typeof_date_returns_date_not_never() {
    // `typeof Date` has both call (returns string) and construct (returns Date) sigs.
    // The constructor pattern must use construct signatures → I = Date.
    let source = r#"
type InstanceOf<T> = T extends new (...args: any[]) => infer I ? I : never;
type IO = InstanceOf<typeof Date>;
const io: IO = new Date();
export {};
"#;
    assert_no_ts2322_with_libs(source, "InstanceOf<typeof Date> = Date");
}

#[test]
fn constructor_infer_user_class_without_libs() {
    // User-defined class `typeof Cls` must also resolve correctly.
    // Tests that visit_type_query deep-evaluates Lazy types.
    assert_no_ts2322(
        r#"
class Cls { x: number = 1; }
type InstanceOf<T> = T extends new (...args: any[]) => infer I ? I : never;
type R = InstanceOf<typeof Cls>;
const r: R = new Cls();
export {};
"#,
        "InstanceOf<typeof Cls> = Cls",
    );
}

#[test]
fn constructor_infer_renamed_type_param_k_user_class() {
    // Renamed outer and infer params must not change the result — the fix must be structural.
    assert_no_ts2322(
        r#"
class Widget { name: string = ""; }
type ConstructedBy<K> = K extends new (...args: any[]) => infer Result ? Result : never;
type W = ConstructedBy<typeof Widget>;
const w: W = new Widget();
export {};
"#,
        "ConstructedBy<typeof Widget> = Widget",
    );
}

#[test]
fn constructor_infer_typeof_map_returns_map_not_never() {
    // Map also has both call and construct sigs — confirms construct-sig selection is general.
    let source = r#"
type InstanceOf<T> = T extends new (...args: any[]) => infer I ? I : never;
type M = InstanceOf<typeof Map>;
const m: M = new Map();
export {};
"#;
    assert_no_ts2322_with_libs(source, "InstanceOf<typeof Map> = Map");
}

#[test]
fn constructor_infer_non_constructable_yields_never() {
    // A plain call-only function type must not match a construct pattern → `never`.
    assert_no_ts2322(
        r#"
type InstanceOf<T> = T extends new (...args: any[]) => infer I ? I : never;
type R = InstanceOf<() => string>;
type IsNever = [R] extends [never] ? true : false;
const check: IsNever = true;
export {};
"#,
        "InstanceOf<() => string> = never",
    );
}

const EQUAL_PRELUDE: &str = r#"type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;
"#;

#[test]
fn equal_any_then_is_union_no_cross_contamination() {
    // Equal<any, X> evaluations must not corrupt subsequent IsUnion evaluations.
    assert_no_ts2322(
        &format!(
            "{EQUAL_PRELUDE}\n\
            {IS_UNION_PRELUDE}\n\
            type E1 = Equal<any, string>;  const e1: E1 = false;\n\
            type E2 = Equal<any, number>;  const e2: E2 = false;\n\
            type E3 = Equal<string, any>;  const e3: E3 = false;\n\
            type U1 = IsUnion<\"a\" | \"b\">; const u1: U1 = true;\n\
            type F1 = IsUnion<string>;     const f1: F1 = false;\n\
            type U2 = IsUnion<1 | 2>;      const u2: U2 = true;\n\
            type F2 = IsUnion<number>;     const f2: F2 = false;\n\
            type E4 = Equal<string, string>; const e4: E4 = true;\n\
            type E5 = Equal<{{a: 1}}, {{a: 1}}>; const e5: E5 = true;\n\
            export {{}};\n"
        ),
        "Equal<any,X> then IsUnion cross-contamination",
    );
}

// =============================================================================
// Issue #6374: Application-source infer matching via structural expansion
//
// Rule: When `source` is `Application(A, args)` and the pattern is
// `Application(B, infers)` where A structurally extends B (but A != B),
// the infer variables in B's pattern should be bound by expanding both
// sides to their structural forms and matching property-by-property.
//
// Concrete case: Promise<X> extends PromiseLike<infer U> → U = X.
// =============================================================================

#[test]
fn promiselike_infer_unwraps_promise_application() {
    // type Awaited2<T> = T extends PromiseLike<infer U> ? Awaited2<U> : T;
    // type A = Awaited2<Promise<string>>;  // should be string
    let source = r#"
type Awaited2<T> = T extends PromiseLike<infer U> ? Awaited2<U> : T;
type A = Awaited2<Promise<string>>;
const _a: A = "hello";
export {};
"#;
    assert_no_ts2322(source, "Awaited2<Promise<string>> = string");
}

#[test]
fn promiselike_infer_unwraps_nested_promise_application() {
    // tsc: Awaited2<Promise<Promise<string>>> = string
    let source = r#"
type Awaited2<T> = T extends PromiseLike<infer U> ? Awaited2<U> : T;
type A = Awaited2<Promise<Promise<string>>>;
const _a: A = "nested";
export {};
"#;
    assert_no_ts2322(source, "Awaited2<Promise<Promise<string>>> = string");
}

#[test]
fn promiselike_infer_terminates_on_non_promise() {
    // When T does not extend PromiseLike, the result is T itself.
    let source = r#"
type Awaited2<T> = T extends PromiseLike<infer U> ? Awaited2<U> : T;
type N = Awaited2<number>;
const _n: N = 42;
type S = Awaited2<string>;
const _s: S = "hello";
export {};
"#;
    assert_no_ts2322(source, "Awaited2<non-promise> = identity");
}

#[test]
fn promiselike_infer_with_renamed_type_param() {
    // Verifies the fix is not tied to the name 'U' or 'T'.
    let source = r#"
type Unwrap<X> = X extends PromiseLike<infer Inner> ? Unwrap<Inner> : X;
type A = Unwrap<Promise<number>>;
const _a: A = 42;
export {};
"#;
    assert_no_ts2322(source, "Unwrap with renamed params unwraps Promise<number>");
}

#[test]
fn promiselike_infer_bound_to_complex_type() {
    // The infer variable should bind to any type arg, including objects and unions.
    let source = r#"
type Extract<T> = T extends PromiseLike<infer U> ? U : never;
type A = Extract<Promise<{ x: number; y: string }>>;
const _a: A = { x: 1, y: "hello" };
export {};
"#;
    assert_no_ts2322(source, "Extract PromiseLike<infer U> from Promise<object>");
}
