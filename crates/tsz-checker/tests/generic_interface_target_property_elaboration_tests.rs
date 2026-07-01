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

fn ts2322_all(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect()
}

/// A **fresh object literal** returned to a generic-interface target elaborates
/// per-property, mirroring `tsc`'s `elaborateObjectLiteral`: each incompatible
/// property gets its own TS2322 anchored at the property value with a direct
/// `Type 'X' is not assignable to type 'Y'.` message — not a single whole-object
/// `Type '{ ... }' is not assignable to type 'A<T>'.` wrapper (which would carry
/// a nested `Types of property ...` chain and surface only once).
///
/// Regression: because the generic target routed to the coarse
/// `target_prefers_outer_assignment_diagnostic` surface, the fresh-literal
/// source elaboration was skipped and the return collapsed to the single outer
/// wrapper, dropping the second property's error entirely. The variable-init
/// path already drilled; only the return/assignment path diverged.
#[test]
fn fresh_object_literal_return_to_generic_target_elaborates_per_property() {
    let diagnostics = ts2322_all(
        "interface A<T> { x: number; y: string; }\n\
         function make<T>(): A<T> { return { x: \"bad\", y: 5 }; }\n",
    );
    assert_eq!(
        diagnostics.len(),
        2,
        "expected one TS2322 per incompatible property, got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text == "Type 'string' is not assignable to type 'number'."),
        "expected the direct per-property message for 'x', got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text == "Type 'number' is not assignable to type 'string'."),
        "expected the direct per-property message for 'y', got {diagnostics:#?}"
    );
    // No whole-object wrapper: a per-property elaboration never nests a
    // `Types of property ...` frame.
    assert!(
        diagnostics
            .iter()
            .all(|d| !has_related(d, "Types of property")),
        "expected flat per-property diagnostics, not a nested wrapper, got {diagnostics:#?}"
    );
}

/// The per-property drill for a fresh literal is keyed on the structural
/// relation, not on any identifier: vary the interface name, the type-parameter
/// spelling, and the property names. Every variant elaborates both mismatches.
#[test]
fn fresh_object_literal_return_per_property_is_name_independent() {
    for (iface, type_param, num_prop, str_prop) in [
        ("Container", "Element", "head", "tail"),
        ("Wrapper", "K", "slot", "label"),
        ("Holder", "Payload", "count", "name"),
    ] {
        let source = format!(
            "interface {iface}<{type_param}> {{ {num_prop}: number; {str_prop}: string; }}\n\
             function build<{type_param}>(): {iface}<{type_param}> {{ \
                return {{ {num_prop}: \"bad\", {str_prop}: 5 }}; }}\n",
        );
        let diagnostics = ts2322_all(&source);
        assert_eq!(
            diagnostics.len(),
            2,
            "[{iface}/{type_param}] expected two per-property TS2322, got {diagnostics:#?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message_text == "Type 'string' is not assignable to type 'number'."),
            "[{iface}/{type_param}] expected '{num_prop}' message, got {diagnostics:#?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message_text == "Type 'number' is not assignable to type 'string'."),
            "[{iface}/{type_param}] expected '{str_prop}' message, got {diagnostics:#?}"
        );
    }
}

/// A property whose target type is the bare free type parameter and whose source
/// value already satisfies it is NOT flagged; only the genuinely incompatible
/// sibling property elaborates. This proves the drill uses the real per-property
/// relation rather than blanket-erroring every member of a generic target.
#[test]
fn fresh_object_literal_return_skips_satisfied_free_param_property() {
    let diagnostics = ts2322_all(
        "interface A<T> { x: T; y: string; }\n\
         function make<T>(t: T): A<T> { return { x: t, y: 5 }; }\n",
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected only the incompatible 'y' property to error, got {diagnostics:#?}"
    );
    assert_eq!(
        diagnostics[0].message_text, "Type 'number' is not assignable to type 'string'.",
        "expected the direct 'y' message, got {diagnostics:#?}"
    );
}

/// Guard against over-drilling: when the generic target is an unresolved
/// conditional application (`C<T>` with `T` still free), `tsc` keeps the single
/// whole-object `Type '{ ... }' is not assignable to type 'C<T>'.` wrapper
/// because there is no concrete member surface to elaborate into. The fix must
/// not manufacture per-property errors here.
#[test]
fn fresh_object_literal_return_to_unresolved_conditional_target_stays_outer() {
    let diagnostics = ts2322_all(
        "type C<T> = T extends string ? { a: number } : { b: string };\n\
         function make<T extends string>(): C<T> { return { a: \"bad\" }; }\n",
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected a single whole-object wrapper for the unresolved conditional target, \
         got {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].message_text.contains("C<T>"),
        "expected the outer wrapper naming the conditional target, got {diagnostics:#?}"
    );
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
