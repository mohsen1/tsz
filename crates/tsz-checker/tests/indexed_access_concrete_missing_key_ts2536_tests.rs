//! A concrete object type indexed by a missing literal key reports the property
//! as missing (TS2339), never "Type 'X' cannot be used to index type 'Y'"
//! (TS2536).
//!
//! Structural rule: tsc reserves TS2536 for indexed accesses whose index type is
//! a type parameter (or whose object type is generic/deferred). When the object
//! type is fully concrete — an anonymous object literal, an interface/class, a
//! union or intersection of such, a function/callable type, or `unknown` — and
//! the index is a concrete literal that is absent, tsc emits TS2339 for the
//! missing property and does *not* additionally emit TS2536. Previously tsz
//! emitted a spurious TS2536 alongside the TS2339 for anonymous object-literal
//! aliases (and emitted only the spurious TS2536 for unions/function types),
//! diverging from interfaces/classes which already reached the property path.
//!
//! These tests pin the rule structurally (varying type names, iteration-variable
//! names, and shapes) so the fix cannot be a spelling-specific point patch, and
//! include negative controls proving the legitimate TS2536 (type-parameter index)
//! and valid accesses are unaffected.

use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_diagnostics(source)
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

fn missing_property_messages(diags: &[Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == 2339)
        .map(|d| d.message_text.clone())
        .collect()
}

/// Reported repro: a type alias to an anonymous object literal, indexed by a
/// missing string literal, must report TS2339 only — no spurious TS2536.
#[test]
fn anonymous_object_alias_missing_string_literal_is_ts2339_only() {
    let diags = check(
        r#"
type TA = { a: number };
type R = TA["b"];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "anonymous object alias must not emit TS2536, got: {:?}",
        codes(&diags)
    );
    assert!(
        missing_property_messages(&diags)
            .iter()
            .any(|m| m.contains("'b'") && m.contains("type 'TA'")),
        "expected TS2339 for missing 'b', got: {:?}",
        missing_property_messages(&diags)
    );
}

/// Same rule with a different alias name and property spelling — proves the fix
/// is structural, not keyed to `TA`/`b`.
#[test]
fn anonymous_object_alias_missing_key_is_name_agnostic() {
    let diags = check(
        r#"
type Shape = { width: number; height: number };
type R = Shape["depth"];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "must not emit TS2536, got: {:?}",
        codes(&diags)
    );
    assert!(
        missing_property_messages(&diags)
            .iter()
            .any(|m| m.contains("'depth'") && m.contains("type 'Shape'")),
        "expected TS2339 for missing 'depth', got: {:?}",
        missing_property_messages(&diags)
    );
}

/// An inline anonymous object literal (no alias) indexed by a missing literal.
#[test]
fn inline_anonymous_object_missing_literal_is_ts2339_only() {
    let diags = check(
        r#"
type R = { a: number }["b"];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "inline object literal must not emit TS2536, got: {:?}",
        codes(&diags)
    );
    assert!(
        has_code(&diags, 2339),
        "expected TS2339, got: {:?}",
        codes(&diags)
    );
}

/// A union of concrete object types indexed by a key missing from one member.
#[test]
fn union_of_objects_missing_member_is_ts2339_only() {
    let diags = check(
        r#"
type U = { a: number } | { a: number; b: string };
type R = U["b"];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "union of objects must not emit TS2536, got: {:?}",
        codes(&diags)
    );
    assert!(
        has_code(&diags, 2339),
        "expected TS2339, got: {:?}",
        codes(&diags)
    );
}

/// An intersection of concrete object types indexed by an absent key.
#[test]
fn intersection_of_objects_missing_key_is_ts2339_only() {
    let diags = check(
        r#"
type In = { a: number } & { c: string };
type R = In["b"];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "intersection of objects must not emit TS2536, got: {:?}",
        codes(&diags)
    );
    assert!(
        has_code(&diags, 2339),
        "expected TS2339, got: {:?}",
        codes(&diags)
    );
}

/// A function type indexed by a missing member: tsc reports TS2339, not TS2536.
#[test]
fn function_type_missing_member_is_ts2339_only() {
    let diags = check(
        r#"
type Fn = (x: number) => string;
type R = Fn["nope"];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "function type must not emit TS2536, got: {:?}",
        codes(&diags)
    );
    assert!(
        has_code(&diags, 2339),
        "expected TS2339, got: {:?}",
        codes(&diags)
    );
}

/// The result of a (key-remapping-free) mapped type is a concrete object; an
/// absent key reported through it must be TS2339, not TS2536, regardless of the
/// mapped-type iteration variable name.
#[test]
fn mapped_type_result_missing_literal_is_ts2339_only() {
    for iter_var in ["P", "Key"] {
        let src = format!(
            r#"
type Mapped = {{ [{iter_var} in "a" | "b"]: number }};
type R = Mapped["c"];
"#
        );
        let diags = check(&src);
        assert!(
            !has_code(&diags, 2536),
            "mapped-type result (iter var {iter_var}) must not emit TS2536, got: {:?}",
            codes(&diags)
        );
        assert!(
            has_code(&diags, 2339),
            "expected TS2339 (iter var {iter_var}), got: {:?}",
            codes(&diags)
        );
    }
}

/// Union-of-literals index over an anonymous object: the valid member is
/// suppressed, the missing member reports TS2339, and no TS2536 appears.
#[test]
fn union_literal_index_over_anonymous_object_is_ts2339_only() {
    let diags = check(
        r#"
type TA = { a: number };
type R = TA["a" | "b"];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "union-literal index over anonymous object must not emit TS2536, got: {:?}",
        codes(&diags)
    );
    let msgs = missing_property_messages(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("'b'")),
        "expected TS2339 for missing 'b', got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("'a'")),
        "valid member 'a' must not report, got: {msgs:?}"
    );
}

/// Negative control / generalization gate: the *legitimate* TS2536 — a concrete
/// object indexed by a type parameter that is not constrained to that object's
/// keyof — must be preserved. Exercised with two distinct type-parameter names
/// to prove the preserved path is not name-based.
#[test]
fn legitimate_ts2536_type_parameter_index_is_preserved() {
    for param in ["K", "Idx"] {
        let src = format!(
            r#"
type Target = {{ a: number; b: string }};
type Other = {{ z: number }};
type R<{param} extends keyof Other> = Target[{param}];
"#
        );
        let diags = check(&src);
        assert!(
            has_code(&diags, 2536),
            "legitimate TS2536 (type param {param}) must be preserved, got: {:?}",
            codes(&diags)
        );
    }
}

/// Negative control: a valid access into a concrete object type must stay clean.
#[test]
fn valid_concrete_object_access_is_clean() {
    let diags = check(
        r#"
type O = { a: number; b: string };
type R1 = O["a"];
type R2 = O["a" | "b"];
type R3 = O[keyof O];
"#,
    );
    assert!(
        !has_code(&diags, 2536) && !has_code(&diags, 2339),
        "valid concrete-object accesses must be clean, got: {:?}",
        codes(&diags)
    );
}

/// Negative control: a string index signature accepts any string key, so an
/// anonymous object literal carrying one must not report a missing property.
#[test]
fn anonymous_object_with_string_index_signature_is_clean() {
    let diags = check(
        r#"
type Dict = { [k: string]: number };
type R = Dict["anything"];
"#,
    );
    assert!(
        !has_code(&diags, 2536) && !has_code(&diags, 2339),
        "string index signature must accept the key, got: {:?}",
        codes(&diags)
    );
}
