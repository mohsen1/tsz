/// Tests for non-unique `symbol` typed computed property names in interfaces and type literals.
///
/// Rule: when an interface or type literal has a member whose computed property name cannot be
/// resolved to a specific string/unique-symbol key (because the variable has the general `symbol`
/// type), the resulting object type must carry `HAS_LATE_BOUND_MEMBERS`.  Indexed access via a
/// `symbol`-typed key on such a type must return `any` (matching tsc), not `undefined`.
///
/// Without this flag, tsz incorrectly falls through to `undefined` and emits a false-positive
/// TS2322 when the result is assigned to the declared element type.
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn has_2322(source: &str) -> bool {
    codes(source).contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
}

// ── Interface with property signature ────────────────────────────────────────

#[test]
fn interface_non_unique_symbol_property_no_false_positive_ts2322() {
    // The original repro: `sym: symbol`, interface property `[sym]: number`.
    // Assigning `ws[sym]` to `number` must not produce TS2322.
    assert!(!has_2322(
        r#"
const sym: symbol = Symbol("test");
interface WithSym { [sym]: number; }
declare const ws: WithSym;
const _x: number = ws[sym];
"#
    ));
}

#[test]
fn interface_non_unique_symbol_property_different_name_no_false_positive() {
    // Prove the fix is structural (any `symbol`-typed var), not tied to the name "sym".
    assert!(!has_2322(
        r#"
const k: symbol = Symbol();
interface I { [k]: string; }
declare const obj: I;
const _x: string = obj[k];
"#
    ));
}

#[test]
fn interface_non_unique_symbol_property_third_name_no_false_positive() {
    // A third distinct spelling to further confirm the rule is general.
    assert!(!has_2322(
        r#"
const myKey: symbol = Symbol("myKey");
interface MyInterface { [myKey]: boolean; }
declare const o: MyInterface;
const _x: boolean = o[myKey];
"#
    ));
}

// ── Inline type literal ───────────────────────────────────────────────────────

#[test]
fn type_literal_non_unique_symbol_property_no_false_positive() {
    assert!(!has_2322(
        r#"
const sym: symbol = Symbol();
type T = { [sym]: number };
declare const obj: T;
const _x: number = obj[sym];
"#
    ));
}

#[test]
fn inline_object_type_annotation_non_unique_symbol_no_false_positive() {
    assert!(!has_2322(
        r#"
const k: symbol = Symbol();
declare const obj: { [k]: string };
const _x: string = obj[k];
"#
    ));
}

// ── Method signature ──────────────────────────────────────────────────────────

#[test]
fn interface_non_unique_symbol_method_signature_no_false_positive() {
    // Method signature with computed name: `[sym](): void`
    assert!(!has_2322(
        r#"
const sym: symbol = Symbol();
interface WithMethod { [sym](): void; }
declare const obj: WithMethod;
const _fn: (() => void) | undefined = obj[sym];
"#
    ));
}

// ── Readonly property ─────────────────────────────────────────────────────────

#[test]
fn interface_readonly_non_unique_symbol_property_no_false_positive() {
    assert!(!has_2322(
        r#"
const k: symbol = Symbol();
interface ReadonlyI { readonly [k]: number; }
declare const obj: ReadonlyI;
const _x: number = obj[k];
"#
    ));
}

// ── Multiple non-unique symbol members ────────────────────────────────────────

#[test]
fn interface_multiple_non_unique_symbol_members_no_false_positive() {
    assert!(!has_2322(
        r#"
const a: symbol = Symbol("a");
const b: symbol = Symbol("b");
interface Multi { [a]: number; [b]: string; }
declare const obj: Multi;
const _x: number = obj[a];
const _y: string = obj[b];
"#
    ));
}

// ── Unique symbol still works ─────────────────────────────────────────────────

#[test]
fn unique_symbol_property_still_resolves_correctly() {
    // Unique-symbol typed computed properties should resolve to their declared type.
    // This must not regress.
    assert!(!has_2322(
        r#"
declare const uSym: unique symbol;
interface WithUnique { [uSym]: number; }
declare const obj: WithUnique;
const _x: number = obj[uSym];
"#
    ));
}

// ── Actual type errors still caught ──────────────────────────────────────────

#[test]
fn plain_string_property_mismatch_still_reported() {
    // A normal (string-keyed) property mismatch must still produce TS2322.
    assert!(has_2322(
        r#"
interface Plain { x: number; }
declare const obj: Plain;
const _x: string = obj.x;
"#
    ));
}
