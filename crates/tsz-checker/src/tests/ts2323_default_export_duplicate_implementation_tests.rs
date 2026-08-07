//! Regression tests for the `TS2323`/`TS2393`/`TS2528` classification of
//! duplicate default-exported function implementations (#16719).
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! tsc binds every `export default function` declaration — whatever its local
//! name — to the single `default` export symbol, so a module whose conflicting
//! default exports are all function declarations never reports `TS2528`.
//! Duplicate bodies surface instead as the redeclare family:
//!
//! - `TS2393` ("Duplicate function implementation.") on **every** function
//!   declaration in the set, overload signatures included, once two or more
//!   declarations carry bodies;
//! - `TS2323` ("Cannot redeclare exported variable 'default'.") on each
//!   declaration that **carries a body** — never on a signature.
//!
//! Both anchor at the function name, or at the statement when the function is
//! anonymous. When a non-function default export is also present, `TS2528`
//! still fires at every default site alongside the redeclare family. In the
//! function/class merge arm, `TS2323` likewise marks only value sites (bodies
//! and the class), so a signature-only overload set beside a class reports the
//! merge family alone.
//!
//! tsz decides all of this in the checker's export-default pass
//! (`declarations/import/core/module_exports.rs`).
//!
//! Every row below was measured against the pin with
//! `--strict false --module commonjs --target es2015`. Binder names are varied
//! across rows: the rule is structural, so no row may depend on a particular
//! identifier spelling. Known, deliberately unasserted gap: tsc also runs
//! overload-consistency validation over the merged `default` symbol
//! (`TS2391`/`TS2394` on signature-only groups); tsz does not yet.

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

fn codes(source: &str) -> Vec<u32> {
    check_module(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

/// The issue witness: two same-named implementations are duplicate
/// implementations of the one `default` symbol, not multiple default exports.
///
/// ```text
/// tsc: (1,25) TS2323 + TS2393, (2,25) TS2323 + TS2393
/// ```
#[test]
fn two_same_named_implementations_report_the_redeclare_family() {
    let source = "export default function handler(x: string) { return x; }\n\
                  export default function handler(x: number) { return x; }\n";
    assert_diagnostic_shapes_exactly(
        source,
        &check_module(source),
        &[
            DiagnosticShape::code(2323).at(1, 25),
            DiagnosticShape::code(2393).at(1, 25),
            DiagnosticShape::code(2323).at(2, 25),
            DiagnosticShape::code(2393).at(2, 25),
        ],
    );
}

/// The local names do not matter — tsc merges by the export name `default`,
/// so differently-named implementations are the exact same conflict.
#[test]
fn differently_named_implementations_are_the_same_conflict() {
    let source = "export default function alpha(a: string): string { return a; }\n\
                  export default function omega(a: number): number { return a; }\n";
    assert_diagnostic_shapes_exactly(
        source,
        &check_module(source),
        &[
            DiagnosticShape::code(2323).at(1, 25),
            DiagnosticShape::code(2393).at(1, 25),
            DiagnosticShape::code(2323).at(2, 25),
            DiagnosticShape::code(2393).at(2, 25),
        ],
    );
}

/// A signature in the run gets `TS2393` (tsc marks every declaration of the
/// merged symbol) but never `TS2323` (a signature redeclares nothing).
#[test]
fn an_overload_signature_gets_ts2393_but_never_ts2323() {
    let source = "export default function make(a: string): string;\n\
                  export default function make(a: string | number) { return a; }\n\
                  export default function build(a: number): number { return a; }\n";
    assert_diagnostic_shapes_exactly(
        source,
        &check_module(source),
        &[
            DiagnosticShape::code(2393).at(1, 25),
            DiagnosticShape::code(2323).at(2, 25),
            DiagnosticShape::code(2393).at(2, 25),
            DiagnosticShape::code(2323).at(3, 25),
            DiagnosticShape::code(2393).at(3, 25),
        ],
    );
}

/// Anonymous implementations anchor at the statement, exactly like tsc.
#[test]
fn anonymous_implementations_anchor_at_the_statement() {
    let source = "export default function (a: string) { return a; }\n\
                  export default function (a: number) { return a; }\n";
    assert_diagnostic_shapes_exactly(
        source,
        &check_module(source),
        &[
            DiagnosticShape::code(2323).at(1, 1),
            DiagnosticShape::code(2393).at(1, 1),
            DiagnosticShape::code(2323).at(2, 1),
            DiagnosticShape::code(2393).at(2, 1),
        ],
    );
}

/// With at most one body across multiple signature groups nothing in this
/// family fires: no `TS2528` (all defaults are functions), no `TS2323`, and no
/// `TS2393` (only one implementation).
#[test]
fn signature_only_groups_report_nothing_from_the_default_export_pass() {
    let source = "export default function req(a: string): string;\n\
                  export default function res(a: number): number;\n\
                  export default function res(a: number): number { return a; }\n";
    let observed = codes(source);
    for code in [2528, 2323, 2393] {
        assert_eq!(
            observed.iter().filter(|c| **c == code).count(),
            0,
            "a run of signature groups with one body is left to overload \
             validation; TS{code} must not fire. Got: {observed:?}"
        );
    }
}

/// Two bodyless signatures with different names likewise stay clear of the
/// multiple-default complaint entirely.
#[test]
fn two_signature_only_defaults_never_report_ts2528() {
    let source = "export default function first(a: string): string;\n\
                  export default function second(a: number): number;\n";
    let observed = codes(source);
    for code in [2528, 2323, 2393] {
        assert_eq!(
            observed.iter().filter(|c| **c == code).count(),
            0,
            "two signature-only defaults are one merged symbol short of an \
             implementation, not a conflict; TS{code} must not fire. \
             Got: {observed:?}"
        );
    }
}

/// The same rule holds inside an ambient module, where no declaration can
/// carry a body at all.
#[test]
fn ambient_module_signature_defaults_stay_clean() {
    let source = "declare module \"remote\" {\n\
                  \x20   export default function connect(host: string): void;\n\
                  \x20   export default function listen(port: number): void;\n\
                  }\n";
    let observed = codes(source);
    assert_eq!(
        observed.iter().filter(|c| **c == 2528).count(),
        0,
        "ambient signature-only defaults merge into one symbol. Got: {observed:?}"
    );
}

/// Positive control: a non-function default beside two implementations keeps
/// `TS2528` at every default site, on top of the redeclare family.
///
/// ```text
/// tsc: (1,25) TS2323 + TS2393 + TS2528, (2,25) TS2323 + TS2393 + TS2528,
///      (3,1) TS2528
/// ```
#[test]
fn a_value_default_beside_two_implementations_keeps_ts2528_everywhere() {
    let source = "export default function render(a: string) { return a; }\n\
                  export default function paint(a: number) { return a; }\n\
                  export default 1;\n";
    assert_diagnostic_shapes_exactly(
        source,
        &check_module(source),
        &[
            DiagnosticShape::code(2323).at(1, 25),
            DiagnosticShape::code(2393).at(1, 25),
            DiagnosticShape::code(2528).at(1, 25),
            DiagnosticShape::code(2323).at(2, 25),
            DiagnosticShape::code(2393).at(2, 25),
            DiagnosticShape::code(2528).at(2, 25),
            DiagnosticShape::code(2528).at(3, 1),
        ],
    );
}

/// Function/class merge arm: a signature-only overload set beside a class has
/// no value site pair, so no `TS2323` fires — only the merge family.
#[test]
fn a_signature_only_function_beside_a_class_reports_no_redeclare() {
    let source = "export default function zeta(a: string): string;\n\
                  export default class Store {}\n";
    let observed = codes(source);
    assert_eq!(
        observed.iter().filter(|c| **c == 2323).count(),
        0,
        "a signature redeclares nothing, and one class alone is not a \
         redeclaration pair. Got: {observed:?}"
    );
    assert!(
        observed.contains(&2813) && observed.contains(&2814),
        "the merge family itself still fires. Got: {observed:?}"
    );
}

/// Function/class merge arm: the body and the class are the two value sites;
/// the signature in front of them still gets no `TS2323`.
///
/// ```text
/// tsc: (1,25) TS2814, (2,25) TS2323 + TS2814, (3,22) TS2323 + TS2813
/// ```
#[test]
fn a_signature_in_a_function_class_merge_gets_no_redeclare() {
    let source = "export default function open(a: string): string;\n\
                  export default function open(a: string) { return a; }\n\
                  export default class Session {}\n";
    assert_diagnostic_shapes_exactly(
        source,
        &check_module(source),
        &[
            DiagnosticShape::code(2814).at(1, 25),
            DiagnosticShape::code(2323).at(2, 25),
            DiagnosticShape::code(2814).at(2, 25),
            DiagnosticShape::code(2323).at(3, 22),
            DiagnosticShape::code(2813).at(3, 22),
        ],
    );
}

/// Guard: a single implementation beside a class keeps both redeclare sites —
/// the value-site filter must not eat the plain function/class conflict.
#[test]
fn an_implementation_beside_a_class_keeps_both_redeclare_sites() {
    let source = "export default function fetch_(a: string) { return a; }\n\
                  export default class Client {}\n";
    let observed = codes(source);
    assert_eq!(
        observed.iter().filter(|c| **c == 2323).count(),
        2,
        "one body plus one class are two value sites. Got: {observed:?}"
    );
}

/// Guard: a lone default-exported implementation reports nothing at all.
#[test]
fn a_single_default_implementation_stays_clean() {
    let source = "export default function only(a: string) { return a; }\n";
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "one default export is no conflict"
    );
}
