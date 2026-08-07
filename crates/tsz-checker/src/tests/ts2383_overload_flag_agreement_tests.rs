//! Overload-group modifier flag agreement across export and ambient axes
//! (#16742): `TS2383` / `TS2384`, and the `TS2652` exemption for pure
//! function overload groups.
//!
//! Structural rule (every row pinned against `typescript@7.0.2` with
//! `--strict false --module commonjs --target es2015`): tsc's
//! `checkFunctionOrConstructorSymbol` accumulates export/ambient modifier
//! flags over a symbol's function-like declarations — implementation
//! included — and, when the group has at least one bodyless overload
//! signature and the flags disagree, blames every declaration of the merged
//! symbol whose flags deviate from the canonical declaration (the
//! implementation when it shares a statement container with the first
//! overload). A declaration deviating on both axes reports only `TS2383`:
//! the export mismatch takes precedence over the ambient one. A group made
//! entirely of function declarations is an overload group, not a merged
//! declaration, so default-export disagreement there never reports `TS2652`;
//! one non-function member (namespace, class, variable) restores the
//! merged-declaration family. tsz implements both halves in
//! `check_duplicate_identifiers` (`types/type_checking/duplicate_identifiers.rs`).
//!
//! Binder names vary across rows; no row depends on identifier spelling.

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

/// Diagnostics restricted to the flag-agreement and merged-declaration
/// visibility family, so rows stay pinned even when an unrelated family
/// (`TS2393` duplicate implementations, `TS2394` compatibility) fires on the
/// same fixture.
fn family(source: &str) -> Vec<Diagnostic> {
    check_module(source)
        .into_iter()
        .filter(|d| matches!(d.code, 2383 | 2384 | 2395 | 2652))
        .collect()
}

fn assert_family_exactly(source: &str, shapes: &[DiagnosticShape]) {
    assert_diagnostic_shapes_exactly(source, &family(source), shapes);
}

// -- TS2383: a single deviating signature against an implementation ---------

/// #16742 witness: a default-exported signature above a plain implementation
/// deviates from the implementation's (canonical) flags. `TS2652` must NOT
/// fire: a pure function group is one overload group, not a merged
/// declaration.
///
/// ```text
/// tsc: (1,25) TS2383
/// ```
#[test]
fn default_exported_signature_plain_impl_reports_ts2383_not_ts2652() {
    let source = "export default function fn(a: string): string;\n\
                  function fn(a: string): string { return a; }\n\
                  export {};\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 25)]);
}

/// #16742 witness: one exported signature above a plain implementation.
///
/// ```text
/// tsc: (1,17) TS2383
/// ```
#[test]
fn exported_signature_plain_impl_reports_ts2383() {
    let source = "export function collect(x: string): string;\n\
                  function collect(x: string): string { return x; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 17)]);
}

/// Reverse order: a plain signature above a default-exported implementation.
/// The implementation is canonical, so the signature is the deviator.
///
/// ```text
/// tsc: (1,10) TS2383
/// ```
#[test]
fn plain_signature_default_exported_impl_reports_ts2383_at_signature() {
    let source = "function shape(a: string): string;\n\
                  export default function shape(a: string): string { return a; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 10)]);
}

/// Reverse order with a plain `export` implementation.
///
/// ```text
/// tsc: (1,10) TS2383
/// ```
#[test]
fn plain_signature_exported_impl_reports_ts2383_at_signature() {
    let source = "function draw(x: string): string;\n\
                  export function draw(x: string): string { return x; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 10)]);
}

// -- Two duplicate implementations carry no flag-agreement error ------------

/// Two bodies and no bodyless signature: `TS2393` territory (other family) —
/// no `TS2383`, and no `TS2652` despite the default/non-default mix.
#[test]
fn default_impl_plus_plain_impl_reports_neither_ts2383_nor_ts2652() {
    let source = "export default function pair(a: string): string { return a; }\n\
                  function pair(a: number): number { return a; }\n\
                  export {};\n";
    assert_family_exactly(source, &[]);
}

/// Same, with a plain `export` implementation.
#[test]
fn exported_impl_plus_plain_impl_reports_no_flag_agreement_error() {
    let source = "export function twice(x: string): string { return x; }\n\
                  function twice(x: number): number { return x; }\n";
    assert_family_exactly(source, &[]);
}

// -- Multi-signature groups: every deviator is blamed -----------------------

/// Mixed signatures: only the signature deviating from the (non-exported)
/// implementation is blamed.
///
/// ```text
/// tsc: (1,17) TS2383
/// ```
#[test]
fn mixed_signatures_blame_only_the_deviating_one() {
    let source = "export function mix(x: string): string;\n\
                  function mix(x: number): number;\n\
                  function mix(x: any): any { return x; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 17)]);
}

/// Both exported signatures deviate from the non-exported implementation.
///
/// ```text
/// tsc: (1,17) TS2383, (2,17) TS2383
/// ```
#[test]
fn two_exported_signatures_plain_impl_blame_both_signatures() {
    let source = "export function fold(x: string): string;\n\
                  export function fold(x: number): number;\n\
                  function fold(x: any): any { return x; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2383).at(1, 17),
            DiagnosticShape::code(2383).at(2, 17),
        ],
    );
}

/// Both plain signatures deviate from the exported (canonical)
/// implementation.
///
/// ```text
/// tsc: (1,10) TS2383, (2,10) TS2383
/// ```
#[test]
fn two_plain_signatures_exported_impl_blame_both_signatures() {
    let source = "function seek(x: number): string;\n\
                  function seek(x: string): string;\n\
                  export function seek(x: any): string { return \"\"; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2383).at(1, 10),
            DiagnosticShape::code(2383).at(2, 10),
        ],
    );
}

/// A deviating exported implementation in a duplicate-implementation run is
/// blamed alongside the deviating signature (canonical = first body).
///
/// ```text
/// tsc: (1,17) TS2383, (3,17) TS2383 (TS2393/TS2394 are other-family)
/// ```
#[test]
fn deviating_second_implementation_is_blamed_too() {
    let source = "export function wave(x: string): string;\n\
                  function wave(x: any): any { return x; }\n\
                  export function wave(x: number): number { return x; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2383).at(1, 17),
            DiagnosticShape::code(2383).at(3, 17),
        ],
    );
}

// -- Uniform groups stay clean ----------------------------------------------

#[test]
fn uniformly_exported_group_is_clean() {
    let source = "export function all(x: string): string;\n\
                  export function all(x: number): number;\n\
                  export function all(x: any): any { return x; }\n";
    assert_family_exactly(source, &[]);
}

#[test]
fn uniformly_default_exported_pair_is_clean() {
    let source = "export default function one(x: string): string;\n\
                  export default function one(x: any): any { return x; }\n";
    assert_family_exactly(source, &[]);
}

// -- TS2384: the implementation's ambient status is canonical ---------------

/// A lone `declare` signature above a non-ambient implementation deviates:
/// the implementation is canonical, exactly as on the export axis.
///
/// ```text
/// tsc: (1,18) TS2384
/// ```
#[test]
fn declare_signature_plain_impl_reports_ts2384_at_signature() {
    let source = "declare function probe(a: string): void;\n\
                  function probe(a: any): any { return a; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2384).at(1, 18)]);
}

/// Module-context variant of the same shape.
#[test]
fn declare_signature_plain_impl_in_module_reports_ts2384() {
    let source = "declare function gate(a: string): void;\n\
                  function gate(a: any): any { return a; }\n\
                  export {};\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2384).at(1, 18)]);
}

/// Both `declare` signatures deviate from the non-ambient implementation.
///
/// ```text
/// tsc: (1,18) TS2384, (2,18) TS2384
/// ```
#[test]
fn two_declare_signatures_plain_impl_blame_both_signatures() {
    let source = "declare function dual(a: string): void;\n\
                  declare function dual(a: number): void;\n\
                  function dual(a: any): any { return a; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2384).at(1, 18),
            DiagnosticShape::code(2384).at(2, 18),
        ],
    );
}

/// Mixed ambient signatures: only the `declare` one deviates from the
/// non-ambient implementation.
///
/// ```text
/// tsc: (2,18) TS2384
/// ```
#[test]
fn mixed_ambient_signatures_blame_only_the_declare_one() {
    let source = "function vary(x: string): string;\n\
                  declare function vary(x: number): number;\n\
                  function vary(x: any): any { return x; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2384).at(2, 18)]);
}

// -- Precedence: export deviation wins over ambient deviation ---------------

/// A `declare` signature in an exported group deviates on both axes but
/// reports only `TS2383` — tsc's else-if chain gives export precedence.
///
/// ```text
/// tsc: (1,18) TS2383 (no TS2384)
/// ```
#[test]
fn export_deviation_takes_precedence_over_ambient_deviation() {
    let source = "declare function pick(x: string): string;\n\
                  export function pick(x: number): number;\n\
                  export function pick(x: any): any { return x; }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(1, 18)]);
}

// -- Non-function members of the merged symbol are blamed too ---------------

/// An exported namespace merged into a mixed function group deviates from
/// the canonical implementation and is blamed, alongside the
/// merged-declaration visibility family the namespace re-enables.
///
/// ```text
/// tsc: (1,17) TS2383+TS2395, (2,10) TS2395, (3,18) TS2383+TS2395
/// ```
#[test]
fn exported_namespace_in_mixed_function_group_is_blamed() {
    let source = "export function core(x: string): string;\n\
                  function core(x: any): any { return x; }\n\
                  export namespace core { export const y = 1; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2383).at(1, 17),
            DiagnosticShape::code(2395).at(1, 17),
            DiagnosticShape::code(2395).at(2, 10),
            DiagnosticShape::code(2383).at(3, 18),
            DiagnosticShape::code(2395).at(3, 18),
        ],
    );
}

/// An ambient-only mismatch among the function-likes still blames a merged
/// namespace whose export status deviates from the canonical implementation.
///
/// ```text
/// tsc: (1,18) TS2384+TS2395, (2,10) TS2395, (3,18) TS2383+TS2395
/// ```
#[test]
fn ambient_mismatch_still_blames_export_deviating_namespace() {
    let source = "declare function haze(x: string): string;\n\
                  function haze(x: any): any { return x; }\n\
                  export namespace haze { export const y = 1; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2384).at(1, 18),
            DiagnosticShape::code(2395).at(1, 18),
            DiagnosticShape::code(2395).at(2, 10),
            DiagnosticShape::code(2383).at(3, 18),
            DiagnosticShape::code(2395).at(3, 18),
        ],
    );
}

// -- TS2652 stays for genuinely merged declarations -------------------------

/// A default-exported function merged with an instantiated namespace is a
/// merged declaration: `TS2652` at both value-space contributors, unchanged.
///
/// ```text
/// tsc: (1,25) TS2652, (2,11) TS2652
/// ```
#[test]
fn default_function_with_instantiated_namespace_keeps_ts2652() {
    let source = "export default function join(a: string): string { return a; }\n\
                  namespace join { export const x = 1; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2652).at(1, 25),
            DiagnosticShape::code(2652).at(2, 11),
        ],
    );
}

/// A default-exported function against a same-named `var` is a merged
/// declaration, not an overload group: `TS2652` at both.
///
/// ```text
/// tsc: (1,25) TS2652, (2,5) TS2652
/// ```
#[test]
fn default_function_with_var_keeps_ts2652() {
    let source = "export default function knot(a: string): string { return a; }\n\
                  var knot = 1;\n\
                  export {};\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2652).at(1, 25),
            DiagnosticShape::code(2652).at(2, 5),
        ],
    );
}

/// A namespace in an otherwise-pure default-signature group restores the
/// whole merged-declaration family: every value-space contributor gets
/// `TS2652`, and the deviating signature still gets its `TS2383`.
///
/// ```text
/// tsc: (1,25) TS2383+TS2652, (2,11) TS2652, (3,10) TS2652
///      (TS2391/TS2434 are other-family)
/// ```
#[test]
fn namespace_restores_ts2652_for_default_signature_group() {
    let source = "export default function blend(a: string): string;\n\
                  namespace blend { export const x = 1; }\n\
                  function blend(a: string): string { return a; }\n";
    assert_family_exactly(
        source,
        &[
            DiagnosticShape::code(2383).at(1, 25),
            DiagnosticShape::code(2652).at(1, 25),
            DiagnosticShape::code(2652).at(2, 11),
            DiagnosticShape::code(2652).at(3, 10),
        ],
    );
}

// -- Namespace bodies and ambient containers --------------------------------

/// Inside a non-ambient namespace body the same implementation-canonical
/// rule applies.
///
/// ```text
/// tsc: (2,21) TS2383
/// ```
#[test]
fn namespace_body_exported_signature_plain_impl_reports_ts2383() {
    let source = "namespace Depot {\n\
                  \x20   export function load(x: string): string;\n\
                  \x20   function load(x: any): any { return x; }\n\
                  }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(2, 21)]);
}

/// Cross-container groups fall back to the first overload as canonical
/// (`overloadsInDifferentContainersDisagreeOnAmbient.ts`): an ambient
/// signature in a `declare namespace` block is canonical for the merged
/// symbol, so the non-ambient implementation in the sibling non-ambient
/// block is the deviator.
///
/// ```text
/// tsc: (5,21) TS2384
/// ```
#[test]
fn cross_container_ambient_disagreement_blames_the_implementation() {
    let source = "declare namespace Realm {\n\
                  \x20   export function act(): void;\n\
                  }\n\
                  namespace Realm {\n\
                  \x20   export function act(): void { }\n\
                  }\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2384).at(5, 21)]);
}

/// Members of an ambient namespace body are implicitly exported; mixed
/// `export` keywords there are not overload-consistency errors.
#[test]
fn ambient_namespace_body_mixed_export_stays_exempt() {
    let source = "declare namespace Vault {\n\
                  \x20   function open(): void;\n\
                  \x20   export function open(): void;\n\
                  \x20   function open(): void;\n\
                  }\n";
    assert_family_exactly(source, &[]);
}

// -- No-implementation groups: canonical is the FIRST signature in SOURCE
// -- order, so the deviator is always the later one (#16742 follow-up). The
// -- binder's declaration push order differs from source order for an
// -- `export default`-wrapped member, which mis-anchored the first row.

/// A default-exported signature above a plain one: the plain signature
/// deviates.
///
/// ```text
/// tsc: (2,10) TS2383 (+ TS2391, outside this filter)
/// ```
#[test]
fn no_impl_default_then_plain_blames_the_plain_signature() {
    let source = "export default function Execute(): void;\n\
                  function Execute(): void;\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(2, 10)]);
}

/// Reverse order: the default-exported signature deviates from the plain
/// canonical one.
///
/// ```text
/// tsc: (2,25) TS2383
/// ```
#[test]
fn no_impl_plain_then_default_blames_the_default_signature() {
    let source = "function Launch(): void;\n\
                  export default function Launch(): void;\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(2, 25)]);
}

/// Named-export variant: the plain signature deviates from the exported
/// canonical one.
///
/// ```text
/// tsc: (2,10) TS2383
/// ```
#[test]
fn no_impl_export_then_plain_blames_the_plain_signature() {
    let source = "export function relay(x: string): string;\n\
                  function relay(x: number): number;\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(2, 10)]);
}

/// Named-export variant, reverse order: the exported signature deviates.
///
/// ```text
/// tsc: (2,17) TS2383
/// ```
#[test]
fn no_impl_plain_then_export_blames_the_export_signature() {
    let source = "function relay(x: string): string;\n\
                  export function relay(x: number): number;\n";
    assert_family_exactly(source, &[DiagnosticShape::code(2383).at(2, 17)]);
}
