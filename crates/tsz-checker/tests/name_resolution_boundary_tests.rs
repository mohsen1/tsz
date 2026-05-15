//! Regression tests for the unified name resolution boundary
//! (`query_boundaries/name_resolution.rs`).
//!
//! Covers the target diagnostic families:
//! - TS2304 (cannot find name)
//! - TS2552 (spelling suggestion)
//! - TS2694 (namespace has no exported member)
//! - TS2708 (cannot use namespace as a value)
//! - TS2693 (type used as value)
//! - TS2724 (namespace export spelling suggestion)
//! - TS2749 (value used as type)

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    diagnostic_codes, diagnostic_count, diagnostics_where, diagnostics_with_code,
    has_diagnostic_code, has_diagnostic_code_where,
};

fn check(source: &str) -> Vec<Diagnostic> {
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn check_named_files(files: &[(&str, &str)], entry_file: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_multi_file(files, entry_file, CheckerOptions::default())
        .into_iter()
        .filter(|d| d.code != 2318)
        .collect()
}

fn expect_diagnostic_code<'a>(diags: &'a [Diagnostic], code: u32, message: &str) -> &'a Diagnostic {
    diags.iter().find(|d| d.code == code).expect(message)
}

// =========================================================================
// TS2693: Type used as value (routes through boundary)
// =========================================================================

#[test]
fn ts2693_type_alias_used_as_value() {
    let diags = check(
        r#"
type Foo = string;
let x = Foo;
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2693),
        "Expected TS2693 for type alias used as value, got: {diags:?}"
    );
}

#[test]
fn ts2693_interface_used_as_value() {
    let diags = check(
        r#"
interface Bar { x: number; }
let y = Bar;
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2693),
        "Expected TS2693 for interface used as value, got: {diags:?}"
    );
}

#[test]
fn ts2693_not_for_class() {
    let diags = check(
        r#"
class Baz {}
let z = Baz;
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2693),
        "Should not emit TS2693 for class (which is also a value), got: {diags:?}"
    );
}

#[test]
fn ts2693_not_for_merged_type_and_value() {
    let diags = check(
        r#"
type FAILURE = "FAILURE";
const FAILURE = "FAILURE";
let x = FAILURE;
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2693),
        "Should not emit TS2693 when value shadows type alias, got: {diags:?}"
    );
}

// =========================================================================
// TS2708: Cannot use namespace as a value
// =========================================================================

#[test]
fn ts2708_uninstantiated_namespace_as_value() {
    let diags = check(
        r#"
namespace MyNs {
    export type Foo = string;
}
let x = MyNs;
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2708),
        "Expected TS2708 for uninstantiated namespace used as value, got: {diags:?}"
    );
}

#[test]
fn ts2708_not_for_instantiated_namespace() {
    let diags = check(
        r#"
namespace MyNs {
    export const value = 42;
}
let x = MyNs;
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2708),
        "Should not emit TS2708 for instantiated namespace, got: {diags:?}"
    );
}

#[test]
fn ts2708_plain_import_equals_require_to_uninstantiated_export_equals_namespace() {
    let diags = check_named_files(
        &[
            (
                "decl.ts",
                r#"
declare module "foo" {
    namespace B {
        export interface A {}
    }
    interface B {
        bar(name: string): B.A;
    }
    export = B;
}
"#,
            ),
            (
                "use.ts",
                r#"
import foo = require("foo");
declare var z: foo;
z.bar("hello");
var x: foo.A = foo.bar("hello");
"#,
            ),
        ],
        "use.ts",
    );
    let ts2708_count = diagnostic_count(&diags, 2708);
    assert_eq!(
        ts2708_count, 1,
        "Expected exactly one TS2708 for value access through the import alias, got: {diags:?}"
    );
}

// =========================================================================
// TS2749: Value used as type
// =========================================================================

#[test]
fn ts2749_value_used_as_type() {
    let diags = check(
        r#"
const myVal = 42;
let x: myVal;
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2749),
        "Expected TS2749 for value used as type, got: {diags:?}"
    );
}

fn assert_missing_name_positions(source: &str, starts: &[u32]) {
    let diags = check(source);
    let actual: Vec<u32> = diagnostics_with_code(&diags, 2304)
        .iter()
        .map(|d| d.start)
        .collect();
    assert_eq!(
        actual, starts,
        "Expected TS2304 anchors {starts:?}, got {diags:?}"
    );
}

#[test]
fn ts2304_class_generic_constraint_reports_nested_type_args() {
    assert_missing_name_positions("class C<T extends List<List>> {}", &[18, 23]);
}

#[test]
fn ts2304_function_generic_constraint_reports_nested_type_args() {
    assert_missing_name_positions("function f<T extends List<List>>() {}", &[21, 26]);
}

#[test]
fn ts2304_class_expression_generic_constraint_reports_nested_type_args() {
    assert_missing_name_positions("const C = class<T extends List<List>> {};", &[26, 31]);
}

#[test]
fn ts2304_function_expression_generic_constraint_reports_nested_type_args() {
    assert_missing_name_positions("const f = function<T extends List<List>>() {};", &[29, 34]);
}

// =========================================================================
// TS2694: Namespace has no exported member (routed through boundary)
// =========================================================================

#[test]
fn ts2694_namespace_missing_export() {
    let diags = check(
        r#"
namespace MyNs {
    export type Foo = string;
}
let x: MyNs.Bar;
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2694),
        "Expected TS2694 for missing namespace export, got: {diags:?}"
    );
}

#[test]
fn ts2694_not_for_existing_export() {
    let diags = check(
        r#"
namespace MyNs {
    export type Foo = string;
}
let x: MyNs.Foo;
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 2694),
        "Should not emit TS2694 for existing namespace export, got: {diags:?}"
    );
}

#[test]
fn ts2694_qualified_namespace_lookup_does_not_fall_back_to_file_exports() {
    let diags = check_named_files(
        &[
            ("a.ts", "declare namespace X { export interface bar {} }"),
            (
                "b.ts",
                r#"
declare namespace X { export interface foo {} }
export { X };
export declare function foo(): X.foo;
export declare function bar(): X.bar;
"#,
            ),
        ],
        "b.ts",
    );
    assert!(
        has_diagnostic_code(&diags, 2694),
        "Expected TS2694 for missing namespace export, got: {diags:?}"
    );
    assert!(
        !has_diagnostic_code(&diags, 2749),
        "File-level exports should not leak into namespace member lookup, got: {diags:?}"
    );
}

// =========================================================================
// TS2724: Namespace export spelling suggestion (routed through boundary)
// =========================================================================

#[test]
fn ts2724_namespace_export_spelling_suggestion() {
    let diags = check(
        r#"
namespace MyNs {
    export type MyType = string;
    export type MyOtherType = number;
}
let x: MyNs.MyTyp;
"#,
    );
    let has_ns_error = has_diagnostic_code_where(&diags, |code| code == 2694 || code == 2724);
    assert!(
        has_ns_error,
        "Expected TS2694 or TS2724 for misspelled namespace export, got: {diags:?}"
    );
}

// =========================================================================
// Cross-concern: type vs value distinction
// =========================================================================

#[test]
fn type_value_merged_class_no_errors() {
    let diags = check(
        r#"
class Pair { constructor(public a: number, public b: number) {} }
let p = new Pair(1, 2);
let t: Pair;
"#,
    );
    let type_value_errors = diagnostics_where(&diags, |code| code == 2693 || code == 2749);
    assert!(
        type_value_errors.is_empty(),
        "Class should not produce type/value errors, got: {type_value_errors:?}"
    );
}

#[test]
fn enum_is_both_type_and_value() {
    let diags = check(
        r#"
enum Direction { Up, Down, Left, Right }
let d = Direction.Up;
let t: Direction;
"#,
    );
    let type_value_errors = diagnostics_where(&diags, |code| matches!(code, 2693 | 2749 | 2708));
    assert!(
        type_value_errors.is_empty(),
        "Enum should not produce type/value errors, got: {type_value_errors:?}"
    );
}

// =========================================================================
// Boundary: report_wrong_meaning routing tests
// =========================================================================

#[test]
fn phase2_type_alias_in_value_routes_through_report_wrong_meaning() {
    let diags = check(
        r#"
type StringAlias = string;
let v = StringAlias;
"#,
    );
    let ts2693_count = diagnostic_count(&diags, 2693);
    assert!(
        ts2693_count == 1,
        "Expected exactly 1 TS2693, got {ts2693_count}: {diags:?}"
    );
}

#[test]
fn phase2_namespace_as_value_routes_through_report_wrong_meaning() {
    let diags = check(
        r#"
namespace PureTypeNs {
    export interface I {}
}
let v = PureTypeNs;
"#,
    );
    let ts2708_count = diagnostic_count(&diags, 2708);
    assert!(
        ts2708_count == 1,
        "Expected exactly 1 TS2708, got {ts2708_count}: {diags:?}"
    );
}

#[test]
fn phase2_value_only_in_type_routes_through_boundary_in_type_literal() {
    let diags = check(
        r#"
function myFunc() { return 1; }
type T = { x: myFunc };
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2749),
        "Expected TS2749 for function used as type in type literal, got: {diags:?}"
    );
}

#[test]
fn phase2_value_only_in_qualified_type_routes_through_boundary() {
    let diags = check(
        r#"
namespace NS {
    export const val = 42;
}
let x: NS.val;
"#,
    );
    assert!(
        has_diagnostic_code_where(&diags, |code| code == 2749 || code == 2694),
        "Expected TS2749 or TS2694 for value-only qualified name, got: {diags:?}"
    );
}

#[test]
fn merged_value_namespace_reexport_anchors_nested_type_member() {
    let diags = check(
        r#"
type A = number;
declare const Q: number;
declare namespace Q {
    export { A };
}
declare namespace Q2 {
    export { Q };
}
declare const tryMember: Q2.Q.A;
export {};
"#,
    );

    let ts2749_count = diagnostic_count(&diags, 2749);
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 when a re-exported value/namespace merge anchors a nested type member, got: {diags:?}"
    );
}

#[test]
fn merged_value_namespace_reexport_final_type_still_errors() {
    let diags = check(
        r#"
type A = number;
declare const Q: number;
declare namespace Q {
    export { A };
}
declare namespace Q2 {
    export { Q };
}
declare const tryAnchor: Q2.Q;
export {};
"#,
    );

    let ts2749_count = diagnostic_count(&diags, 2749);
    assert_eq!(
        ts2749_count, 1,
        "Expected TS2749 when the merged value/namespace export is used as the final type, got: {diags:?}"
    );
}

// =========================================================================
// Boundary: Type-position suggestion collection through boundary
// =========================================================================

#[test]
fn phase2_type_position_not_found_goes_through_boundary() {
    let diags = check(
        r#"
let x: UnknownTypeName;
"#,
    );
    assert!(
        has_diagnostic_code_where(&diags, |code| code == 2304 || code == 2552),
        "Expected TS2304/TS2552 for unknown type name, got: {diags:?}"
    );
}

// =========================================================================
// Boundary: No duplicate diagnostics after migration
// =========================================================================

#[test]
fn phase2_no_double_diagnostic_for_interface_as_value() {
    let diags = check(
        r#"
interface IFoo { x: number; }
let v = IFoo;
"#,
    );
    let ts2693_count = diagnostic_count(&diags, 2693);
    assert!(
        ts2693_count <= 1,
        "Should emit at most 1 TS2693, got {ts2693_count}: {diags:?}"
    );
}

#[test]
fn phase2_no_double_diagnostic_for_namespace_as_value() {
    let diags = check(
        r#"
namespace NS {
    export type T = string;
}
let v = NS;
"#,
    );
    let ts2708_count = diagnostic_count(&diags, 2708);
    assert!(
        ts2708_count <= 1,
        "Should emit at most 1 TS2708, got {ts2708_count}: {diags:?}"
    );
}

#[test]
fn phase2_no_double_diagnostic_for_value_as_type() {
    let diags = check(
        r#"
const myVal = 42;
let x: myVal;
"#,
    );
    let ts2749_count = diagnostic_count(&diags, 2749);
    assert!(
        ts2749_count <= 1,
        "Should emit at most 1 TS2749, got {ts2749_count}: {diags:?}"
    );
}

// =========================================================================
// Boundary: boundary_known_value_no_diagnostic
// =========================================================================

#[test]
fn boundary_known_value_no_diagnostic() {
    let diags = check(
        r#"
const knownValue = 42;
function test() {
    return knownValue;
}
"#,
    );
    let relevant = diagnostics_where(&diags, |code| matches!(code, 2304 | 2693 | 2749));
    assert!(
        relevant.is_empty(),
        "Expected no name resolution errors for known value, got: {relevant:?}"
    );
}

#[test]
fn boundary_namespace_member_access_works() {
    let diags = check(
        r#"
namespace NS {
    export const val = 42;
}
let x = NS.val;
"#,
    );
    let relevant = diagnostics_where(&diags, |code| matches!(code, 2694 | 2708 | 2724));
    assert!(
        relevant.is_empty(),
        "Valid namespace member access should not produce errors, got: {relevant:?}"
    );
}

#[test]
fn boundary_nested_namespace_export_missing() {
    let diags = check(
        r#"
namespace Outer {
    export namespace Inner {
        export type Exists = number;
    }
}
let x: Outer.Inner.DoesNotExist;
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2694),
        "Expected TS2694 for missing nested namespace export, got: {diags:?}"
    );
}

#[test]
fn boundary_string_module_parent_does_not_qualify_namespace_ts2694() {
    let diags = check(
        r#"
declare namespace X { export interface bar { } }
declare module "m" {
    namespace X { export interface foo { } }
    export { X };
    export function foo(): X.foo;
    export function bar(): X.bar;
}
"#,
    );

    let ts2694 =
        expect_diagnostic_code(&diags, 2694, "Expected TS2694 for missing namespace export");

    assert!(
        ts2694
            .message_text
            .contains("Namespace 'X' has no exported member 'bar'."),
        "Expected TS2694 to use the local namespace name, got: {ts2694:?}"
    );
    assert!(
        !ts2694.message_text.contains("m.X"),
        "TS2694 should not qualify namespaces through string-literal modules: {ts2694:?}"
    );
}

#[test]
fn boundary_external_module_parent_qualifies_namespace_ts2694() {
    let diags = check(
        r#"
export namespace Promise {}
let x: Promise.Resolver<string>;
"#,
    );

    let ts2694 =
        expect_diagnostic_code(&diags, 2694, "Expected TS2694 for missing namespace export");

    assert!(
        ts2694
            .message_text
            .contains("Namespace '\"test\".Promise' has no exported member 'Resolver'."),
        "Expected TS2694 to qualify exported external-module namespace, got: {ts2694:?}"
    );
}

#[test]
fn boundary_external_module_nested_export_qualifies_root_namespace_ts2694() {
    let diags = check(
        r#"
export namespace Outer {
    export namespace Inner {
        export type Exists = number;
    }
}
let x: Outer.Inner.DoesNotExist;
"#,
    );

    let ts2694 = expect_diagnostic_code(
        &diags,
        2694,
        "Expected TS2694 for missing nested namespace export",
    );

    assert!(
        ts2694
            .message_text
            .contains("Namespace '\"test\".Outer.Inner' has no exported member 'DoesNotExist'."),
        "Expected TS2694 to qualify nested exported namespaces through the file module name, got: {ts2694:?}"
    );
}

#[test]
fn boundary_external_module_local_namespace_does_not_gain_file_prefix_ts2694() {
    let diags = check(
        r#"
namespace foo {
    export namespace bar {
        export namespace baz {
            export class boo {}
        }
    }
}
import booz = foo.bar.baz;
let x: booz.bar;
"#,
    );

    let ts2694 =
        expect_diagnostic_code(&diags, 2694, "Expected TS2694 for missing namespace export");

    assert!(
        ts2694
            .message_text
            .contains("Namespace 'foo.bar.baz' has no exported member 'bar'."),
        "Expected local module namespace chains to stay unqualified, got: {ts2694:?}"
    );
    assert!(
        !ts2694.message_text.contains("\"test\"."),
        "Local namespaces should not be prefixed with the file module name: {ts2694:?}"
    );
}

// =========================================================================
// Boundary: Wrong-meaning migration through boundary
// =========================================================================

/// Primitive keyword types used as values route through wrong-meaning boundary
#[test]
fn phase2_primitive_keyword_type_as_value_routes_through_boundary() {
    // NOTE: "void" and "undefined" excluded — they are keyword tokens with
    // special parsing rules, not identifiers in `const x = void` context.
    // "null" excluded — it's a valid value (NullKeyword).
    for keyword in &[
        "number", "string", "boolean", "any", "unknown", "never", "object", "bigint",
    ] {
        let src = format!("const x = {keyword};");
        let diags = check(&src);
        assert!(
            has_diagnostic_code(&diags, 2693),
            "Expected TS2693 for '{keyword}' used as value, got: {:?}",
            diagnostic_codes(&diags)
        );
        // Should not emit TS2304 alongside TS2693
        assert!(
            !has_diagnostic_code(&diags, 2304),
            "Should not emit TS2304 alongside TS2693 for '{keyword}', got: {:?}",
            diagnostic_codes(&diags)
        );
    }
}

/// Value-only symbol in type position routes through wrong-meaning boundary (TS2749)
#[test]
fn phase2_value_only_type_routes_through_boundary_in_return_type() {
    let diags = check(
        r#"
const myVal = "hello";
function f(): myVal { return ""; }
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 2749),
        "Expected TS2749 for value used as return type, got: {:?}",
        diagnostic_codes(&diags)
    );
}

/// Keyword type in `new` expression routes through wrong-meaning boundary
#[test]
fn phase2_keyword_type_in_new_routes_through_boundary() {
    let diags = check("const x = new string();");
    assert!(
        has_diagnostic_code(&diags, 2693),
        "Expected TS2693 for 'new string()', got: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn phase2_unresolved_new_target_reports_ts2304_not_ts2693() {
    let diags = check("new A().b();");
    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for unresolved constructor target, got: {diags:?}"
    );
    assert!(
        !codes.contains(&2693),
        "Should not emit TS2693 for unresolved constructor target, got: {diags:?}"
    );
}

#[test]
fn new_target_is_allowed_in_constructor_body() {
    let diags = check(
        r#"
class C {
    constructor() {
        new.target;
    }
}
"#,
    );
    assert!(
        !has_diagnostic_code(&diags, 17013),
        "Did not expect TS17013 for new.target inside a constructor, got: {diags:?}"
    );
}

#[test]
fn new_target_in_method_reports_ts17013() {
    let diags = check(
        r#"
class C {
    method() {
        new.target;
    }
}
"#,
    );
    assert!(
        has_diagnostic_code(&diags, 17013),
        "Expected TS17013 for new.target inside a method, got: {diags:?}"
    );
}

#[test]
fn new_target_uses_function_expando_properties() {
    let diags = check(
        r#"
function foo(x: true) { }

function f() {
  if (new.target.marked === true) {
    foo(new.target.marked);
  }
}

f.marked = true;
"#,
    );
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for new.target expando property narrowing, got: {diags:?}"
    );
}

/// Heritage clause with unresolved name routes through boundary for suggestions
#[test]
fn phase2_heritage_unresolved_routes_through_boundary() {
    let diags = check(
        r#"
class Base {}
class Derived extends Bace {}
"#,
    );
    // Should get TS2304 or TS2552 (spelling suggestion for "Base" -> "Bace")
    assert!(
        has_diagnostic_code_where(&diags, |code| code == 2304 || code == 2552),
        "Expected TS2304/TS2552 for misspelled heritage name, got: {:?}",
        diagnostic_codes(&diags)
    );
}

/// `typeof default` emits TS2304 through boundary
#[test]
fn phase2_typeof_default_routes_through_boundary() {
    let diags = check("type T = typeof default;");
    assert!(
        has_diagnostic_code(&diags, 2304),
        "Expected TS2304 for 'typeof default', got: {:?}",
        diagnostic_codes(&diags)
    );
}

/// Suggestion suppression: accessibility modifiers don't get TS2552
#[test]
fn phase2_suggestion_suppression_for_accessibility_modifiers() {
    // "private" used as identifier — should get TS2304, not TS2552
    let diags = check(
        r#"
function f() {
    return private;
}
"#,
    );
    // Should not produce TS2552 suggestions for "private"
    let has_ts2552 = has_diagnostic_code(&diags, 2552);
    // Either TS2304 or no error (strict mode may produce different diagnostics)
    assert!(
        !has_ts2552,
        "Should not emit TS2552 for 'private', got: {:?}",
        diagnostic_codes(&diags)
    );
}

/// No double diagnostics after migration: globalThis property type-only
#[test]
fn phase2_no_double_diagnostic_for_global_type_only() {
    let diags = check(
        r#"
interface Foo { x: number; }
const a = Foo;
"#,
    );
    let ts2693_count = diagnostic_count(&diags, 2693);
    assert!(
        ts2693_count <= 1,
        "Should emit at most 1 TS2693 for interface as value, got {ts2693_count}: {:?}",
        diagnostic_codes(&diags)
    );
}
