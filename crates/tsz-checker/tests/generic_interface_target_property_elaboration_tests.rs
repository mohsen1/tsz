//! TS2322 elaboration must drill into the failing member when the target is a
//! generic-interface application (`Iface<T>`), mirroring `tsc`.
//!
//! Regression: a structural property mismatch against an instantiated generic
//! interface used to collapse to the bare outer
//! `Type 'S' is not assignable to type 'Iface<T>'.` line because the target was
//! routed to the "outer assignment" coarse diagnostic reserved for the
//! type-argument / indexed-access / mapped surfaces. `tsc` always elaborates a
//! genuine member mismatch (`Types of property 'p' are incompatible.` + the
//! nested root reason), so the structural property failure must keep its chain.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2322(source: &str) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS2322 diagnostic, got {diagnostics:#?}"
    );
    diagnostics.into_iter().next().unwrap()
}

fn has_related(diagnostic: &Diagnostic, expected: &str) -> bool {
    diagnostic
        .related_information
        .iter()
        .any(|related| related.message_text.contains(expected))
}

/// A concrete property mismatch against a generic-interface target drills into
/// the `Types of property 'p' are incompatible.` chain and the nested root
/// reason — not the bare outer line.
#[test]
fn concrete_member_mismatch_against_generic_interface_target_drills_in() {
    let diagnostic = ts2322(
        "interface Box<T> { value: string; tag: T; }\n\
         function make<T>(input: { value: number; tag: T }): Box<T> { return input; }\n",
    );
    assert!(
        has_related(&diagnostic, "Types of property 'value' are incompatible."),
        "expected the property-path wrapper, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'number' is not assignable to type 'string'."
        ),
        "expected the nested root reason, got {diagnostic:#?}"
    );
}

/// The drill-in is keyed on the structural relation, not on any identifier:
/// vary the interface name, the type-parameter spelling, and the failing
/// property name. Every variant must still elaborate the member mismatch.
#[test]
fn generic_interface_member_mismatch_drill_in_is_name_independent() {
    for (iface, type_param, prop) in [
        ("Container", "Element", "head"),
        ("Wrapper", "K", "slot"),
        ("Holder", "Payload", "data"),
    ] {
        let source = format!(
            "interface {iface}<{type_param}> {{ {prop}: string; rest: {type_param}; }}\n\
             function build<{type_param}>(\
                input: {{ {prop}: number; rest: {type_param} }}\
             ): {iface}<{type_param}> {{ return input; }}\n",
        );
        let diagnostic = ts2322(&source);
        let wrapper = format!("Types of property '{prop}' are incompatible.");
        assert!(
            has_related(&diagnostic, &wrapper),
            "[{iface}/{type_param}/{prop}] expected property-path wrapper, got {diagnostic:#?}"
        );
        assert!(
            has_related(
                &diagnostic,
                "Type 'number' is not assignable to type 'string'."
            ),
            "[{iface}/{type_param}/{prop}] expected nested root reason, got {diagnostic:#?}"
        );
    }
}

/// A missing required member against a generic-interface target keeps its
/// dedicated TS2741 elaboration rather than collapsing to the outer line.
#[test]
fn missing_member_against_generic_interface_target_is_preserved() {
    let diagnostic = check_source_diagnostics(
        "interface Pair<T> { left: T; right: string; }\n\
         function take<T>(input: { left: T }): Pair<T> { return input; }\n",
    )
    .into_iter()
    .find(|diagnostic| diagnostic.code == 2741 || diagnostic.code == 2739)
    .expect("expected a missing-property diagnostic (TS2741/TS2739)");
    assert!(
        diagnostic.message_text.contains("right"),
        "expected the missing 'right' member to be named, got {diagnostic:#?}"
    );
}
