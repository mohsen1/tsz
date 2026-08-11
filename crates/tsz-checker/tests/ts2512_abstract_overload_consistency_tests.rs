//! Tests for TS2512 ("Overload signatures must all be abstract or
//! non-abstract.") across the two overload-set shapes that carry it.
//!
//! tsc routes both through `checkFunctionOrConstructorSymbol`: it takes the
//! canonical abstractness from `implementation ?? overloads[0]` and flags every
//! signature that deviates, reporting on `getNameOfDeclaration(o)`.
//!
//! - A **method** overload set has a name, so the diagnostic is anchored at the
//!   deviating method's name (a located `test.ts(line,col)` diagnostic).
//! - A **constructor** overload set has no name, so `getNameOfDeclaration`
//!   resolves to `undefined` and tsc emits the diagnostic with *no source
//!   location* at all — it prints as a bare `error TS2512: ...`. Several
//!   deviating constructor signatures collapse to a single entry through the
//!   diagnostic set's deduplication.
//!
//! tsz previously implemented only the method arm, so a mixed abstract /
//! non-abstract constructor overload set silently dropped TS2512 (issue
//! #17166). The constructor `abstract`-modifier form is itself a grammar error
//! (TS1242), so this shape only arises in already-erroneous code, but tsc still
//! runs the abstract-consistency check and so must tsz.
//!
//! Oracle-verified against pinned `typescript@7.0.2` with
//! `--noEmit --pretty false --singleThreaded --strict`. Class and method
//! binders are varied so no fix can key on a specific name.

use tsz_checker::test_utils::{check_source_code_messages, check_source_diagnostics};

const TS2512: u32 = 2512;

fn count(source: &str, code: u32) -> usize {
    check_source_code_messages(source)
        .iter()
        .filter(|d| d.0 == code)
        .count()
}

fn has(source: &str, code: u32) -> bool {
    count(source, code) > 0
}

// ---------------------------------------------------------------------------
// Constructor overload sets — the fixed case.
// ---------------------------------------------------------------------------

/// The reported repro: a non-abstract constructor signature followed by an
/// `abstract` one. The canonical abstractness is the first signature's
/// (non-abstract), so the `abstract` signature deviates -> one TS2512.
#[test]
fn mixed_constructor_overloads_report_ts2512() {
    let source = "class A { constructor(x: number); abstract constructor(x: string); }\n";
    assert_eq!(count(source, TS2512), 1);
}

/// The constructor TS2512 is emitted with no source location — an empty file
/// and a zero span — matching tsc's location-less `error(undefined, ...)`.
#[test]
fn constructor_ts2512_is_location_less() {
    let source = "class A { constructor(x: number); abstract constructor(x: string); }\n";
    let ts2512: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == TS2512)
        .collect();
    assert_eq!(ts2512.len(), 1, "exactly one TS2512");
    let diag = &ts2512[0];
    assert!(
        diag.file.is_empty(),
        "constructor TS2512 has no file, got {:?}",
        diag.file
    );
    assert_eq!(diag.start, 0, "constructor TS2512 has a zero span start");
    assert_eq!(diag.length, 0, "constructor TS2512 has a zero span length");
}

/// The abstractness order does not matter: an `abstract` signature first, then
/// non-abstract ones, still reports TS2512. The canonical is the first
/// (abstract) signature, so the two non-abstract signatures each deviate — but
/// the identical location-less diagnostics collapse to a single entry, exactly
/// as tsc reports it once.
#[test]
fn two_deviating_constructors_collapse_to_one_ts2512() {
    let source = "class Ledger {\n  abstract constructor(x: number);\n  constructor(x: string);\n  constructor(x: boolean);\n}\n";
    assert_eq!(
        count(source, TS2512),
        1,
        "multiple deviating constructors dedupe to a single TS2512"
    );
}

/// When an implementation body is present, its abstractness (non-abstract — an
/// abstract member cannot have a body) is canonical, so a preceding `abstract`
/// signature deviates -> TS2512.
#[test]
fn constructor_with_implementation_flags_abstract_signature() {
    let source =
        "class Meter { abstract constructor(x: number); constructor(x: string | number) {} }\n";
    assert_eq!(count(source, TS2512), 1);
}

/// Two non-abstract constructor overloads agree -> no TS2512.
#[test]
fn all_non_abstract_constructor_overloads_no_ts2512() {
    let source = "class Dial { constructor(x: number); constructor(x: string); }\n";
    assert!(!has(source, TS2512));
}

/// A single `abstract` constructor is not an overload *set* (one declaration),
/// so there is nothing to disagree with -> no TS2512 (only the TS1242 grammar
/// error, checked elsewhere).
#[test]
fn abstract_constructor_alone_no_ts2512() {
    let source = "class Gauge { abstract constructor(); }\n";
    assert!(!has(source, TS2512));
}

/// Two `abstract` constructor signatures agree with each other -> no TS2512.
#[test]
fn two_abstract_constructor_overloads_no_ts2512() {
    let source =
        "class Valve { abstract constructor(x: number); abstract constructor(x: string); }\n";
    assert!(!has(source, TS2512));
}

/// A single constructor with a body is not an overload set -> no TS2512.
#[test]
fn single_constructor_no_ts2512() {
    let source = "class Piston { constructor(x: number) {} }\n";
    assert!(!has(source, TS2512));
}

/// The fix does not key on a class name: a renamed binder around the same mixed
/// shape still reports exactly one location-less TS2512.
#[test]
fn mixed_constructor_ts2512_binder_name_varies() {
    let source =
        "class ZqxWidgetFactory { constructor(a: number); abstract constructor(a: string); }\n";
    let ts2512: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == TS2512)
        .collect();
    assert_eq!(ts2512.len(), 1);
    assert!(ts2512[0].file.is_empty());
}

/// Two classes in one compilation, each with a mixed constructor overload set,
/// still yield exactly one TS2512: the location-less diagnostics share the
/// `(start, code)` dedup key and collapse to a single entry, matching tsc (which
/// emits its program-level TS2512 at most once across the whole compilation,
/// even across classes and files).
#[test]
fn two_classes_share_a_single_ts2512() {
    let source = "class A { constructor(x: number); abstract constructor(x: string); }\nclass B { constructor(y: number); abstract constructor(y: string); }\n";
    assert_eq!(count(source, TS2512), 1);
}

// ---------------------------------------------------------------------------
// Method overload sets — the legal analogue (already handled; pinned here so
// the two export forms of the same rule stay paired).
// ---------------------------------------------------------------------------

/// A mixed abstract / non-abstract method overload set in an abstract class —
/// the legal shape where TS2512 actually matters — reports TS2512. (The paired
/// TS2391 abstract-final suppression on this same shape is covered by
/// `abstract_method_overload_ts2391_suppression_tests`.)
#[test]
fn mixed_method_overloads_in_abstract_class_report_ts2512() {
    let source =
        "abstract class Shape {\n  m(x: number): void;\n  abstract m(x: string): void;\n}\n";
    assert_eq!(count(source, TS2512), 1);
}

/// Unlike the constructor case, a method TS2512 is anchored at the deviating
/// method's name — a located diagnostic in the file, with a non-zero span.
#[test]
fn method_ts2512_is_anchored_at_the_name() {
    let source =
        "abstract class Shape {\n  m(x: number): void;\n  abstract m(x: string): void;\n}\n";
    let ts2512: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == TS2512)
        .collect();
    assert_eq!(ts2512.len(), 1);
    let diag = &ts2512[0];
    assert!(
        !diag.file.is_empty(),
        "method TS2512 is anchored in the file"
    );
    assert!(diag.length > 0, "method TS2512 has a non-zero span");
}

/// A method implementation body makes its abstractness canonical
/// (non-abstract), so a preceding abstract signature deviates -> TS2512.
#[test]
fn method_with_implementation_flags_abstract_signature() {
    let source = "abstract class Shape {\n  abstract m(x: string): void;\n  m(x: number | string): void {}\n}\n";
    assert_eq!(count(source, TS2512), 1);
}

/// Non-abstract method overloads that agree report no TS2512.
#[test]
fn all_non_abstract_method_overloads_no_ts2512() {
    let source = "abstract class Shape {\n  m(x: number): void;\n  m(x: string): void;\n  m(x: number | string): void {}\n}\n";
    assert!(!has(source, TS2512));
}

/// The method arm does not key on a name either: a renamed method around the
/// same mixed shape still reports exactly one name-anchored TS2512.
#[test]
fn mixed_method_ts2512_binder_name_varies() {
    let source = "abstract class Shape {\n  zqxTransform(x: number): void;\n  abstract zqxTransform(x: string): void;\n}\n";
    let ts2512: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == TS2512)
        .collect();
    assert_eq!(ts2512.len(), 1);
    assert!(!ts2512[0].file.is_empty());
}
