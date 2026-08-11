//! Regression tests for missing-property promotion on a **call argument** whose
//! parameter type carries BOTH an index signature and a method with its own
//! generic type parameter(s).
//!
//! Structural rule (oracled against `typescript@7.0.2`, the conformance pin):
//! passing an incompatible value to a parameter of an object/interface type that
//! declares a required member the value lacks reports the specific
//! missing-property diagnostic (`TS2741` for one, `TS2739`/`TS2740` for more) —
//! the same family the direct-assignment path emits — regardless of whether the
//! parameter type also happens to contain a method with its own generic
//! signature (`m<S>(x: S): S`). A signature-bound type parameter is not a *free*
//! type parameter of the call, so the target stays fully structural.
//!
//! The regression (#17145): the call-argument mismatch renderer routed on
//! `contains_type_parameters(expected)`, which counts a method's
//! signature-bound `S`, and so misrouted these concrete targets to the
//! elaboration-free `error_argument_not_assignable_preserving_param_display`
//! path — dropping the missing-property line and leaving a bare `TS2345`. The
//! fix routes on `contains_free_type_parameters` instead
//! (`types/computation/call_result.rs`), so only a genuine unresolved outer
//! type parameter still preserves its bare display.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

const TS2345: u32 = diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
const TS2739: u32 = diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE;
const TS2741: u32 = diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE;

/// The single missing-property / argument diagnostic (`TS2741`/`TS2739`/`TS2345`)
/// as a `(code, message)` pair. Asserts exactly one such diagnostic exists.
fn missing_property_diagnostic(source: &str) -> (u32, String) {
    let diagnostics = check_source_diagnostics(source);
    let matching: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.code == TS2741 || d.code == TS2739 || d.code == TS2345)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one missing-property/argument diagnostic; got {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    (matching[0].code, matching[0].message_text.clone())
}

// --- the bug: a call argument, index signature + generic method -------------

#[test]
fn call_argument_number_index_plus_generic_method_promotes_to_ts2741() {
    // Was a bare `TS2345` with no elaboration before #17145.
    assert_eq!(
        missing_property_diagnostic(
            "interface Big {\n    m<S>(x: S): S;\n    readonly [n: number]: string;\n}\nfunction f(x: Big) {}\nf({});\n",
        ),
        (
            TS2741,
            "Property 'm' is missing in type '{}' but required in type 'Big'.".to_string()
        ),
    );
}

/// A string index signature triggers the same combination (binder names differ
/// from the number-index case on purpose).
#[test]
fn call_argument_string_index_plus_generic_method_promotes_to_ts2741() {
    // The generic method is not assignable to the string index type, so tsc also
    // reports TS2411 at the interface declaration — but the *call argument* still
    // gets its missing-property line, which is what this test pins.
    let (code, message) = missing_property_diagnostic(
        "interface Bag {\n    grab<Item>(value: Item): Item;\n    readonly [key: string]: string;\n}\nfunction take(bag: Bag) {}\ntake({});\n",
    );
    assert_eq!(code, TS2741);
    assert_eq!(
        message,
        "Property 'grab' is missing in type '{}' but required in type 'Bag'."
    );
}

/// Two missing required members promote to the plural `TS2739`, exactly as the
/// assignment path does.
#[test]
fn call_argument_two_missing_members_promotes_to_ts2739() {
    let (code, message) = missing_property_diagnostic(
        "interface Wide {\n    pick<K>(k: K): K;\n    tag: number;\n    readonly [n: number]: string;\n}\nfunction use(w: Wide) {}\nuse({});\n",
    );
    assert_eq!(code, TS2739);
    assert_eq!(
        message,
        "Type '{}' is missing the following properties from type 'Wide': pick, tag"
    );
}

/// A non-empty source that still lacks the required generic method promotes too.
#[test]
fn call_argument_non_empty_source_still_promotes() {
    let (code, _message) = missing_property_diagnostic(
        "interface Store {\n    load<T>(x: T): T;\n    readonly [n: number]: string;\n}\nfunction run(s: Store) {}\nrun({ 0: \"present\" });\n",
    );
    assert_eq!(code, TS2741);
}

// --- parity anchor: the call diagnostic equals the assignment diagnostic -----

/// The call-argument and direct-assignment paths must agree on the exact
/// missing-property diagnostic for the same source/target pair. Encodes the
/// structural rule directly and is robust to display refinements.
#[test]
fn call_argument_matches_assignment_diagnostic() {
    let shape = "interface Rec {\n    map<U>(x: U): U;\n    readonly [n: number]: string;\n}\n";
    let call = format!("{shape}function f(r: Rec) {{}}\nf({{}});\n");
    let assign = format!("{shape}const r: Rec = {{}};\n");
    assert_eq!(
        missing_property_diagnostic(&call),
        missing_property_diagnostic(&assign),
        "call-argument and assignment must produce the same missing-property diagnostic"
    );
}

// --- controls: each condition alone already worked (must stay working) ------

/// Index signature with a NON-generic method: promotion already worked; must
/// keep working (guards against over-narrowing the fix).
#[test]
fn call_argument_index_plus_non_generic_method_still_promotes() {
    let (code, _message) = missing_property_diagnostic(
        "interface Plain {\n    hit(x: number): number;\n    readonly [n: number]: string;\n}\nfunction go(p: Plain) {}\ngo({});\n",
    );
    assert_eq!(code, TS2741);
}

/// A generic method WITHOUT an index signature: promotion already worked.
#[test]
fn call_argument_generic_method_without_index_still_promotes() {
    let (code, _message) = missing_property_diagnostic(
        "interface Only {\n    each<E>(x: E): E;\n}\nfunction call(o: Only) {}\ncall({});\n",
    );
    assert_eq!(code, TS2741);
}

/// Positive control: a value that DOES satisfy the shape produces no diagnostic
/// at the call site, so the reroute did not start manufacturing errors.
#[test]
fn call_argument_compatible_value_reports_nothing() {
    let diagnostics = check_source_diagnostics(
        "interface Good {\n    m<S>(x: S): S;\n    readonly [n: number]: string;\n}\nfunction f(x: Good) {}\nf({ m<S>(x: S): S { return x; } });\n",
    );
    let argument_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == TS2345 || d.code == TS2741 || d.code == TS2739)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    assert!(
        argument_errors.is_empty(),
        "compatible argument must not error; got {argument_errors:?}"
    );
}
