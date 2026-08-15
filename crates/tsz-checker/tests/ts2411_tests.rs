//! Tests for TS2411: Property type not assignable to index signature type
//!
//! Verifies that getter/setter accessors are checked against index signatures.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source, check_source_code_messages as get_diagnostics, check_source_diagnostics,
    diagnostic_code_messages,
};

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

#[test]
fn local_symbol_property_access_computed_name_is_string_keyed() {
    let source = r#"
const Symbol = { tag: "name" } as const;

interface Bag {
    [key: string]: number;
    [Symbol.tag]: string;
}
"#;
    let diagnostics = get_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2411 && message.contains("[Symbol.tag]") && message.contains("number")
        }),
        "expected TS2411 because local Symbol.tag is a string-keyed computed property, got: {diagnostics:?}"
    );
}

// =========================================================================
// Getter without type annotation vs string index signature
// =========================================================================

#[test]
fn test_getter_no_annotation_string_index_class() {
    // Getter returns boolean, string index requires string
    let source = r#"
class Foo {
    [key: string]: string;
    get bar() { return true; }
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for getter returning boolean vs string index"
    );
}

#[test]
fn test_getter_no_annotation_string_index_interface() {
    let source = r#"
interface Foo {
    [key: string]: string;
    get bar(): boolean;
}
"#;
    // Interface getters always have type annotation in syntax, so this uses the annotation path
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for interface getter with mismatched return type"
    );
}

// =========================================================================
// Getter without type annotation vs number index signature
// =========================================================================

#[test]
fn test_getter_no_annotation_number_index() {
    let source = r#"
class Foo {
    [key: number]: string;
    get 0() { return 42; }
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for numeric getter returning number vs number index string"
    );
}

// =========================================================================
// Getter with explicit type annotation (should still work)
// =========================================================================

#[test]
fn test_getter_with_annotation_mismatch() {
    let source = r#"
class Foo {
    [key: string]: string;
    get bar(): number { return 42; }
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for getter with explicit return type mismatch"
    );
}

#[test]
fn test_getter_with_annotation_compatible() {
    let source = r#"
class Foo {
    [key: string]: string;
    get bar(): string { return "hello"; }
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Should NOT emit TS2411 when getter return type matches index signature"
    );
}

// =========================================================================
// Getter without annotation, compatible return type
// =========================================================================

#[test]
fn test_getter_no_annotation_compatible() {
    let source = r#"
class Foo {
    [key: string]: string;
    get bar() { return "hello"; }
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Should NOT emit TS2411 when inferred getter return type matches index signature"
    );
}

// =========================================================================
// Setter parameter type vs index signature
// =========================================================================

#[test]
fn test_setter_with_annotation_mismatch() {
    let source = r#"
class Foo {
    [key: string]: string;
    set bar(val: number) {}
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for setter with mismatched parameter type"
    );
}

#[test]
fn test_setter_with_annotation_compatible() {
    let source = r#"
class Foo {
    [key: string]: string;
    set bar(val: string) {}
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Should NOT emit TS2411 when setter parameter type matches index signature"
    );
}

// =========================================================================
// Method signature vs index signature (interface)
// =========================================================================

#[test]
fn test_method_signature_vs_index_signature() {
    // Method bar():any has function type () => any, which is not assignable to number
    let source = r#"
interface Foo {
    bar(): any;
    [s: string]: number;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411),
        "Should emit TS2411 for method signature type not assignable to index, got: {diags:?}"
    );
}

#[test]
fn test_method_declaration_vs_index_signature() {
    // Class method bar():any has function type () => any, not assignable to number
    let source = r#"
class Foo {
    bar(): any { return 1; }
    [s: string]: number;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411),
        "Should emit TS2411 for class method type not assignable to index, got: {diags:?}"
    );
}

// =========================================================================
// Type literal (object type) members vs index signature
// =========================================================================

#[test]
fn test_type_literal_property_vs_index_signature() {
    let source = r#"
interface I { k: any; }
var x: { z: I; [s: string]: { x: any; y: any; } };
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411),
        "Should emit TS2411 for type literal property not assignable to index, got: {diags:?}"
    );
}

#[test]
fn numeric_literal_property_name_keeps_source_spelling_against_number_index() {
    // tsc prints the numeric-literal property's source spelling (`2.0`) in the
    // TS2411 message, not the canonicalized numeric name (`2`) used for index
    // lookup -- see numericIndexerConstrainsPropertyDeclarations.ts.
    let source = r#"
class C {
    [x: number]: string;
    2.0: number;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags
            .iter()
            .any(|d| d.0 == 2411 && d.1.contains("Property '2.0' of type 'number'")),
        "Should print the source-spelled numeric literal name '2.0', got: {diags:?}"
    );
}

#[test]
fn numeric_literal_property_name_keeps_source_spelling_against_string_index() {
    let source = r#"
interface I {
    [x: string]: string;
    2.0: number;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags
            .iter()
            .any(|d| d.0 == 2411 && d.1.contains("Property '2.0' of type 'number'")),
        "Should print the source-spelled numeric literal name '2.0', got: {diags:?}"
    );
}

#[test]
fn plain_integer_property_name_is_unaffected_by_numeric_literal_spelling() {
    // Negative control: an integer literal with no fractional spelling still
    // renders as its plain digits, not accidentally re-quoted or altered.
    let source = r#"
class C {
    [x: number]: string;
    2: number;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags
            .iter()
            .any(|d| d.0 == 2411 && d.1.contains("Property '2' of type 'number'")),
        "Should print the plain integer name '2', got: {diags:?}"
    );
}

#[test]
fn type_literal_in_generic_type_argument_checks_property_against_index_signature() {
    let source = r#"
type KeysOfIndex<T> = keyof T;

type KI = KeysOfIndex<{ [key: string]: number; a: boolean }>;
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411
            && d.1.contains("Property 'a' of type 'boolean'")
            && d.1.contains("index type 'number'")),
        "Should emit TS2411 for type literal generic argument property not assignable to index, got: {diags:?}"
    );
}

#[test]
fn test_type_literal_union_function_property_vs_index_signature() {
    let source = r#"
function test(arg: string | number, whatever: any) {
  if (typeof arg === "string") {
    const o: { [k: string]: () => typeof arg; x: (() => boolean) | (() => void) } = whatever;
  }
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411
            && d.1.contains("Property 'x' of type")
            && d.1.contains("index type '() => string | number'")),
        "Should emit TS2411 for union function property not assignable to index, got: {diags:?}"
    );
}

// =========================================================================
// Inherited member vs index signature: computed-name display (#16866)
//
// tsc's `declarationNameToString` renders a computed member name
// (`["get1"]`) as verbatim source text. tsz's own-member TS2411 path already
// did this (`diag_prop_name`), but the INHERITED-member path
// (`check_inherited_properties_against_index_signatures`) used the bare
// resolved property name instead, dropping the brackets for a member
// inherited from a base class/interface.
// =========================================================================

#[test]
fn test_ts2411_inherited_computed_name_class_keeps_brackets() {
    let source = r#"
class Foo { x: number = 0; }
class Foo2 { x: number = 0; y: number = 0; }
class C {
    get ["get1"]() { return new Foo(); }
}
class D extends C {
    [s: string]: Foo2;
}
"#;
    let diags = get_diagnostics(source);
    let ts2411 = diags
        .iter()
        .find(|d| d.0 == 2411)
        .expect("expected TS2411 for inherited computed-name getter vs string index");
    assert!(
        ts2411.1.contains("[\"get1\"]"),
        "TS2411 must render the inherited computed name as `[\"get1\"]` (tsc's \
         declarationNameToString), got: {}",
        ts2411.1
    );
}

#[test]
fn test_ts2411_inherited_plain_identifier_has_no_brackets() {
    // Adjacent case: a plain (non-computed) inherited member name must NOT
    // gain brackets -- only computed names get the verbatim-source treatment.
    let source = r#"
class Foo { x: number = 0; }
class Foo2 { x: number = 0; y: number = 0; }
class C {
    get plainGetter() { return new Foo(); }
}
class D extends C {
    [s: string]: Foo2;
}
"#;
    let diags = get_diagnostics(source);
    let ts2411 = diags
        .iter()
        .find(|d| d.0 == 2411)
        .expect("expected TS2411 for inherited plain getter vs string index");
    assert!(
        ts2411.1.contains("Property 'plainGetter'"),
        "TS2411 for a plain inherited identifier must not gain brackets, got: {}",
        ts2411.1
    );
}

#[test]
fn test_ts2411_inherited_computed_name_interface_keeps_brackets() {
    // Adjacent case: the same fix applies to `interface extends`, not just
    // `class extends` (the shared function handles both declaration kinds).
    let source = r#"
class Foo { x: number = 0; }
class Foo2 { x: number = 0; y: number = 0; }
interface IBase {
    ["ifaceGet"]: Foo;
}
interface IDerived extends IBase {
    [s: string]: Foo2;
}
"#;
    let diags = get_diagnostics(source);
    let ts2411 = diags
        .iter()
        .find(|d| d.0 == 2411)
        .expect("expected TS2411 for inherited interface computed name vs string index");
    assert!(
        ts2411.1.contains("[\"ifaceGet\"]"),
        "TS2411 must render the inherited interface computed name as \
         `[\"ifaceGet\"]`, got: {}",
        ts2411.1
    );
}

#[test]
fn test_ts2411_inherited_computed_name_multi_level_class_chain() {
    // Adjacent case: the computed name lives two `extends` hops up
    // (D -> C -> B), exercising the heritage-walk recursion.
    let source = r#"
class Foo { x: number = 0; }
class Foo2 { x: number = 0; y: number = 0; }
class B {
    get ["deep"]() { return new Foo(); }
}
class C extends B {}
class D extends C {
    [s: string]: Foo2;
}
"#;
    let diags = get_diagnostics(source);
    let ts2411 = diags
        .iter()
        .find(|d| d.0 == 2411)
        .expect("expected TS2411 for a two-level-inherited computed name vs string index");
    assert!(
        ts2411.1.contains("[\"deep\"]"),
        "TS2411 must render the two-level-inherited computed name as \
         `[\"deep\"]`, got: {}",
        ts2411.1
    );
}

// =========================================================================
// Inherited member vs index signature: `TS2728` "declared here" pointer
//
// tsc reports this TS2411 at the index signature the DERIVED type owns, not
// at the property (owned by a base) -- so it attaches a `'{0}' is declared
// here.` (TS2728) related-information entry pointing back at the base
// declaration. The own-member TS2411 path needs no such pointer: the report
// site already IS the declaration.
// =========================================================================

#[test]
fn test_ts2411_inherited_computed_name_has_declared_here_pointer() {
    let source = r#"
class Foo { x: number = 0; }
class Foo2 { x: number = 0; y: number = 0; }
class C {
    get ["get1"]() { return new Foo(); }
}
class D extends C {
    [s: string]: Foo2;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2411 = diagnostics
        .iter()
        .find(|d| d.code == 2411)
        .expect("expected TS2411 for inherited computed-name getter vs string index");
    let declared_here = ts2411
        .related_information
        .iter()
        .find(|r| r.code == 2728)
        .expect("expected a TS2728 'declared here' pointer on the inherited TS2411");
    assert!(
        declared_here.is_location_pointer(),
        "the TS2728 entry must be a cross-location pointer, not an elaboration chain link"
    );
    assert!(
        declared_here.message_text.contains("[\"get1\"]"),
        "TS2728 must name the property with tsc's verbatim computed-name text, got: {}",
        declared_here.message_text
    );
}

#[test]
fn test_ts2411_inherited_plain_identifier_has_declared_here_pointer() {
    // Adjacent case: the pointer is a property of being INHERITED, not of
    // having a computed name -- a plain identifier gets it too.
    let source = r#"
class Foo { x: number = 0; }
class Foo2 { x: number = 0; y: number = 0; }
class C {
    get plainGetter() { return new Foo(); }
}
class D extends C {
    [s: string]: Foo2;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2411 = diagnostics
        .iter()
        .find(|d| d.code == 2411)
        .expect("expected TS2411 for inherited plain getter vs string index");
    let declared_here = ts2411
        .related_information
        .iter()
        .find(|r| r.code == 2728)
        .expect("expected a TS2728 'declared here' pointer on the inherited TS2411");
    assert!(
        declared_here.message_text.contains("'plainGetter'"),
        "TS2728 must name the plain inherited identifier without brackets, got: {}",
        declared_here.message_text
    );
}

#[test]
fn test_ts2411_own_member_has_no_declared_here_pointer() {
    // Negative control: an OWN-member TS2411 (no inheritance involved) must
    // NOT gain a TS2728 pointer -- the report site already is the
    // declaration, so tsc attaches no related information there.
    let source = r#"
class Foo { x: number = 0; }
interface I {
    [s: string]: Foo;
    ["own1"]: number;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2411 = diagnostics
        .iter()
        .find(|d| d.code == 2411)
        .expect("expected TS2411 for own computed-name member vs string index");
    assert!(
        !ts2411.related_information.iter().any(|r| r.code == 2728),
        "an own-member TS2411 must carry no TS2728 pointer, got: {:?}",
        ts2411.related_information
    );
}

#[test]
fn test_ts2411_method_overload_displays_merged_signatures() {
    // When an interface method has multiple overload signatures, the TS2411
    // message must render the property's type as `{ (): any; (): any; }`
    // (matching tsc) instead of just the first signature's `() => any`.
    // Regression test for interfaceMemberValidation.ts.
    let source = r#"
interface foo {
    bar(): any;
    bar(): any;
    [s: string]: number;
}
"#;
    let diags = get_diagnostics(source);
    let ts2411 = diags
        .iter()
        .find(|d| d.0 == 2411)
        .expect("expected TS2411 for `bar` overloads vs string index");
    assert!(
        ts2411.1.contains("{ (): any; (): any; }"),
        "TS2411 must render merged overload type as `{{ (): any; (): any; }}`, got: {}",
        ts2411.1
    );
}

// =========================================================================
// Issue #2871: a local object named `Symbol` must not be treated as the lib
// global `Symbol` when classifying `[Symbol.tag]` as a symbol-keyed property.
// With a `[s: symbol]: number` index signature present, the buggy
// classification routes the `[Symbol.tag]: string` member into the symbol
// index check and emits TS2411 ("string not assignable to 'symbol' index
// type 'number'"). After the fix the local `Symbol` is recognized as a
// shadow, the member is not symbol-keyed, and that diagnostic must not fire.
// =========================================================================

#[test]
fn ts2411_shadowed_symbol_computed_property_is_not_symbol_keyed() {
    let source = r#"
const Symbol = { tag: "name" } as const;

interface Bag {
    [s: symbol]: number;
    [Symbol.tag]: string;
}
"#;
    let ts2411_against_symbol = get_diagnostics(source)
        .into_iter()
        .filter(|d| d.0 == 2411 && d.1.contains("'symbol'"))
        .count();
    assert_eq!(
        ts2411_against_symbol, 0,
        "Expected no symbol-index TS2411 when local Symbol shadows the global, got: {ts2411_against_symbol}"
    );
}

// =========================================================================
// Optional properties in interfaces must be checked as `T | undefined`
// against the index signature (TS2411, issue #6746).
//
// tsc rule: an optional property `prop?: T` has effective type `T | undefined`
// for index-signature compatibility because the property can be absent.
// If `T | undefined` is not assignable to the index value type, TS2411 fires.
// =========================================================================

#[test]
fn ts2411_interface_optional_property_vs_string_index() {
    // `optional?: string` is effectively `string | undefined`.
    // `string | undefined` is not assignable to `string` index value → TS2411.
    let source = r#"
interface WithSpecific {
    [key: string]: string;
    required: string;
    optional?: string;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411: optional property `string | undefined` not assignable to string index"
    );
}

#[test]
fn ts2411_interface_required_property_no_false_positive() {
    // Required `required: string` IS assignable to `string` index → no TS2411.
    let source = r#"
interface WithSpecific {
    [key: string]: string;
    required: string;
}
export {};
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Required property should not trigger TS2411 vs matching string index"
    );
}

#[test]
fn ts2411_interface_optional_property_index_includes_undefined() {
    // When the index value type already includes `undefined`, the optional
    // property's `T | undefined` IS assignable → no TS2411.
    let source = r#"
interface WithUndefined {
    [key: string]: string | undefined;
    optional?: string;
}
export {};
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Optional property should not trigger TS2411 when index type already includes undefined"
    );
}

#[test]
fn ts2411_interface_optional_property_exact_optional_no_undefined_widening() {
    // With exactOptionalPropertyTypes, `optional?: string` is exactly `string` (the optional
    // marker is "missing", not `undefined`), so it is assignable to the `string` index type and
    // tsc reports NO TS2411. Without EOP the same property reads as `string | undefined` and
    // does conflict — that case is covered by ts2411_interface_optional_property_vs_string_index.
    // Verified against tsc 6.0.2: `--strict --exactOptionalPropertyTypes` reports no error here.
    let source = r#"
interface ExactOptional {
    [key: string]: string;
    optional?: string;
}
export {};
"#;
    let diagnostics = diagnostic_code_messages(check_source(
        source,
        "test.ts",
        CheckerOptions {
            exact_optional_property_types: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    ));
    assert!(
        !diagnostics.iter().any(|d| d.0 == 2411),
        "exactOptionalPropertyTypes makes optional?: string exactly string, assignable to the string index — no TS2411 expected, got: {diagnostics:?}"
    );
}

#[test]
fn ts2411_interface_optional_number_index() {
    // Optional numeric property vs number index signature.
    let source = r#"
interface NumericIndex {
    [idx: number]: string;
    0?: string;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 for optional numeric property vs number index"
    );
}

#[test]
fn ts2411_class_optional_property_vs_string_index() {
    // Class optional property must also include `undefined` for the check.
    let source = r#"
class MyClass {
    [key: string]: string;
    optional?: string;
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 for class optional property vs string index"
    );
}

#[test]
fn ts2411_interface_optional_property_renamed_key_var() {
    // Rename the index-signature iteration variable to prove no hardcoding.
    let source = r#"
interface Renamed {
    [x: string]: string;
    prop?: string;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 regardless of the index-signature parameter name"
    );
}

// =========================================================================
// Unannotated parameter in a function/constructor *type node* must default
// to `any` (not left unset), matching tsc's `getTypeOfFunctionTypeNode`. A
// missing default previously left the parameter type-less so both the
// TS2411 relation and the printer treated it as if it agreed with anything.
// See #16131.
// =========================================================================

#[test]
fn ts2411_unannotated_param_function_type_as_index_signature() {
    // The index signature itself carries the unannotated function type.
    let source = r#"
interface I12 {
    [x: string]: (x) => number;
    foo: number;
}
export {};
"#;
    let diagnostics =
        diagnostic_code_messages(check_source(source, "test.ts", CheckerOptions::default()));
    assert!(
        diagnostics
            .iter()
            .any(|(code, msg)| *code == 2411 && msg.contains("(x: any) => number")),
        "Expected TS2411 with the parameter defaulted to `any`, got: {diagnostics:?}"
    );
}

#[test]
fn ts2411_explicit_any_param_function_type_as_index_signature_control() {
    // Control: explicit `any` must keep firing exactly as before.
    let source = r#"
interface J12 {
    [x: string]: (x: any) => number;
    foo: number;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Control with explicit `any` parameter must still report TS2411"
    );
}

#[test]
fn ts2411_unannotated_param_function_type_as_property() {
    // The unannotated function type is the *property*, not the index signature.
    let source = r#"
interface K {
    [x: string]: number;
    foo12: (x) => number;
}
export {};
"#;
    let diagnostics =
        diagnostic_code_messages(check_source(source, "test.ts", CheckerOptions::default()));
    assert!(
        diagnostics
            .iter()
            .any(|(code, msg)| *code == 2411 && msg.contains("(x: any) => number")),
        "Expected TS2411 with the property's parameter defaulted to `any`, got: {diagnostics:?}"
    );
}

#[test]
fn ts2411_unannotated_param_function_type_alias_as_property() {
    // The miss must survive a type alias wrapper, not just a literal function type.
    let source = r#"
type F = (x) => number;
interface K {
    [x: string]: number;
    foo12: F;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 through a type-alias-wrapped unannotated function type"
    );
}

#[test]
fn ts2411_unannotated_param_method_signature_as_property() {
    // Method-signature syntax (`foo12(a): number`) shares the same parameter
    // lowering as a property typed with a function-type literal.
    let source = r#"
interface MethodSig {
    [x: string]: number;
    foo12(a): number;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 for an unannotated method-signature parameter"
    );
}

#[test]
fn ts2411_unannotated_param_constructor_type_as_index_signature() {
    // Constructor types (`new (a) => T`) share `FUNCTION_TYPE`'s parameter
    // lowering; verify the `CONSTRUCTOR_TYPE` arm is fixed too.
    let source = r#"
interface CtorIdx {
    [x: string]: new (a) => object;
    foo: number;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 for an unannotated constructor-type parameter"
    );
}

#[test]
fn ts2411_unannotated_param_renamed_binders() {
    // Rename both the index-signature key and the function parameter to
    // prove the fix is structural, not name-driven.
    let source = r#"
interface RenamedGeneric {
    [zzz: string]: (qqq) => number;
    other: number;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 regardless of parameter/index-signature identifier names"
    );
}

#[test]
fn ts2411_unannotated_param_function_type_nested_in_array() {
    // The default must apply even when the function type is nested inside
    // another type constructor, not just at the top level of a member.
    // Uses the `T[]` array-type node (not `Array<T>`) because the unit-test
    // harness runs with no lib loaded, so a `TYPE_REFERENCE` to `Array`
    // would not resolve.
    let source = r#"
interface NestedArr {
    [x: string]: ((a) => number)[];
    foo: number;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 for an unannotated parameter nested inside an array type"
    );
}

#[test]
fn ts2411_unannotated_param_function_type_no_false_positive_when_compatible() {
    // Negative/fallback direction: two structurally-identical unannotated
    // function types (both defaulted to `any`) must NOT trigger a spurious
    // TS2411 — the fix must not overcorrect into new false positives.
    let source = r#"
interface Compatible {
    [x: string]: (a) => number;
    foo: (a) => number;
}
export {};
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Two structurally-identical unannotated function types must not report TS2411"
    );
}

// ---------------------------------------------------------------------------
// A computed member name that is not an entity name contributes no index
// signature to the class type.
//
// `tsc` only lets an entity-name key (`[s]`, `[o.p]`) contribute an index
// signature. An arbitrary expression key (`["" + ""]`, `[+s]`, `[f()]`)
// contributes nothing and is only ever *checked* against index signatures
// contributed by others. tsz used to synthesize a signature from any computed
// name whose key type was `string`/`number`/`any`, so such a member
// manufactured the very signature it was then measured against.
// ---------------------------------------------------------------------------

#[test]
fn non_entity_computed_method_name_contributes_no_index_signature() {
    // The conformance shape: computedPropertyNamesDeclarationEmit1_ES5/ES6.
    let source = r#"
class C {
    ["" + ""]() { }
    get ["" + ""]() { return 0; }
    set ["" + ""](x) { }
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "a class whose only members have non-entity computed names has no index \
         signature at all, so nothing can be checked against one: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn non_entity_computed_method_name_contributes_nothing_under_renamed_binders() {
    // Same structure, different key text and member names -- the rule is about
    // the shape of the key expression, never the spelling of any binder.
    let source = r#"
class Renamed {
    ["alpha" + "beta"]() { }
    get ["gamma" + "delta"]() { return 0; }
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "renaming the class and the key operands must not change the outcome: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn non_entity_computed_name_wrapper_forms_contribute_no_index_signature() {
    // Wrapper/nesting forms of a non-entity key: parenthesized identifier,
    // unary operator, and a call expression all fail the entity-name test.
    let source = r#"
declare var s: string;
declare function f(): string;
class C {
    [(s)]() { }
    [+s]() { }
    [f()]() { }
    get [(s)]() { return 0; }
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "parenthesized, unary and call-expression keys are not entity names: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn non_entity_computed_static_method_name_contributes_no_index_signature() {
    let source = r#"
class C {
    static ["" + ""]() { }
    static get ["" + ""]() { return 0; }
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "the static side follows the same rule as the instance side: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn declared_index_signature_still_checks_a_non_entity_computed_member() {
    // Positive control, and the direction the gate must NOT suppress: a member
    // with a non-entity computed name contributes no signature, but it is still
    // *checked* against a signature that was really declared.
    let source = r#"
class C {
    [k: string]: number;
    ["" + ""]() { }
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "a declared string index signature still constrains a non-entity \
         computed member: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn declared_index_signature_still_checks_a_literal_computed_member() {
    // Adjacent concrete form: a computed name with a literal type resolves to a
    // real named property and is checked exactly like one.
    let source = r#"
class C {
    [k: string]: number;
    ["lit"]() { }
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "a literal computed name is a named property and is still checked: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn entity_computed_name_still_contributes_an_index_signature() {
    // The gate must not over-apply: an entity-name key still contributes, and a
    // sibling non-entity computed member is still measured against it.
    let source = r#"
declare var s: string;
class C {
    [s]: number;
    [+s]: typeof s;
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "an entity-name key still contributes a string index signature that \
         constrains other computed members: {:?}",
        get_diagnostics(source)
    );
}

// =========================================================================
// #16477: an explicit `[key: symbol]` index does not dominate an implicit
// computed member keyed by a plain (non-`unique`) `symbol`-typed variable.
//
// Structural rule: when a computed property's key expression is a plain
// identifier of type `symbol` (not `unique symbol`), its runtime identity is
// unknown, so tsc treats the member as an implicit, late-bound contribution
// to the containing interface/type-literal's symbol index rather than a
// property independently checked against it. Members with a *fixed* symbol
// identity -- `unique symbol`, or a well-known `Symbol.x` access -- keep the
// TS2411 check. Classes never get this exemption, even for a plain `symbol`
// key: tsc still reports TS2411 there (measured against tsc 7.0.2).
// =========================================================================

#[test]
fn explicit_symbol_index_dominates_implicit_member_implicit_first() {
    let source = r#"
declare const s1: symbol;
declare const other: symbol;
interface M { [s1]: string; [key: symbol]: number }
declare const m: M;
export const r: number = m[other];
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "a plain symbol-keyed member is late-bound and is not checked \
         against the interface's own explicit symbol index: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn explicit_symbol_index_dominates_implicit_member_explicit_first() {
    // Order-independence: swapping which member comes first must not change
    // the outcome.
    let source = r#"
declare const s1: symbol;
interface M { [key: symbol]: number; [s1]: string; }
declare const m: M;
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "declaration order of the index signature vs. the late-bound member \
         must not matter: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn explicit_symbol_index_dominates_implicit_member_type_literal() {
    // Adjacent container form: a type literal follows the same rule as an
    // interface (both are non-class object types).
    let source = r#"
declare const s1: symbol;
type M = { [s1]: string; [key: symbol]: number };
declare const m: M;
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "a type literal exempts a late-bound symbol-keyed member the same \
         way an interface does: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn explicit_symbol_index_dominates_implicit_member_renamed_binder() {
    // Prove the rule is structural (any plain-`symbol`-typed variable), not
    // tied to a specific identifier spelling.
    let source = r#"
declare const myOwnKey: symbol;
interface M { [myOwnKey]: boolean; [key: symbol]: number }
declare const m: M;
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "the exemption must not be keyed to the identifier's name: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn class_symbol_keyed_member_still_checks_explicit_symbol_index() {
    // Negative case: unlike an interface/type-literal, a class's own
    // plain-symbol-keyed member is still checked against a declared symbol
    // index -- tsc reports TS2411 here (measured against tsc 7.0.2).
    let source = r#"
declare const s1: symbol;
class C { [s1]: string = ""; [key: symbol]: number; }
"#;
    assert!(
        has_error_with_code(source, 2411),
        "a class does not get the late-bound exemption an interface gets: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn class_static_symbol_keyed_member_still_checks_explicit_symbol_index() {
    // Adjacent form: the static side of a class follows the same rule as the
    // instance side.
    let source = r#"
declare const s1: symbol;
class C { static [s1]: string = ""; static [key: symbol]: number; }
"#;
    assert!(
        has_error_with_code(source, 2411),
        "a class's static side does not get the late-bound exemption either: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn unique_symbol_keyed_member_still_checks_explicit_symbol_index() {
    // Negative case: a `unique symbol` has a fixed compile-time identity, so
    // it keeps the strict TS2411 check even in an interface (measured against
    // tsc 7.0.2).
    let source = r#"
declare const s1: unique symbol;
interface M { [s1]: string; [key: symbol]: number }
declare const m: M;
"#;
    assert!(
        has_error_with_code(source, 2411),
        "a unique symbol key has a fixed identity and is not late-bound: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn well_known_symbol_keyed_member_still_checks_explicit_symbol_index() {
    // Negative case: a well-known symbol access (`Symbol.iterator`) also has a
    // fixed identity and keeps the strict check (measured against tsc 7.0.2).
    let source = r#"
interface M { [Symbol.iterator]: string; [key: symbol]: number }
declare const m: M;
"#;
    assert!(
        has_error_with_code(source, 2411),
        "a well-known symbol key has a fixed identity and is not late-bound: {:?}",
        get_diagnostics(source)
    );
}

// =========================================================================
// Regression: a numeric/string enum unioned with its own primitive absorbs
// into that primitive before the index-signature check runs, matching tsc's
// `removeRedundantLiteralTypes` (which sweeps an enum member's LiteralType
// the same as any other literal). Belt-and-suspenders: the TS2411 check site
// also widens through `evaluate_type_with_env` before decomposing the union
// (see `property_type_assignable_to_index_type`), so the fix holds even if a
// future caller builds the union some other way. #16866 /
// unionSubtypeIfEveryConstituentTypeIsSubtype.ts.
// =========================================================================

#[test]
fn ts2411_numeric_enum_unioned_with_number_absorbs_against_unrelated_enum_index() {
    // `e | number` collapses to `number` before this check runs, so it is
    // never compared against `E2` as a nominal enum member — matches tsc
    // 7.0.2 exactly (byte-for-byte on the real conformance fixture).
    let source = r#"
enum e { e1, e2 }
enum E2 { A }
interface I14 {
    [x: string]: E2;
    foo2: e | number;
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "e | number must absorb into number and not be checked against the unrelated enum E2: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn enum_member_absorbed_into_number_in_union_matches_numeric_enum_index() {
    // A union constituent that is a subtype of a sibling constituent is
    // absorbed into it when tsc constructs the union type -- `Suit | number`
    // is the type `number`, not a two-member union (confirmed against tsc
    // 7.0.2: `let s: string = suitOrNumber` reports "Type 'number' is not
    // assignable to type 'string'", not "Type 'Suit | number'"). A property
    // of that union is therefore checked against a numeric-enum index the
    // same way a bare `number` property is -- no TS2411.
    let source = r#"
enum Suit { Hearts, Spades }
enum Rank { Ace }
interface Hand {
    [x: string]: Rank;
    card: Suit | number;
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "`Suit | number` collapses to `number`, which a numeric enum index accepts: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn ts2411_string_enum_unioned_with_string_still_errors_against_unrelated_enum_index() {
    // Negative control, oracle-verified against tsc 7.0.2: `s | string` DOES
    // absorb into plain `string` (same union-construction rule as the
    // numeric case), but unlike numeric enums, tsc has no "open string enum"
    // exception — a bare `string` is never assignable to ANY string enum, so
    // the diagnostic still fires. The message text itself proves absorption
    // happened: it names the collapsed type `'string'`, not the enum `'s'`.
    let source = r#"
enum s { a = "a", b = "b" }
enum S2 { X = "x" }
interface I {
    [x: string]: S2;
    foo2: s | string;
}
"#;
    let diagnostics = get_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2411 && message.contains("type 'string'") && message.contains("'S2'")
        }),
        "s | string must absorb into string (proven by the message naming 'string', not 's') \
         and still fail against the unrelated string enum S2: {diagnostics:?}"
    );
}

#[test]
fn ts2411_specific_numeric_enum_member_unioned_with_number_absorbs() {
    // Adjacent case: a specific member (`e.e1`), not the whole enum type,
    // must absorb the same way.
    let source = r#"
enum e { e1, e2 }
enum E2 { A }
interface I {
    [x: string]: E2;
    foo2: e.e1 | number;
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "e.e1 | number must absorb into number: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn ts2411_renamed_numeric_enum_unioned_with_number_absorbs() {
    // Adjacent case: renamed binders prove the rule is structural, not tied
    // to a specific identifier.
    let source = r#"
enum Direction { Up, Down }
enum Suit { Clubs }
interface Board {
    [x: string]: Suit;
    cell: Direction | number;
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Direction | number must absorb into number regardless of binder names: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn ts2411_numeric_enum_alias_wrapper_unioned_with_number_absorbs() {
    // Adjacent case: alias/wrapper — the union is written through a type
    // alias rather than inline in the property; the alias resolves to the
    // same underlying union before this check runs.
    let source = r#"
enum e { e1, e2 }
enum E2 { A }
type EOrNumber = e | number;
interface I {
    [x: string]: E2;
    foo2: EOrNumber;
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "an aliased e | number must still absorb into number: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn ts2411_string_literal_still_errors_against_unrelated_numeric_enum_index() {
    // Negative control: `foo: string | number` in the same interface must
    // still error — `string` is not absorbed, and `number` alone is not
    // structurally assignable to the unrelated enum `E2` (matches tsc).
    let source = r#"
enum E2 { A }
interface I14 {
    [x: string]: E2;
    foo: string | number;
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "string | number must still be rejected against the unrelated enum E2: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn distinct_enum_union_without_number_sibling_still_reports_ts2411() {
    // Negative control: without a `number` (or matching-enum) sibling to
    // absorb into, a different enum's member type is still not assignable to
    // this index's enum -- the widen step must not silently accept every
    // enum union.
    let source = r#"
enum Suit { Hearts, Spades }
enum Rank { Ace }
enum Season { Summer }
interface Hand {
    [x: string]: Rank;
    card: Suit | Season;
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "`Suit | Season` has no `number` sibling to collapse into, and neither is `Rank`: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn ts2411_distinct_numeric_enums_without_a_bare_number_stay_nominal() {
    // Negative control: without a co-present bare `number`, two distinct
    // numeric enums do NOT absorb into each other — nominal typing holds.
    let source = r#"
enum e { e1, e2 }
enum E2 { A }
interface I {
    [x: string]: E2;
    foo2: e;
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "e alone (no bare number present) must stay nominal and fail against E2: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn numeric_literal_absorbed_into_number_in_union_matches_numeric_enum_index_renamed() {
    // Same shape as the enum case above but with a numeric literal type and
    // renamed binders, confirming the widen step is not keyed to a specific
    // enum/interface/property name.
    let source = r#"
enum Level { Low, Mid, High }
interface Config {
    [k: string]: Level;
    threshold: 1 | number;
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "`1 | number` collapses to `number`, matching the numeric enum index: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn union_still_reports_ts2411_when_a_constituent_genuinely_mismatches() {
    // Positive control: the widen step must not blanket-suppress TS2411 -- a
    // union that still contains a genuinely incompatible constituent after
    // widening (`string`, which has no relation to a `number` index) keeps
    // reporting.
    let source = r#"
interface Bag {
    [k: string]: number;
    label: string | number;
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "`string | number` still has `string`, incompatible with the `number` index: {:?}",
        get_diagnostics(source)
    );
}
