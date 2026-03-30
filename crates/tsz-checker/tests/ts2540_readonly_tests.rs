//! Tests for TS2540 readonly property assignment errors
//!
//! Verifies that assigning to readonly properties emits TS2540.

use tsz_binder::BinderState;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn has_error_with_code(source: &str, code: u32) -> bool {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        tsz_checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().any(|d| d.code == code)
}

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        tsz_checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318) // Filter global type errors
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// =========================================================================
// Class readonly property tests
// =========================================================================

#[test]
fn test_readonly_class_property_assignment() {
    let source = r"
class C {
    readonly x: number = 1;
}
const c = new C();
c.x = 10;
";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to readonly class property"
    );
}

#[test]
fn test_non_readonly_class_property_assignment_ok() {
    let source = r"
class C {
    y: number = 2;
}
const c = new C();
c.y = 20;
";
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for non-readonly property"
    );
}

#[test]
fn test_readonly_class_mixed_properties() {
    // Class with both readonly and mutable properties
    let source = r#"
class C {
    readonly ro: string = "hello";
    mut_prop: string = "world";
}
const c = new C();
c.ro = "new";
c.mut_prop = "ok";
"#;
    let diags = get_diagnostics(source);
    let ts2540_count = diags.iter().filter(|(code, _)| *code == 2540).count();
    assert_eq!(
        ts2540_count, 1,
        "Should emit exactly 1 TS2540 (for ro), got: {diags:?}"
    );
}

// =========================================================================
// Interface readonly property tests
// =========================================================================

#[test]
fn test_readonly_interface_property() {
    let source = r"
interface I {
    readonly x: number;
}
declare const obj: I;
obj.x = 10;
";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to readonly interface property"
    );
}

#[test]
fn test_non_readonly_interface_property_ok() {
    let source = r"
interface I {
    x: number;
}
declare const obj: I;
obj.x = 10;
";
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for mutable interface property"
    );
}

// =========================================================================
// Const variable tests
// =========================================================================

#[test]
fn test_const_variable_assignment() {
    // TS2588: Cannot assign to 'x' because it is a constant
    let source = r"
const x = 10;
x = 20;
";
    assert!(
        has_error_with_code(source, 2588),
        "Should emit TS2588 for assigning to const variable"
    );
}

// =========================================================================
// Namespace const export tests
// =========================================================================

#[test]
fn test_namespace_const_export_readonly() {
    let source = r"
namespace M {
    export const x = 0;
}
M.x = 1;
";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to namespace const export"
    );
}

// =========================================================================
// Interface mixed readonly tests
// =========================================================================

#[test]
fn test_readonly_interface_mixed_properties() {
    // Interface with both readonly and mutable properties
    let source = r#"
interface I {
    readonly ro: string;
    mut_prop: string;
}
declare const obj: I;
obj.ro = "new";
obj.mut_prop = "ok";
"#;
    let diags = get_diagnostics(source);
    let ts2540_count = diags.iter().filter(|(code, _)| *code == 2540).count();
    assert_eq!(
        ts2540_count, 1,
        "Should emit exactly 1 TS2540 (for ro), got: {diags:?}"
    );
}

#[test]
fn test_readonly_interface_multiple_readonly_props() {
    // Interface with multiple readonly properties
    let source = r#"
interface I {
    readonly a: number;
    readonly b: string;
    c: boolean;
}
declare const obj: I;
obj.a = 1;
obj.b = "x";
obj.c = true;
"#;
    let diags = get_diagnostics(source);
    let ts2540_count = diags.iter().filter(|(code, _)| *code == 2540).count();
    assert_eq!(
        ts2540_count, 2,
        "Should emit 2 TS2540 errors (for a and b), got: {diags:?}"
    );
}

// =========================================================================
// Namespace let export should be mutable
// =========================================================================

#[test]
fn test_namespace_let_export_mutable() {
    let source = r"
namespace M {
    export let x = 0;
}
M.x = 1;
";
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for namespace let export"
    );
}

// =========================================================================
// Element access readonly tests
// =========================================================================

#[test]
fn test_readonly_interface_element_access() {
    // obj["x"] should also be caught as readonly
    let source = r#"
interface I {
    readonly x: number;
}
declare const obj: I;
obj["x"] = 10;
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for element access to readonly interface property"
    );
}

#[test]
fn test_readonly_class_compound_assignment() {
    // Compound assignments (+=, -=, etc.) should also be caught
    let source = r"
class C {
    readonly x: number = 1;
}
const c = new C();
c.x += 10;
";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for compound assignment to readonly class property"
    );
}

#[test]
fn test_readonly_class_increment() {
    // Increment/decrement should also be caught
    let source = r"
class C {
    readonly x: number = 1;
}
const c = new C();
c.x++;
";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for increment on readonly class property"
    );
}

// =========================================================================
// Parenthesized expression tests
// =========================================================================

#[test]
fn test_readonly_parenthesized_increment() {
    // ++((M.x)) should detect readonly through parentheses
    let source = r"
namespace M {
    export const x = 0;
}
++((M.x));
";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for parenthesized increment on namespace const"
    );
}

#[test]
fn test_readonly_parenthesized_assignment() {
    // (obj.x) = 1 should detect readonly through parentheses
    let source = r"
interface I {
    readonly x: number;
}
declare const obj: I;
(obj.x) = 10;
";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for parenthesized assignment to readonly property"
    );
}

#[test]
fn test_readonly_double_parenthesized_increment() {
    // ++((obj.x)) with double parens
    let source = r"
class C {
    readonly x: number = 1;
}
const c = new C();
++((c.x));
";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for double-parenthesized increment on readonly"
    );
}

#[test]
fn test_non_readonly_parenthesized_ok() {
    // Parenthesized assignment to non-readonly should be fine
    let source = r"
class C {
    x: number = 1;
}
const c = new C();
(c.x) = 10;
";
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for parenthesized assignment to mutable property"
    );
}

// =========================================================================
// globalThis readonly property tests
// =========================================================================

#[test]
fn test_global_this_self_reference_is_readonly() {
    // globalThis.globalThis is a readonly self-reference (TS2540)
    let source = r"globalThis.globalThis = 1 as any;";
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to globalThis.globalThis (readonly)"
    );
}

#[test]
fn test_global_this_var_property_assignment_ok() {
    // globalThis.x where x is a var-declared global should not emit TS2540
    let source = r"
var x = 1;
globalThis.x = 3;
";
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for assigning to var-declared global via globalThis"
    );
}

// =========================================================================
// Readonly tuple element access: TS2540 vs TS2542
// =========================================================================

#[test]
fn test_readonly_tuple_fixed_element_emits_ts2540() {
    // Assigning to a fixed element of a readonly tuple should emit TS2540
    // (named property), NOT TS2542 (index signature).
    let source = r"
declare var v: readonly [number, number];
v[0] = 1;
";
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2540),
        "Should emit TS2540 for assigning to readonly tuple fixed element, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2542),
        "Should NOT emit TS2542 for readonly tuple fixed element, got: {diags:?}"
    );
}

#[test]
fn test_readonly_tuple_rest_element_emits_ts2542() {
    // Assigning to a rest-range index of a readonly tuple should emit TS2542
    // (index signature only permits reading).
    let source = r"
declare var v: readonly [number, number, ...number[]];
v[2] = 1;
";
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2542),
        "Should emit TS2542 for assigning to readonly tuple rest element, got: {diags:?}"
    );
}

#[test]
fn test_readonly_tuple_mixed_fixed_and_rest() {
    // Fixed elements get TS2540, rest-range elements get TS2542.
    let source = r"
declare var v: readonly [number, number, ...number[]];
v[0] = 1;
v[1] = 1;
v[2] = 1;
";
    let diags = get_diagnostics(source);
    let ts2540_count = diags.iter().filter(|d| d.0 == 2540).count();
    let ts2542_count = diags.iter().filter(|d| d.0 == 2542).count();
    assert_eq!(
        ts2540_count, 2,
        "Expected 2 TS2540 for fixed elements 0 and 1, got {ts2540_count}: {diags:?}"
    );
    assert_eq!(
        ts2542_count, 1,
        "Expected 1 TS2542 for rest element 2, got {ts2542_count}: {diags:?}"
    );
}

// =========================================================================
// String primitive readonly index signature
// =========================================================================

#[test]
fn test_string_primitive_element_access_assignment_emits_ts2542() {
    // The `string` primitive has an implicit readonly number index signature.
    // Assigning via bracket notation (e.g., `s[0] = "x"`) should emit TS2542.
    let source = r#"
declare var s: string;
s[0] = "x";
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2542),
        "Should emit TS2542 for assigning to string element, got: {diags:?}"
    );
}

#[test]
fn test_string_union_element_access_assignment_emits_ts2542() {
    // A union containing `string` has a readonly number index at the union level
    // because `string` has a readonly implicit number index.
    let source = r#"
interface Obj { [n: number]: string; }
declare var x: string | Obj;
x[0] = "y";
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2542),
        "Should emit TS2542 for assigning to union with string member, got: {diags:?}"
    );
}

#[test]
fn test_string_reading_element_access_no_ts2542() {
    // Reading from a string via bracket notation should NOT emit TS2542.
    let source = r#"
declare var s: string;
let c = s[0];
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2542),
        "Should NOT emit TS2542 for reading from string element, got: {diags:?}"
    );
}

// =========================================================================
// TS1354: readonly type modifier on non-array/tuple types
// =========================================================================

#[test]
fn test_readonly_on_non_array_type_emits_ts1354() {
    let source = r"
type T = readonly string;
";
    assert!(
        has_error_with_code(source, 1354),
        "Should emit TS1354 for 'readonly string'"
    );
}

#[test]
fn test_readonly_on_array_type_no_ts1354() {
    let source = r"
type T = readonly string[];
";
    assert!(
        !has_error_with_code(source, 1354),
        "Should NOT emit TS1354 for 'readonly string[]'"
    );
}

#[test]
fn test_readonly_on_tuple_type_no_ts1354() {
    let source = r"
type T = readonly [string, number];
";
    assert!(
        !has_error_with_code(source, 1354),
        "Should NOT emit TS1354 for 'readonly [string, number]'"
    );
}

#[test]
fn test_omit_preserves_readonly_modifier() {
    // Omit<T, K> is a homomorphic mapped type that preserves readonly modifiers
    // from the source type. Assigning to a readonly property through Omit should
    // still emit TS2540.
    let source = r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

type A = {
    a: number;
    b?: string;
    readonly c: boolean;
    d: unknown;
};

type B = Omit<A, 'a'>;

function f(x: B) {
    x.c = true;
}
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to readonly property 'c' through Omit type"
    );
}

#[test]
fn test_readonly_on_type_reference_emits_ts1354() {
    // readonly Array<string> should emit TS1354 — use ReadonlyArray<string> instead
    let source = r"
type T = readonly Array<string>;
";
    assert!(
        has_error_with_code(source, 1354),
        "Should emit TS1354 for 'readonly Array<string>'"
    );
}
