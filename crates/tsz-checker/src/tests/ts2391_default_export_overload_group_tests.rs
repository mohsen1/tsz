//! Regression tests for statement-level function-implementation grouping of
//! exported function declarations (#16723).
//!
//! Structural rule (pinned against `typescript@7.0.2` with
//! `--strict false --module commonjs --target es2015`): the parser wraps
//! `export function ...` and `export default function ...` in an
//! `EXPORT_DECLARATION` node, and the checker's statement walk must see
//! through that wrapper — an exported function declaration joins the same
//! name-keyed overload grouping as a bare one (`TS2391`/`TS2389`), exactly as
//! tsc's per-local-name `checkFunctionOrConstructorSymbol` run does.
//!
//! Additionally, tsc binds every `export default function` — whatever its
//! local name — to the single `default` export symbol and runs the same
//! validation over that merged list: anonymous signatures group there
//! (`TS2391` anchored at the whole statement), a body-carrying member whose
//! group continues past a textual gap is marked too, and every bodyless
//! signature is checked against the group's *first* implementation with only
//! the first incompatible one reported (`TS2394`). tsz implements the merged
//! half in `check_default_export_function_group`
//! (`state_checking_members/default_export_overload_group.rs`).
//!
//! Binder names vary across rows; no row depends on identifier spelling.
//! Mixed default/non-default same-name runs and plain export/non-export flag
//! agreement (`TS2383`) belong to a different diagnostic family and are
//! covered by the `mixed_export_visibility` tests below.

use crate::context::ScriptTarget;
use crate::test_utils::{DiagnosticShape, assert_diagnostic_shapes_exactly, check_source};
use crate::{CheckerOptions, diagnostics::Diagnostic};

fn check_module(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
}

/// Diagnostics restricted to the function-implementation grouping family, so
/// rows stay pinned even when an unrelated family fires on the same fixture.
fn family(source: &str) -> Vec<Diagnostic> {
    check_module(source)
        .into_iter()
        .filter(|d| matches!(d.code, 2389 | 2391 | 2394))
        .collect()
}

fn assert_family_exactly(source: &str, shapes: &[DiagnosticShape]) {
    assert_diagnostic_shapes_exactly(source, &family(source), shapes);
}

/// #16723 witness: a cross-name default-exported signature is an orphan for
/// its own local name (TS2391) and incompatible with the merged group's
/// implementation (TS2394).
///
/// ```text
/// tsc: (1,25) TS2391 + TS2394
/// ```
#[test]
fn cross_name_default_signature_gets_orphan_and_incompatibility() {
    let source = "export default function f(a: string): string;\n\
                  export default function g(a: number): number;\n\
                  export default function g(a: number): number { return a; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2391).at(1, 25),
            DiagnosticShape::code(2394).at(1, 25),
        ],
    );
}

/// #16723 witness: two cross-name bodyless defaults each orphan their own
/// local name.
#[test]
fn two_cross_name_signature_only_defaults_each_get_ts2391() {
    let source = "export default function head(a: string): string;\n\
                  export default function tail(a: number): number;\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2391).at(1, 25),
            DiagnosticShape::code(2391).at(2, 25),
        ],
    );
}

/// A same-named default-exported overload set is clean, exactly like a bare
/// one.
#[test]
fn same_named_default_overload_set_is_clean() {
    let source = "export default function make(a: string): string;\n\
                  export default function make(a: string): string { return a; }\n";
    assert_family_exactly(source, &[]);
}

/// A compatible cross-name implementation adjacent to the signature reports
/// the wrong-name diagnostic, not the orphan one.
#[test]
fn adjacent_cross_name_implementation_reports_ts2389() {
    let source = "export default function alpha(a: string): string;\n\
                  export default function beta(a: string): string { return a; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2389).at(2, 25)]);
}

/// Order flip: the implementation first, then an incompatible signature —
/// terminal TS2391 plus merged-group TS2394, both at the signature.
#[test]
fn implementation_before_incompatible_signature() {
    let source = "export default function beta(a: number): number { return a; }\n\
                  export default function alpha(a: string): string;\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2391).at(2, 25),
            DiagnosticShape::code(2394).at(2, 25),
        ],
    );
}

/// A compatible cross-name signature gets only the orphan diagnostic — no
/// TS2394 when the merged implementation accepts it.
#[test]
fn compatible_cross_name_signature_gets_only_ts2391() {
    let source = "export default function one(a: number): number;\n\
                  export default function two(a: number): number;\n\
                  export default function two(a: number): number { return a; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2391).at(1, 25)]);
}

/// Same-name incompatible pair: the classic TS2394, now reachable through the
/// export wrapper.
#[test]
fn same_named_incompatible_default_pair_reports_ts2394() {
    let source = "export default function pick(a: string): number;\n\
                  export default function pick(a: string): string { return a; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2394).at(1, 25)]);
}

/// Three names: only the FIRST incompatible signature of the merged group is
/// reported (tsc breaks after one TS2394 per symbol), and the middle name's
/// orphan resolves to a wrong-name report at the adjacent implementation.
#[test]
fn merged_group_reports_only_first_incompatible_signature() {
    let source = "export default function f(a: string): string;\n\
                  export default function g(a: number): number;\n\
                  export default function h(a: number): number { return a; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2391).at(1, 25),
            DiagnosticShape::code(2394).at(1, 25),
            DiagnosticShape::code(2389).at(3, 25),
        ],
    );
}

/// A non-function statement between the signature and the group's
/// implementation is a textual gap: orphan plus incompatibility at the
/// signature.
#[test]
fn statement_gap_before_implementation_is_an_orphan() {
    let source = "export default function f(a: string): string;\n\
                  const x = 1;\n\
                  export default function g(a: number): number { return a; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2391).at(1, 25),
            DiagnosticShape::code(2394).at(1, 25),
        ],
    );
}

/// A gap after a body-carrying member whose group continues also marks the
/// implementation itself (oracle-pinned: tsc reports both sites).
#[test]
fn gap_after_implementation_marks_the_implementation_too() {
    let source = "export default function beta(a: number): number { return a; }\n\
                  const z = 1;\n\
                  export default function alpha(a: string): string;\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2391).at(1, 25),
            DiagnosticShape::code(2391).at(3, 25),
            DiagnosticShape::code(2394).at(3, 25),
        ],
    );
}

/// A same-named implementation separated from its signature by a statement is
/// only the signature's orphan — the implementation is not marked.
#[test]
fn same_name_gap_reports_only_the_signature() {
    let source = "export default function a1(x: string): string;\n\
                  const q = 1;\n\
                  export default function a1(x: string): string { return x; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2391).at(1, 25)]);
}

/// An anonymous default signature anchors its orphan at the whole statement.
#[test]
fn anonymous_default_signature_anchors_at_the_statement() {
    let source = "export default function (a: string): string;\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2391).at(1, 1)]);
}

/// Consecutive anonymous bodyless defaults report only the last one, like a
/// same-named overload run.
#[test]
fn consecutive_anonymous_signatures_report_only_the_last() {
    let source = "export default function (a: string): string;\n\
                  export default function (a: string): string;\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2391).at(2, 1)]);
}

/// An anonymous signature satisfied by an adjacent anonymous implementation is
/// clean.
#[test]
fn anonymous_signature_with_anonymous_implementation_is_clean() {
    let source = "export default function (a: string): string;\n\
                  export default function (a: string): string { return a; }\n";
    assert_family_exactly(source, &[]);
}

/// An anonymous signature satisfied by an adjacent NAMED implementation is
/// clean too — the merged `default` group does not care about local names.
#[test]
fn anonymous_signature_with_named_implementation_is_clean() {
    let source = "export default function (a: string): string;\n\
                  export default function g(a: string): string { return a; }\n";
    assert_family_exactly(source, &[]);
}

/// A named signature followed by an anonymous implementation demands the
/// name, anchored at the implementation statement.
#[test]
fn named_signature_with_anonymous_implementation_reports_ts2389() {
    let source = "export default function s(a: string): string;\n\
                  export default function (a: string): string { return a; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2389).at(2, 1)]);
}

/// An anonymous signature incompatible with the anonymous implementation gets
/// TS2394 at the signature statement.
#[test]
fn anonymous_incompatible_signature_reports_ts2394_at_the_statement() {
    let source = "export default function (a: string): string;\n\
                  export default function (a: number): number { return a; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2394).at(1, 1)]);
}

/// A signature whose own name has a compatible implementation still checks
/// against the merged group's FIRST body: `q`'s signature is validated
/// against `p`'s implementation. The redeclare family keeps its own sites.
#[test]
fn signature_checks_against_the_groups_first_body() {
    let source = "export default function p(a: string): string;\n\
                  export default function p(a: string): string { return a; }\n\
                  export default function q(a: number): number;\n\
                  export default function q(a: number): number { return a; }\n";
    let observed: Vec<Diagnostic> = check_module(source)
        .into_iter()
        .filter(|d| matches!(d.code, 2389 | 2391 | 2394 | 2393 | 2323))
        .collect();
    assert_diagnostic_shapes_exactly(
        source,
        &observed,
        &[
            DiagnosticShape::code(2393).at(1, 25),
            DiagnosticShape::code(2323).at(2, 25),
            DiagnosticShape::code(2393).at(2, 25),
            DiagnosticShape::code(2394).at(3, 25),
            DiagnosticShape::code(2393).at(3, 25),
            DiagnosticShape::code(2323).at(4, 25),
            DiagnosticShape::code(2393).at(4, 25),
        ],
    );
}

/// Non-default `export function` declarations join the grouping too: a
/// cross-name exported implementation reports the wrong-name diagnostic.
#[test]
fn exported_cross_name_implementation_reports_ts2389() {
    let source = "export function a(x: string): string;\n\
                  export function b(x: string): string { return x; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2389).at(2, 17)]);
}

/// A lone exported bodyless signature is an orphan, exactly like a bare one.
#[test]
fn lone_exported_signature_reports_ts2391() {
    let source = "export function solo2(a: string): string;\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2391).at(1, 17)]);
}

/// Positive controls: the bare-function paths keep their behavior.
#[test]
fn bare_function_grouping_is_unchanged() {
    let orphan = "function solo(a: string): string;\n";
    assert_family_exactly(orphan, &[DiagnosticShape::code(2391).at(1, 10)]);

    let cross = "function a(x: string): string;\n\
                 function b(x: string): string { return x; }\n";
    assert_family_exactly(cross, &[DiagnosticShape::code(2389).at(2, 10)]);
}

/// Exported signatures inside a namespace body group as well.
#[test]
fn namespace_exported_signatures_join_the_grouping() {
    let orphan = "namespace N {\n\
                  \x20   export function inner(a: string): string;\n\
                  }\n\
                  export {};\n";
    assert_family_exactly(orphan, &[DiagnosticShape::code(2391).at(2, 21)]);

    let cross = "namespace M {\n\
                 \x20   export function u(a: string): string;\n\
                 \x20   export function v(a: string): string { return a; }\n\
                 }\n\
                 export {};\n";
    assert_family_exactly(cross, &[DiagnosticShape::code(2389).at(3, 21)]);
}

/// Negative controls: ambient declarations stay silent — a `declare module`
/// body full of bodyless defaults and an `export declare` signature are fine.
#[test]
fn ambient_signatures_stay_clean() {
    let ambient_module = "declare module \"remote\" {\n\
                          \x20   export default function connect(host: string): void;\n\
                          \x20   export default function listen(port: number): void;\n\
                          }\n";
    assert_family_exactly(ambient_module, &[]);

    let export_declare = "export declare function amb(a: string): string;\n";
    assert_family_exactly(export_declare, &[]);
}

/// Negative control: a single default-exported implementation reports nothing
/// from this family.
#[test]
fn single_default_implementation_is_clean() {
    let source = "export default function only(a: string) { return a; }\n";
    assert_family_exactly(source, &[]);
}

/// #16742: a same-named function overload run mixing `export default` and
/// plain (non-exported) declarations is a flag-agreement mismatch (`TS2383`),
/// not a merged-declaration/default-export conflict. tsc reports exactly one
/// `TS2383` here; tsz previously reported two spurious `TS2652`s instead
/// (`MERGED_DECLARATION_CANNOT_INCLUDE_A_DEFAULT_EXPORT_DECLARATION...`)
/// because the merged-declaration scan's `all_functions` exemption only
/// covered `TS2395`, not the analogous `TS2652` default-export branch.
mod mixed_export_visibility {
    use crate::context::ScriptTarget;
    use crate::test_utils::{DiagnosticShape, assert_diagnostic_shapes_exactly, check_source};
    use crate::{CheckerOptions, diagnostics::Diagnostic};

    fn check_module(source: &str) -> Vec<Diagnostic> {
        check_source(
            source,
            "test.ts",
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..CheckerOptions::default()
            },
        )
    }

    /// Diagnostics restricted to the export/default-export flag-agreement
    /// family (`TS2383`, `TS2395`, `TS2652`).
    fn family(source: &str) -> Vec<Diagnostic> {
        check_module(source)
            .into_iter()
            .filter(|d| matches!(d.code, 2383 | 2395 | 2652))
            .collect()
    }

    fn assert_family_exactly(source: &str, shapes: &[DiagnosticShape]) {
        assert_diagnostic_shapes_exactly(source, &family(source), shapes);
    }

    /// Default-exported signature, plain (non-exported) implementation: only
    /// `TS2383` at the deviating signature, never `TS2652`.
    #[test]
    fn default_exported_signature_then_plain_implementation_reports_only_ts2383() {
        let source = "export default function fn(a: string): string;\n\
                      function fn(a: string): string { return a; }\n\
                      export {};\n";
        assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 25)]);
    }

    /// Order flipped: plain signature first, default-exported implementation
    /// second — same single `TS2383`, anchored at the signature.
    #[test]
    fn plain_signature_then_default_exported_implementation_reports_only_ts2383() {
        let source = "function fn(a: string): string;\n\
                      export default function fn(a: string): string { return a; }\n\
                      export {};\n";
        assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 10)]);
    }

    /// Renamed binder: the same shape under a different local name stays
    /// clean of `TS2652`. The name's own length doesn't shift the anchor —
    /// `export default function ` is a fixed-width prefix, so the signature's
    /// name always starts at column 25 regardless of spelling.
    #[test]
    fn default_exported_signature_then_plain_implementation_renamed_binder() {
        let source = "export default function widget(a: number): number;\n\
                      function widget(a: number): number { return a; }\n\
                      export {};\n";
        assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 25)]);
    }

    /// Plain `export` (non-default) signature mixed with a non-exported
    /// implementation: still `TS2383`, not `TS2395`/`TS2652` — `all_functions`
    /// groups never hit the merged-declaration branches.
    #[test]
    fn exported_signature_then_plain_implementation_reports_only_ts2383() {
        let source = "export function c(x: string): string;\n\
                      function c(x: string): string { return x; }\n";
        assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 17)]);
    }

    /// Positive control: a default-exported class colliding with a local
    /// interface in TYPE space is a genuine merged-declaration conflict
    /// (`defaultExportsCannotMerge03`-shaped, pinned separately in
    /// `tests/default_export_merge_diagnostics_tests.rs`) — not an
    /// `all_functions` group, so the `TS2652` exemption must not apply here.
    #[test]
    fn default_exported_class_and_local_interface_still_reports_ts2652() {
        let source = "export default class Model {}\n\
                      interface Model {}\n";
        assert_family_exactly(
            source,
            &[
                DiagnosticShape::code(2652).at(1, 22),
                DiagnosticShape::code(2652).at(2, 11),
            ],
        );
    }
}
