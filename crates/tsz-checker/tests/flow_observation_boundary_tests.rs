//! Regression tests for the `FlowObservation` boundary (`query_boundaries/flow.rs`).
//!
//! These tests verify that control-flow narrowing decisions are correctly
//! routed through the boundary layer rather than being made locally in the
//! checker.  Each test targets a specific observation kind.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn codes(diags: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == code).collect()
}

// =============================================================================
// Destructuring control-flow
// =============================================================================

/// Destructuring with defaults should strip `undefined` from optional properties.
#[test]
fn destructuring_property_default_strips_undefined() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(opts: { name?: string }) {
    const { name = "default" } = opts;
    const x: string = name;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Destructuring default should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Destructuring element with default in array pattern.
#[test]
fn destructuring_element_default_strips_undefined() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(arr: [string | undefined]) {
    const [first = "fallback"] = arr;
    const x: string = first;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Array destructuring default should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Nested destructuring with defaults.
#[test]
fn nested_destructuring_with_defaults() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface Config {
    server?: {
        port?: number;
    };
}
function f(config: Config) {
    const { server: { port = 3000 } = {} } = config;
    const p: number = port;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Nested destructuring default should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Optional-chain narrowing
// =============================================================================

/// Optional chain in truthy branch should narrow the base to non-nullish.
#[test]
fn optional_chain_truthy_narrows_non_nullish() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(x: { y: number } | null | undefined) {
    if (x?.y) {
        const n: number = x.y;
    }
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Optional chain truthy should narrow non-nullish, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Chained optional access in condition.
#[test]
fn chained_optional_access_narrows() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface A { b?: { c: number } }
function f(a: A | null) {
    if (a?.b?.c) {
        const n: number = a.b.c;
    }
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Chained optional access should narrow, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Catch variable unknown behavior
// =============================================================================

/// With useUnknownInCatchVariables (default strict), catch var should be `unknown`.
#[test]
fn catch_variable_is_unknown_by_default() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
try {
    throw new Error("oops");
} catch (e) {
    const msg: string = e.message;
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2571 || d.code == 18046)
        .collect();
    assert!(
        !relevant.is_empty(),
        "Catch variable should be 'unknown' with strict mode, expected error for e.message"
    );
}

/// With useUnknownInCatchVariables=false, catch var should be `any`.
#[test]
fn catch_variable_is_any_when_disabled() {
    let diags = tsz_checker::test_utils::check_source(
        r#"
try {
    throw new Error("oops");
} catch (e) {
    const msg: string = e.message;
}
"#,
        "test.ts",
        CheckerOptions {
            use_unknown_in_catch_variables: false,
            ..CheckerOptions::default()
        },
    );
    let errs = codes(&diags, 2571);
    assert!(
        errs.is_empty(),
        "Catch variable should be 'any' with flag disabled, got TS2571: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Typeof narrowing on catch variable should work from unknown domain.
#[test]
fn catch_variable_typeof_narrows_from_unknown() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
try {
    throw new Error("oops");
} catch (e) {
    if (typeof e === "string") {
        const s: string = e;
    }
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Typeof narrowing on catch variable should work, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// For-of destructuring
// =============================================================================

/// For-of with simple variable binding.
#[test]
fn for_of_simple_variable() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
const arr: number[] = [1, 2, 3];
for (const x of arr) {
    const n: number = x;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of element should have correct type, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// For-of with destructuring pattern.
#[test]
fn for_of_with_destructuring() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
const pairs: [string, number][] = [["a", 1], ["b", 2]];
for (const [key, value] of pairs) {
    const k: string = key;
    const v: number = value;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of destructuring should resolve types, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// For-of with object destructuring and defaults.
#[test]
fn for_of_object_destructuring_with_default() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface Item { name?: string; value: number }
const items: Item[] = [];
for (const { name = "unknown", value } of items) {
    const n: string = name;
    const v: number = value;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of object destructuring with default should work, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Dependent destructured variables
// =============================================================================

/// Multiple bindings from same destructuring share the parent type.
#[test]
fn dependent_destructured_bindings() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface Pair { first: string; second: number }
function f(p: Pair) {
    const { first, second } = p;
    const s: string = first;
    const n: number = second;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Dependent destructured variables should have correct types, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Rest element in destructuring.
#[test]
fn destructuring_rest_element() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(arr: [number, string, ...boolean[]]) {
    const [first, second, ...rest] = arr;
    const n: number = first;
    const s: string = second;
    const b: boolean[] = rest;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Rest element destructuring should work, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// FlowObservation boundary integration
// =============================================================================

/// Verify catch variable type is correctly routed through the boundary.
#[test]
fn flow_observation_boundary_catch_variable_integration() {
    // With strict mode (default), catch variable should be unknown
    let diags_strict = tsz_checker::test_utils::check_source_diagnostics(
        r#"
try {} catch (e) {
    let x = e;
}
"#,
    );
    let ts2322_strict = codes(&diags_strict, 2322);
    assert!(ts2322_strict.is_empty());

    // Without useUnknownInCatchVariables, catch variable should be any
    let diags_lax = tsz_checker::test_utils::check_source(
        r#"
try {} catch (e) {
    let x: number = e;
}
"#,
        "test.ts",
        CheckerOptions {
            use_unknown_in_catch_variables: false,
            ..CheckerOptions::default()
        },
    );
    let ts2322_lax = codes(&diags_lax, 2322);
    assert!(
        ts2322_lax.is_empty(),
        "catch variable with useUnknown=false should be any"
    );
}

// =============================================================================
// Phase 2: NullUndefinedWidening boundary
// =============================================================================

/// When strictNullChecks is off, a destructured binding whose type is
/// `undefined | null` should be widened to `any`.
#[test]
fn destructuring_null_undefined_widens_to_any_when_strict_off() {
    let diags = tsz_checker::test_utils::check_source(
        r#"
declare const obj: { x: undefined };
const { x } = obj;
const n: number = x;
"#,
        "test.ts",
        CheckerOptions {
            strict_null_checks: false,
            use_unknown_in_catch_variables: false,
            ..CheckerOptions::default()
        },
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "undefined binding should widen to any when strictNullChecks off, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// When strictNullChecks is on, `undefined` should NOT widen to `any`.
#[test]
fn destructuring_null_undefined_preserved_when_strict_on() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
declare const obj: { x: undefined };
const { x } = obj;
const n: number = x;
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        !errs.is_empty(),
        "undefined binding should NOT widen to any when strictNullChecks on"
    );
}

/// Variable declaration null/undefined widening through boundary.
#[test]
fn variable_null_undefined_widens_to_any_when_strict_off() {
    let diags = tsz_checker::test_utils::check_source(
        r#"
let x = undefined;
const n: number = x;
"#,
        "test.ts",
        CheckerOptions {
            strict_null_checks: false,
            use_unknown_in_catch_variables: false,
            ..CheckerOptions::default()
        },
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "undefined variable should widen to any when strictNullChecks off, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: UncheckedIndexedAccess boundary
// =============================================================================

/// noUncheckedIndexedAccess adds undefined to array element types
/// during destructuring.
#[test]
fn unchecked_indexed_access_adds_undefined_in_destructuring() {
    let diags = tsz_checker::test_utils::check_source(
        r#"
const arr: string[] = ["a", "b"];
const [first] = arr;
const s: string = first;
"#,
        "test.ts",
        CheckerOptions {
            no_unchecked_indexed_access: true,
            ..CheckerOptions::default()
        },
    );
    let errs = codes(&diags, 2322);
    assert!(
        !errs.is_empty(),
        "noUncheckedIndexedAccess should add undefined, causing TS2322"
    );
}

// =============================================================================
// Phase 2: ForInExpressionNullish boundary
// =============================================================================

/// For-in expression with a non-null type should resolve to string.
#[test]
fn for_in_variable_type_is_string() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
const obj = { a: 1, b: 2 };
for (const key in obj) {
    const s: string = key;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "for-in variable should be string, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Catch variable with destructuring in catch
// =============================================================================

/// Catch variable with explicit unknown annotation should allow typeof narrowing.
#[test]
fn catch_variable_explicit_unknown_annotation_typeof() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
try {
    throw "oops";
} catch (e: unknown) {
    if (typeof e === "string") {
        const s: string = e;
    }
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Catch variable with explicit unknown should allow typeof narrowing, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Catch variable with explicit any annotation should suppress type errors.
#[test]
fn catch_variable_explicit_any_annotation() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
try {
    throw "oops";
} catch (e: any) {
    const n: number = e.anything;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Catch variable with explicit any should suppress errors, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: For-of with various iterable patterns
// =============================================================================

/// For-of with string iteration yields string characters.
#[test]
fn for_of_string_iteration() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
const s = "hello";
for (const ch of s) {
    const c: string = ch;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of string should yield string chars, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// For-of with nested destructuring from array of objects.
#[test]
fn for_of_nested_object_destructuring() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface Entry { key: string; value: number }
const entries: Entry[] = [];
for (const { key, value } of entries) {
    const k: string = key;
    const v: number = value;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of nested object destructuring should work, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Dependent destructured variables (union narrowing)
// =============================================================================

/// Destructuring a discriminated union type preserves property types.
#[test]
fn dependent_destructured_discriminated_union() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
type A = { kind: "a"; value: string };
type B = { kind: "b"; value: number };
type AB = A | B;
function f(x: AB) {
    const { kind, value } = x;
    const k: "a" | "b" = kind;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Discriminated union destructuring should preserve types, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Multiple property destructuring from a single source.
#[test]
fn dependent_destructured_multiple_properties() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface Config {
    host: string;
    port: number;
    debug: boolean;
}
function f(config: Config) {
    const { host, port, debug } = config;
    const h: string = host;
    const p: number = port;
    const d: boolean = debug;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Multiple destructured properties should have correct types, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Destructuring with renaming preserves types.
#[test]
fn destructuring_with_rename() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface Point { x: number; y: number }
function f(p: Point) {
    const { x: myX, y: myY } = p;
    const a: number = myX;
    const b: number = myY;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Destructuring with rename should preserve types, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Optional chain + truthiness combined
// =============================================================================

/// Optional chain in ternary expression.
#[test]
fn optional_chain_ternary_narrows() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(x: { y: number } | null) {
    const result = x?.y ? x.y : 0;
    const n: number = result;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Optional chain in ternary should narrow, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Non-null assertion boundary routing
// =============================================================================

/// Non-null assertion (`!`) should strip null/undefined through the boundary.
#[test]
fn non_null_assertion_strips_nullish() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(x: string | null | undefined) {
    const y: string = x!;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Non-null assertion should strip nullish via boundary, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Non-null assertion on a narrowed-to-null variable should fall back to declared type.
#[test]
fn non_null_assertion_fallback_to_declared() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(x: string | null) {
    x = null;
    const y: string = x!;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Non-null assertion on null-assigned var should use declared type, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Nullish coalescing reachability boundary routing
// =============================================================================

/// Nullish coalescing (`??`) strips nullish from left operand through boundary.
#[test]
fn nullish_coalescing_strips_nullish() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(x: string | null) {
    const y: string = x ?? "default";
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Nullish coalescing should produce non-nullish result, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: For-in with nullish expression boundary routing
// =============================================================================

/// For-in variable type with potentially nullish expression uses boundary.
#[test]
fn for_in_nullish_expression_strips_nullish() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(obj: Record<string, number> | null) {
    if (obj) {
        for (const key in obj) {
            const s: string = key;
        }
    }
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-in variable should be string even with nullish expression, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Parameter default removes undefined through boundary
// =============================================================================

/// Parameter with default value should strip undefined from its type.
#[test]
fn parameter_default_strips_undefined() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(x: string | undefined = "hello") {
    const s: string = x;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Parameter with default should strip undefined via boundary, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Computed binding helper destructuring default boundary routing
// =============================================================================

/// Destructuring default in computed binding context uses boundary.
#[test]
fn computed_binding_destructuring_default() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface Opts { timeout?: number; retries?: number }
function f(opts: Opts) {
    const { timeout = 1000, retries = 3 } = opts;
    const t: number = timeout;
    const r: number = retries;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Computed binding destructuring default should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Destructuring default in flow analysis context
// =============================================================================

/// Assignment destructuring default should strip undefined through boundary.
#[test]
fn assignment_destructuring_default_strips_undefined() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
function f(opts: { x?: string }) {
    let y: string;
    ({ x: y = "fallback" } = opts);
    const s: string = y;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Assignment destructuring default should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: Nested destructuring with defaults in type-checking validation
// =============================================================================

/// Nested destructuring with defaults should strip undefined in type checking.
#[test]
fn nested_destructuring_default_in_type_checking() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
interface Deep { a?: { b?: { c: number } } }
function f(d: Deep) {
    const { a: { b: { c } = { c: 0 } } = {} } = d;
    const n: number = c;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "Nested destructuring defaults should strip undefined in type checking, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// =============================================================================
// Phase 2: For-of with destructuring and defaults combined
// =============================================================================

/// For-of with array destructuring and element defaults.
#[test]
fn for_of_array_destructuring_with_defaults() {
    let diags = tsz_checker::test_utils::check_source_diagnostics(
        r#"
const data: [string | undefined, number | undefined][] = [];
for (const [name = "default", val = 0] of data) {
    const s: string = name;
    const n: number = val;
}
"#,
    );
    let errs = codes(&diags, 2322);
    assert!(
        errs.is_empty(),
        "For-of array destructuring with defaults should strip undefined, got TS2322: {:?}",
        errs.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
