//! Regression tests for issue #4011: a class `implements` clause must emit
//! TS2416 when an interface method requires a type predicate (`x is T`) but
//! the class method's body does not narrow — i.e. tsc cannot infer the
//! predicate either.
//!
//! Previously, the member-compatibility path suppressed TS2416 for any
//! unannotated boolean-returning method, regardless of whether the body
//! was actually inferrable as a predicate. That bypassed the proof-of-
//! inferability that `signature_builder` already runs, hiding real mismatches.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_with_libs, load_lib_files};

/// A class method whose body genuinely narrows (e.g. `typeof x === "string"`)
/// gets a predicate inferred by tsz (mirroring TS 5.5+ inferred predicates),
/// so it satisfies an interface requiring `value is string` — no TS2416.
#[test]
fn implements_predicate_inferred_from_narrowing_body_no_ts2416() {
    let source = r#"
interface IsString {
  isString(value: string | number): value is string;
}

class Acceptable implements IsString {
  isString(value: string | number) {
    return typeof value === "string";
  }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2416: Vec<_> = diags.iter().filter(|d| d.code == 2416).collect();
    let ts2420: Vec<_> = diags.iter().filter(|d| d.code == 2420).collect();
    assert!(
        ts2416.is_empty() && ts2420.is_empty(),
        "Expected no TS2416/TS2420 when body infers the required predicate, got: {diags:#?}"
    );
}

/// A class method that returns plain `boolean` from a non-narrowing body
/// (e.g. `return true;`) cannot have a predicate inferred. tsc reports
/// TS2416 ("Signature must be a type predicate") and tsz must too.
#[test]
fn implements_predicate_with_non_inferable_body_emits_ts2416() {
    let source = r#"
interface IsString {
  isString(value: string | number): value is string;
}

class Broken implements IsString {
  isString(_value: string | number) {
    return true;
  }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    // tsc emits TS2416 (member-level mismatch). Some pipelines also surface
    // TS2420 (class-level "incorrectly implements interface"); accept either
    // surface so long as the predicate mismatch is reported.
    let predicate_mismatch_reported = diags.iter().any(|d| d.code == 2416 || d.code == 2420);
    assert!(
        predicate_mismatch_reported,
        "Expected TS2416/TS2420 for non-inferable predicate body, got: {diags:#?}"
    );
}

/// Same shape with a different parameter name proves the fix is structural,
/// not keyed off a particular identifier.
#[test]
fn implements_predicate_non_inferable_alternate_param_name_emits_ts2416() {
    let source = r#"
interface IsNumber {
  check(input: string | number): input is number;
}

class BrokenAlt implements IsNumber {
  check(_input: string | number) {
    return false;
  }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let predicate_mismatch_reported = diags.iter().any(|d| d.code == 2416 || d.code == 2420);
    assert!(
        predicate_mismatch_reported,
        "Expected TS2416/TS2420 for non-inferable predicate body (alt name), got: {diags:#?}"
    );
}

/// An explicit `: boolean` annotation is also non-inferable in tsc — tsc
/// keeps the annotation literal and reports TS2416. Confirm tsz matches.
#[test]
fn implements_predicate_with_explicit_boolean_annotation_emits_ts2416() {
    let source = r#"
interface IsString {
  isString(value: string | number): value is string;
}

class Annotated implements IsString {
  isString(value: string | number): boolean {
    return typeof value === "string";
  }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let predicate_mismatch_reported = diags.iter().any(|d| d.code == 2416 || d.code == 2420);
    assert!(
        predicate_mismatch_reported,
        "Expected TS2416/TS2420 when class method explicitly annotates `: boolean`, got: {diags:#?}"
    );
}

#[test]
fn implements_public_computed_name_class_shape_does_not_emit_ts2720() {
    let source = r#"
const c0 = "a";
const c1 = 1;
const s0 = Symbol();

declare class T1 {
    [c0]: number;
    [c1]: string;
    [s0]: boolean;
}
declare class T2 extends T1 {
}

const s2: typeof s0 = s0;

declare class T13 implements T2 {
    a: number;
    1: string;
    [s2]: boolean;
}
"#;
    let libs = load_lib_files(&["es2015.d.ts"]);
    let diags = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    assert!(
        diags.iter().all(|diag| diag.code != 2720),
        "Expected no TS2720 for public computed-name class shape, got: {diags:#?}",
    );
}
