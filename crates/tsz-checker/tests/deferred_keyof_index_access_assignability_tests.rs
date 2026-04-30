//! Tests for assignability into deferred `O[K]` indexed-access targets where
//! `K` is a generic type parameter constrained by `keyof O`.
//!
//! Structural rule: a concrete source is assignable to `O[K]` (with K deferred)
//! iff it is assignable, by the same compatibility rules used elsewhere
//! (TS2322 / TS2741), to *every* value type of `O`. The fast-path check used
//! to route through the strict subtype relation, which rejected cases the
//! Lawyer (`CompatChecker`) would accept — most importantly, `{}` against an
//! object type with all-optional properties.
//!
//! Mirrors the line-5 false-positive shape from
//! `errorInfoForRelatedIndexTypesNoConstraintElaboration.ts` without relying
//! on the full JSX/react16 corpus.

use tsz_checker::test_utils::check_source_diagnostics;

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

/// `{}` assigned into `O[K]` (K deferred, O has a single all-optional generic
/// value type) must NOT emit TS2322.
#[test]
fn empty_object_assignable_to_deferred_indexed_optional_props() {
    let source = r#"
interface BaseProps<T> { id?: string; }
interface Things {
    a: BaseProps<number>;
}
class Holder<K extends keyof Things> {
    M() {
        let c1: Things[K] = {};
    }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "{{}} → Things[K] with all-optional value props must not emit TS2322; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(count(&diags, 2741), 0, "and no TS2741 either");
}

/// Same rule with a different bound-parameter name (anti-hardcoding directive
/// §25): the fix must not key on the literal `K`.
#[test]
fn empty_object_assignable_to_deferred_indexed_optional_props_alt_name() {
    let source = r#"
interface BaseProps<T> { id?: string; }
interface Things {
    a: BaseProps<number>;
}
class Holder<TagName extends keyof Things> {
    M() {
        let c1: Things[TagName] = {};
    }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "{{}} → Things[TagName] must not emit TS2322; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(count(&diags, 2741), 0);
}

/// Sibling positive: when value types have a *required* property, `{}` IS
/// rejected. tsc emits TS2322 on `let x: O[K] = {}`. tsz emits either TS2322
/// or TS2741 (missing required property), both are correct.
#[test]
fn empty_object_rejected_against_deferred_indexed_required_props() {
    let source = r#"
interface BaseProps<T> { id: T; }
interface Things {
    a: BaseProps<number>;
}
class Holder<K extends keyof Things> {
    M() {
        let c1: Things[K] = {};
    }
}
"#;
    let diags = check_source_diagnostics(source);
    let assignability_errors = count(&diags, 2322) + count(&diags, 2741);
    assert!(
        assignability_errors >= 1,
        "{{}} → Things[K] with required value prop must emit an assignability error; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Sibling positive (alt name) — same anti-hardcoding pairing.
#[test]
fn empty_object_rejected_against_deferred_indexed_required_props_alt_name() {
    let source = r#"
interface BaseProps<T> { id: T; }
interface Things {
    a: BaseProps<number>;
}
class Holder<TagName extends keyof Things> {
    M() {
        let c1: Things[TagName] = {};
    }
}
"#;
    let diags = check_source_diagnostics(source);
    let assignability_errors = count(&diags, 2322) + count(&diags, 2741);
    assert!(
        assignability_errors >= 1,
        "{{}} → Things[TagName] with required value prop must emit an assignability error"
    );
}

/// Multi-element shape: when only ONE value type has a required prop,
/// the rule still rejects (since K could pick that key). Two name choices
/// covered to keep the fix structural.
#[test]
fn multi_element_one_required_rejects_empty_object_two_names() {
    for tparam in ["K", "Tag"] {
        let source = format!(
            r#"
interface OptProps {{ id?: string; }}
interface ReqProps {{ id: string; }}
interface Things {{
    a: OptProps;
    b: ReqProps;
}}
class Holder<{tparam} extends keyof Things> {{
    M() {{
        let c1: Things[{tparam}] = {{}};
    }}
}}
"#
        );
        let diags = check_source_diagnostics(&source);
        let assignability_errors = count(&diags, 2322) + count(&diags, 2741);
        assert!(
            assignability_errors >= 1,
            "Things[{tparam}] with one required-prop value must reject {{}}; got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        );
    }
}

/// Negative companion: when ALL value types have only optional properties,
/// no assignability error fires regardless of how many elements the indexer
/// can hit.
#[test]
fn multi_element_all_optional_accepts_empty_object_two_names() {
    for tparam in ["K", "Tag"] {
        let source = format!(
            r#"
interface OptA {{ id?: string; }}
interface OptB {{ name?: number; }}
interface Things {{
    a: OptA;
    b: OptB;
}}
class Holder<{tparam} extends keyof Things> {{
    M() {{
        let c1: Things[{tparam}] = {{}};
    }}
}}
"#
        );
        let diags = check_source_diagnostics(&source);
        let assignability_errors = count(&diags, 2322) + count(&diags, 2741);
        assert_eq!(
            assignability_errors,
            0,
            "Things[{tparam}] with all-optional value props must accept {{}}; got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        );
    }
}
