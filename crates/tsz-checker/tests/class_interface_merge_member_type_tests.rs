//! Class+interface declaration-merge member type/signature compatibility.
//!
//! When a class and an interface of the same name merge and declare a
//! same-named instance member with conflicting types, tsc reports TS2717
//! (property type conflict) or TS2394 (interface method signature incompatible
//! with the class method implementation). These tests pin that parity, which
//! was previously a silent false-negative (issue #14798).

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    get_diagnostics(source).iter().map(|d| d.0).collect()
}

fn has_code(source: &str, code: u32) -> bool {
    codes(source).contains(&code)
}

fn message_for(source: &str, code: u32) -> Option<String> {
    get_diagnostics(source)
        .into_iter()
        .find(|d| d.0 == code)
        .map(|d| d.1)
}

// --- TS2394: interface method vs class method implementation ----------------

#[test]
fn method_return_conflict_reports_ts2394_interface_first() {
    let source = r#"
interface B { m(): number; }
class B { m(): string { return ""; } }
"#;
    assert!(
        has_code(source, 2394),
        "expected TS2394, got {:?}",
        codes(source)
    );
}

#[test]
fn method_return_conflict_reports_ts2394_class_first() {
    let source = r#"
class B { m(): string { return ""; } }
interface B { m(): number; }
"#;
    assert!(
        has_code(source, 2394),
        "expected TS2394, got {:?}",
        codes(source)
    );
}

#[test]
fn method_return_compatible_is_clean() {
    let source = r#"
interface C { m(): number; }
class C { m(): number { return 0; } }
"#;
    assert!(
        !has_code(source, 2394),
        "unexpected TS2394: {:?}",
        codes(source)
    );
    assert!(
        !has_code(source, 2717),
        "unexpected TS2717: {:?}",
        codes(source)
    );
}

#[test]
fn method_param_conflict_reports_ts2394() {
    // The implementation accepts a narrower param than the interface overload
    // requires, so the overload is not satisfiable by the implementation.
    let source = r#"
interface B { m(x: number): void; }
class B { m(x: string): void {} }
"#;
    assert!(
        has_code(source, 2394),
        "expected TS2394, got {:?}",
        codes(source)
    );
}

// --- TS2717: interface property vs class field ------------------------------

#[test]
fn property_type_conflict_reports_ts2717_interface_first() {
    let source = r#"
interface D { p: number; }
class D { p = "x"; }
"#;
    assert!(
        has_code(source, 2717),
        "expected TS2717, got {:?}",
        codes(source)
    );
    let msg = message_for(source, 2717).unwrap_or_default();
    assert!(
        msg.contains("Property 'p'")
            && msg.contains("must be of type 'number'")
            && msg.contains("here has type 'string'"),
        "unexpected TS2717 message: {msg}"
    );
}

#[test]
fn property_type_conflict_reports_ts2717_class_first() {
    let source = r#"
class D { p = "x"; }
interface D { p: number; }
"#;
    assert!(
        has_code(source, 2717),
        "expected TS2717, got {:?}",
        codes(source)
    );
    // Now the class field is the canonical (first) declaration: it must be
    // `string`, and the interface property is the one that disagrees.
    let msg = message_for(source, 2717).unwrap_or_default();
    assert!(
        msg.contains("must be of type 'string'") && msg.contains("here has type 'number'"),
        "unexpected TS2717 message: {msg}"
    );
}

#[test]
fn property_annotation_conflict_reports_ts2717() {
    let source = r#"
interface D { p: number; }
class D { p: string = "x"; }
"#;
    assert!(
        has_code(source, 2717),
        "expected TS2717, got {:?}",
        codes(source)
    );
}

#[test]
fn property_type_compatible_is_clean() {
    let source = r#"
interface E { p: number; }
class E { p = 1; }
"#;
    assert!(
        !has_code(source, 2717),
        "unexpected TS2717: {:?}",
        codes(source)
    );
}

// --- Scope guards: static + binder-name independence ------------------------

#[test]
fn static_class_member_does_not_merge_with_interface() {
    // A static class member lives on the constructor side and never merges
    // with the interface instance member, so no conflict is reported.
    let source = r#"
interface F { p: number; }
class F { static p = "x"; p = 1; }
"#;
    assert!(
        !has_code(source, 2717),
        "unexpected TS2717: {:?}",
        codes(source)
    );
}

#[test]
fn conflict_is_structural_not_name_keyed() {
    // Renaming the merged binder must not change the outcome: the rule is
    // structural, not keyed off any particular identifier.
    let source = r#"
interface Widget { compute(): number; value: number; }
class Widget {
    compute(): string { return ""; }
    value = "x";
}
"#;
    assert!(
        has_code(source, 2394),
        "expected TS2394, got {:?}",
        codes(source)
    );
    assert!(
        has_code(source, 2717),
        "expected TS2717, got {:?}",
        codes(source)
    );
}

#[test]
fn fully_compatible_merge_is_clean() {
    let source = r#"
interface G { compute(): number; value: number; }
class G {
    compute(): number { return 0; }
    value = 1;
}
"#;
    assert!(
        !has_code(source, 2394),
        "unexpected TS2394: {:?}",
        codes(source)
    );
    assert!(
        !has_code(source, 2717),
        "unexpected TS2717: {:?}",
        codes(source)
    );
}
